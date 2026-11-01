use crate::auth::{build_offline_auth_args, OfflineProfile};
use crate::crash::crash_analyzer::analyze_instance_crash;
use crate::downloader::modloaders::{download_adoptium_jre, resolve_modloader_profile};
use crate::downloader::mojang::{
    collect_libraries_and_classpath, extract_natives, resolve_vanilla_version,
};
use crate::downloader::{DownloadEngine, DownloadProgressEvent};
use crate::instance::{GlobalSettings, InstanceConfig};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedJavaRuntime {
    pub path: String,
    pub major_version: u32,
    pub vendor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveLaunchSettings {
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub required_java_major: u32,
    pub resolved_java_path: String,
    pub jvm_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreLaunchCleanupSummary {
    pub removed_session_locks: usize,
    pub terminated_zombie_pids: Vec<u32>,
}

pub fn required_java_version_for_mc(mc_version: &str) -> u32 {
    let v = mc_version.trim();
    if let Some(rest) = v.strip_prefix("1.") {
        let parts: Vec<&str> = rest.split('.').collect();
        let minor: u32 = parts
            .first()
            .and_then(|s| {
                s.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .unwrap_or(20);
        let patch: u32 = parts
            .get(1)
            .and_then(|s| {
                s.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .unwrap_or(0);

        if minor <= 16 {
            return 8;
        } else if minor < 20 || (minor == 20 && patch <= 4) {
            return 17;
        } else {
            return 21;
        }
    }

    if v.starts_with("b1.") || v.starts_with("a1.") || v.starts_with("c0.") {
        return 8;
    }

    21
}

/// Resolves memory and strictly matches the exact required Java major version (8, 17, or 21)
/// so older Forge/Modlauncher versions never crash due to running on a newer incompatible Java version.
pub fn resolve_effective_memory_and_java(
    global: &GlobalSettings,
    instance: &InstanceConfig,
    app_data_dir: &Path,
) -> (u32, u32, u32, Option<String>) {
    let min_ram = if instance.override_memory {
        instance.min_ram_mb
    } else {
        global.default_min_ram_mb
    };
    let max_ram = if instance.override_memory {
        instance.max_ram_mb
    } else {
        global.default_max_ram_mb
    };

    let required_java = required_java_version_for_mc(&instance.mc_version);

    let explicit_java = if instance.override_java {
        instance.java_path.clone().filter(|s| !s.trim().is_empty())
    } else {
        global
            .default_java_path
            .clone()
            .filter(|s| !s.trim().is_empty())
    };

    if explicit_java.is_some() {
        return (min_ram, max_ram.max(min_ram), required_java, explicit_java);
    }

    let runtimes = discover_java_runtimes(app_data_dir);
    // Strictly match the exact required Java major version (8, 17, or 21)
    let matched = runtimes
        .iter()
        .find(|r| r.major_version == required_java)
        .map(|r| r.path.clone());

    (min_ram, max_ram.max(min_ram), required_java, matched)
}

pub fn build_jvm_flags(
    min_ram_mb: u32,
    max_ram_mb: u32,
    java_major: u32,
    natives_dir: &Path,
    global_args: &[String],
    instance_args: &[String],
) -> Vec<String> {
    let mut flags = vec![
        format!("-Xms{}M", min_ram_mb),
        format!("-Xmx{}M", max_ram_mb.max(min_ram_mb)),
    ];

    if java_major >= 21 {
        flags.extend([
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+UseZGC".to_string(),
            "-XX:+ZGenerational".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
            "-XX:+PerfDisableSharedMem".to_string(),
        ]);
    } else {
        flags.extend([
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+UseG1GC".to_string(),
            "-XX:+ParallelRefProcEnabled".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:G1NewSizePercent=30".to_string(),
            "-XX:G1MaxNewSizePercent=40".to_string(),
            "-XX:G1HeapRegionSize=8M".to_string(),
            "-XX:G1ReservePercent=20".to_string(),
            "-XX:G1HeapWastePercent=5".to_string(),
            "-XX:G1MixedGCCountTarget=4".to_string(),
            "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
            "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
            "-XX:G1RSetUpdatingPauseTimePercent=5".to_string(),
            "-XX:SurvivorRatio=32".to_string(),
            "-XX:+PerfDisableSharedMem".to_string(),
            "-XX:MaxTenuringThreshold=1".to_string(),
            "-Dusing.aikars.flags=https://mcflags.emc.gs".to_string(),
            "-Daikars.new.flags=true".to_string(),
        ]);
    }

    flags.push("-Dlog4j2.formatMsgNoLookups=true".to_string());
    flags.push(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));
    flags.push(format!(
        "-Dorg.lwjgl.librarypath={}",
        natives_dir.to_string_lossy()
    ));

    let sanitized_global = crate::security::sanitize_jvm_arguments(global_args);
    for arg in sanitized_global {
        if !flags.contains(&arg) {
            flags.push(arg);
        }
    }
    let sanitized_instance = crate::security::sanitize_jvm_arguments(instance_args);
    for arg in sanitized_instance {
        if !flags.contains(&arg) {
            flags.push(arg);
        }
    }

    flags
}

pub fn discover_java_runtimes(app_data_dir: &Path) -> Vec<DetectedJavaRuntime> {
    let mut results = Vec::new();
    let mut candidate_roots: Vec<PathBuf> = vec![app_data_dir.join("runtimes")];

    if let Some(home) = dirs::home_dir() {
        let cf_runtime = home
            .join("curseforge")
            .join("minecraft")
            .join("Install")
            .join("runtime");
        if cf_runtime.exists() {
            candidate_roots.push(cf_runtime);
        }
    }

    if cfg!(target_os = "windows") {
        for base in [
            r"C:\Program Files\Eclipse Adoptium",
            r"C:\Program Files\Java",
            r"C:\Program Files\Microsoft",
            r"C:\Program Files\Zulu",
            r"C:\Program Files\BellSoft",
            r"C:\Program Files (x86)\Minecraft Launcher\runtime",
        ] {
            let p = PathBuf::from(base);
            if p.exists() {
                candidate_roots.push(p);
            }
        }
    } else if cfg!(target_os = "macos") {
        candidate_roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    } else {
        candidate_roots.push(PathBuf::from("/usr/lib/jvm"));
    }

    let bin_name = if cfg!(target_os = "windows") {
        "javaw.exe"
    } else {
        "java"
    };

    for root in candidate_roots {
        scan_java_dir_recursive(&root, bin_name, 0, &mut results);
    }

    results.sort_by(|a, b| b.major_version.cmp(&a.major_version));
    results.dedup_by(|a, b| a.path == b.path);
    results
}

fn scan_java_dir_recursive(
    dir: &Path,
    bin_name: &str,
    depth: usize,
    out: &mut Vec<DetectedJavaRuntime>,
) {
    if depth > 4 || !dir.exists() {
        return;
    }

    let bin_candidate = dir.join("bin").join(bin_name);
    if bin_candidate.exists() {
        if let Some(major) = infer_verified_64bit_java_major(dir) {
            out.push(DetectedJavaRuntime {
                path: bin_candidate.to_string_lossy().to_string(),
                major_version: major,
                vendor: dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Local JRE")
                    .to_string(),
            });
        }
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                scan_java_dir_recursive(&p, bin_name, depth + 1, out);
            }
        }
    }
}

fn infer_verified_64bit_java_major(java_home: &Path) -> Option<u32> {
    let release_file = java_home.join("release");
    if let Ok(content) = fs::read_to_string(&release_file) {
        if content.contains("OS_ARCH=\"x86\"") || content.contains("OS_ARCH=\"i386\"") {
            return None;
        }
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("JAVA_VERSION=") {
                let ver = rest.trim_matches('"');
                if ver.starts_with("1.8") || ver.starts_with("8.") {
                    return Some(8);
                } else if ver.starts_with("17") {
                    return Some(17);
                } else if ver.starts_with("21") {
                    return Some(21);
                } else if let Some(first_num) = ver.split('.').next() {
                    if let Ok(n) = first_num.parse::<u32>() {
                        return Some(n);
                    }
                }
            }
        }
    }

    let folder_str = java_home.to_string_lossy().to_lowercase();
    if folder_str.contains("program files (x86)") && !folder_str.contains("minecraft launcher") {
        return None;
    }
    if folder_str.contains("21") || folder_str.contains("jre-delta") {
        Some(21)
    } else if folder_str.contains("17") || folder_str.contains("jre-gamma") {
        Some(17)
    } else if folder_str.contains("1.8")
        || folder_str.contains("java-8")
        || folder_str.contains("jre-legacy")
    {
        Some(8)
    } else {
        None
    }
}

pub fn cleanup_prelaunch_environment(game_dir: &Path) -> PreLaunchCleanupSummary {
    let mut removed_session_locks = 0;

    let root_lock = game_dir.join("session.lock");
    if root_lock.exists() && fs::remove_file(&root_lock).is_ok() {
        removed_session_locks += 1;
    }

    let saves_dir = game_dir.join("saves");
    if let Ok(entries) = fs::read_dir(&saves_dir) {
        for entry in entries.flatten() {
            let lock_file = entry.path().join("session.lock");
            if lock_file.exists() && fs::remove_file(&lock_file).is_ok() {
                removed_session_locks += 1;
            }
        }
    }

    let mut terminated_zombie_pids = Vec::new();
    let game_dir_str = game_dir.to_string_lossy().to_lowercase();
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for (pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains("java") {
            let cmd_joined = process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            if cmd_joined.contains(&game_dir_str) && process.kill() {
                terminated_zombie_pids.push(pid.as_u32());
            }
        }
    }

    PreLaunchCleanupSummary {
        removed_session_locks,
        terminated_zombie_pids,
    }
}

/// Terminates a running Minecraft process by PID or instance game_dir.
pub fn stop_running_instance(
    instance_id: &str,
    game_dir: &Path,
    running_pids: &Mutex<HashMap<String, u32>>,
) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut killed = false;
    if let Some(pid_num) = running_pids.lock().remove(instance_id) {
        let pid = Pid::from_u32(pid_num);
        if let Some(proc) = sys.process(pid) {
            killed = proc.kill();
        }
    }

    let clean_summary = cleanup_prelaunch_environment(game_dir);
    killed || !clean_summary.terminated_zombie_pids.is_empty()
}

/// Extracts and expands string arguments from `arguments.jvm` or `arguments.game` in a Modloader JSON.
fn extract_modloader_args(
    modloader_json: &Value,
    section: &str,
    library_dir: &Path,
    natives_dir: &Path,
    version_name: &str,
    mc_version: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    let cp_sep = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    let lib_dir_str = library_dir.to_string_lossy();
    let nat_dir_str = natives_dir.to_string_lossy();

    if let Some(arr) = modloader_json
        .pointer(&format!("/arguments/{}", section))
        .and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(s) = item.as_str() {
                let mut expanded = s
                    .replace("${library_directory}", &lib_dir_str)
                    .replace("${natives_directory}", &nat_dir_str)
                    .replace("${classpath_separator}", cp_sep)
                    .replace("${version_name}", version_name);

                // Ensure BootstrapLauncher ignoreList also ignores the Vanilla `<mc_version>.jar`
                if expanded.starts_with("-DignoreList=") {
                    let mc_jar = format!("{}.jar", mc_version);
                    if !expanded.contains(&mc_jar) {
                        expanded.push(',');
                        expanded.push_str(&mc_jar);
                    }
                }
                out.push(expanded);
            }
        }
    }

    // Legacy 1.12.2 / 1.7.10 Forge `minecraftArguments` (`--tweakClass ...`)
    if section == "game" && out.is_empty() {
        if let Some(mc_args_str) = modloader_json
            .get("minecraftArguments")
            .and_then(|v| v.as_str())
        {
            let tokens: Vec<&str> = mc_args_str.split_whitespace().collect();
            for i in 0..tokens.len() {
                if tokens[i] == "--tweakClass" {
                    if let Some(tweak_val) = tokens.get(i + 1) {
                        out.push("--tweakClass".to_string());
                        out.push((*tweak_val).to_string());
                    }
                }
            }
        }
    }

    out
}

pub async fn launch_instance_process(
    app_handle: AppHandle,
    engine: &DownloadEngine,
    instance: InstanceConfig,
    global: GlobalSettings,
    profile: OfflineProfile,
    app_data_dir: &Path,
    running_pids: Arc<Mutex<HashMap<String, u32>>>,
    quick_play_world: Option<String>,
) -> Result<EffectiveLaunchSettings, String> {
    engine.reset_cancel();
    let _cleanup = cleanup_prelaunch_environment(&instance.game_dir);

    // Stage 1/4: Java Runtime Check / Download
    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: instance.id.clone(),
            completed_files: 0,
            total_files: 1,
            current_label: format!("Java kontrol ediliyor (MC {})", instance.mc_version),
            phase: "java-runtime".to_string(),
            stage_text: "Aşama 1/4: Java çalışma zamanı hazırlanıyor...".to_string(),
        },
    );

    let (min_ram, max_ram, required_java, mut java_path_opt) =
        resolve_effective_memory_and_java(&global, &instance, app_data_dir);

    if java_path_opt.is_none() {
        let _ = app_handle.emit(
            "download-progress",
            DownloadProgressEvent {
                instance_id: instance.id.clone(),
                completed_files: 0,
                total_files: 1,
                current_label: format!("Java {} indiriliyor...", required_java),
                phase: "java-runtime".to_string(),
                stage_text: format!(
                    "Aşama 1/4: Java {} çalışma zamanı indiriliyor ve kuruluyor...",
                    required_java
                ),
            },
        );
        let downloaded_jre = download_adoptium_jre(engine, required_java, app_data_dir).await?;
        java_path_opt = Some(downloaded_jre.to_string_lossy().to_string());
    }

    if engine.is_cancelled() {
        let _ = app_handle.emit("download-finished", &instance.id);
        return Err("Başlatma iptal edildi.".to_string());
    }

    let java_bin = java_path_opt.unwrap_or_else(|| "javaw".to_string());

    // Unify shared_root: If this is a CurseForge instance and `<curseforge>/minecraft/Install` exists,
    // use `<curseforge>/minecraft/Install` directly so `-p` (Module Path) and `-cp` (Classpath)
    // share the exact same `libraries/` directory and all pre-built Forge/NeoForge SRG jars exist!
    let cf_install_root_opt = instance
        .game_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|m| m.join("Install"))
        .filter(|p| p.exists());

    let shared_root = cf_install_root_opt.unwrap_or_else(|| app_data_dir.join("shared"));

    // Stage 2/4: Resolve Vanilla & Modloader Profiles
    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: instance.id.clone(),
            completed_files: 0,
            total_files: 1,
            current_label: format!("{} ({:?})", instance.mc_version, instance.modloader),
            phase: "profile-resolve".to_string(),
            stage_text: "Aşama 2/4: Oyun ve Mod yükleyici profili çözülüyor...".to_string(),
        },
    );

    let (vanilla_json, mut tasks) =
        resolve_vanilla_version(engine, &instance.mc_version, &shared_root).await?;

    let modloader_json = if let Some(ref ml_ver) = instance.modloader_version {
        resolve_modloader_profile(
            engine,
            instance.modloader,
            &instance.mc_version,
            ml_ver,
            &shared_root,
            &instance.game_dir,
            &java_bin,
        )
        .await?
    } else {
        None
    };

    if engine.is_cancelled() {
        let _ = app_handle.emit("download-finished", &instance.id);
        return Err("Başlatma iptal edildi.".to_string());
    }

    let (lib_tasks, mut classpath_jars, native_jars) = collect_libraries_and_classpath(
        &vanilla_json,
        modloader_json.as_ref(),
        &instance.mc_version,
        &shared_root,
    );
    tasks.extend(lib_tasks);

    // Stage 3/4: Verify & Heal Libraries and Assets
    let inst_id_for_prog = instance.id.clone();
    let app_for_prog = app_handle.clone();
    engine
        .verify_and_heal_batch(tasks, move |done, total, label| {
            let _ = app_for_prog.emit(
                "download-progress",
                DownloadProgressEvent {
                    instance_id: inst_id_for_prog.clone(),
                    completed_files: done,
                    total_files: total,
                    current_label: label.clone(),
                    phase: "libraries-assets".to_string(),
                    stage_text: format!(
                        "Aşama 3/4: Oyun dosyaları indiriliyor ({}/{}) — {}",
                        done, total, label
                    ),
                },
            );
        })
        .await?;

    // Stage 4/4: Extract Natives & Spawn Game
    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: instance.id.clone(),
            completed_files: 1,
            total_files: 1,
            current_label: "Minecraft başlatılıyor...".to_string(),
            phase: "launching".to_string(),
            stage_text: "Aşama 4/4: Native kütüphaneler çıkarılıyor ve oyun açılıyor...".to_string(),
        },
    );

    let natives_dir = shared_root
        .join("versions")
        .join(&instance.mc_version)
        .join("natives");
    let _ = extract_natives(&native_jars, &natives_dir);

    let _ = app_handle.emit("download-finished", &instance.id);

    classpath_jars.retain(|p| p.exists());

    let jvm_flags = build_jvm_flags(
        min_ram,
        max_ram,
        required_java,
        &natives_dir,
        &global.default_jvm_args,
        &instance.custom_jvm_args,
    );

    let cp_separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };

    let main_class = modloader_json
        .as_ref()
        .and_then(|j| j.get("mainClass"))
        .and_then(|v| v.as_str())
        .or_else(|| vanilla_json.get("mainClass").and_then(|v| v.as_str()))
        .unwrap_or("net.minecraft.client.main.Main")
        .to_string();

    let asset_index_id = vanilla_json
        .pointer("/assetIndex/id")
        .and_then(|v| v.as_str())
        .unwrap_or(&instance.mc_version)
        .to_string();

    let effective_lib_dir = shared_root.join("libraries");

    let mut full_args = jvm_flags.clone();
    let mut module_path_jar_names: HashSet<String> = HashSet::new();

    if let Some(ref ml_json) = modloader_json {
        let ml_ver_id = ml_json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(&instance.mc_version);
        let extra_jvm = extract_modloader_args(
            ml_json,
            "jvm",
            &effective_lib_dir,
            &natives_dir,
            ml_ver_id,
            &instance.mc_version,
        );

        // Collect all JAR filenames placed on `-p` (`--module-path`) so we exclude them from `-cp`
        for idx in 0..extra_jvm.len() {
            if extra_jvm[idx] == "-p" || extra_jvm[idx] == "--module-path" {
                if let Some(mp_val) = extra_jvm.get(idx + 1) {
                    for entry in mp_val.split(cp_separator) {
                        if let Some(fname) = Path::new(entry).file_name().and_then(|n| n.to_str()) {
                            module_path_jar_names.insert(fname.to_lowercase());
                        }
                    }
                }
            }
        }

        full_args.extend(extra_jvm);
    }

    // Remove any JAR from `-cp` that is already loaded on the JVM `-p` module path!
    if !module_path_jar_names.is_empty() {
        classpath_jars.retain(|jar_path| {
            let fname = jar_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            !module_path_jar_names.contains(&fname)
        });
    }

    let classpath_str = classpath_jars
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(cp_separator);

    full_args.push("-cp".to_string());
    full_args.push(classpath_str);
    full_args.push(main_class);

    full_args.extend([
        "--version".to_string(),
        instance.mc_version.clone(),
        "--gameDir".to_string(),
        instance.game_dir.to_string_lossy().to_string(),
        "--assetsDir".to_string(),
        shared_root.join("assets").to_string_lossy().to_string(),
        "--assetIndex".to_string(),
        asset_index_id,
    ]);

    for (flag, val) in build_offline_auth_args(&profile) {
        full_args.push(flag);
        full_args.push(val);
    }

    if let Some(ref ml_json) = modloader_json {
        let ml_ver_id = ml_json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(&instance.mc_version);
        let extra_game = extract_modloader_args(
            ml_json,
            "game",
            &effective_lib_dir,
            &natives_dir,
            ml_ver_id,
            &instance.mc_version,
        );
        full_args.extend(extra_game);
    }

    if let Some(world_folder) = quick_play_world.filter(|w| !w.trim().is_empty()) {
        full_args.push("--quickPlaySingleplayer".to_string());
        full_args.push(world_folder);
    }

    let _ = fs::create_dir_all(&instance.game_dir);
    let mut child = Command::new(&java_bin)
        .args(&full_args)
        .current_dir(&instance.game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Java başlatılamadı ('{}'): {}", java_bin, e))?;

    if let Some(pid) = child.id() {
        running_pids.lock().insert(instance.id.clone(), pid);
    }
    let session_start = std::time::Instant::now();
    let _ = app_handle.emit("game-started", &instance.id);

    if global.hide_launcher_on_launch {
        if let Some(win) = app_handle.get_webview_window("main") {
            let _ = win.hide();
        }
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let app_monitor = app_handle.clone();
    let inst_clone = instance.clone();
    let pids_ref = running_pids.clone();

    tokio::spawn(async move {
        let mut captured_log = String::new();

        if let Some(out) = stdout {
            let app_out = app_monitor.clone();
            let inst_id = inst_clone.id.clone();
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_out.emit("game-log-line", (&inst_id, &line));
                if captured_log.len() < 65536 {
                    captured_log.push_str(&line);
                    captured_log.push('\n');
                }
            }
        }

        if let Some(err) = stderr {
            let app_err = app_monitor.clone();
            let inst_id = inst_clone.id.clone();
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = app_err.emit("game-log-line", (&inst_id, &line));
                if captured_log.len() < 65536 {
                    captured_log.push_str(&line);
                    captured_log.push('\n');
                }
            }
        }

        let status = child.wait().await;
        let elapsed_mins = (session_start.elapsed().as_secs() / 60).max(1);
        let _ = app_monitor.emit("playtime-recorded", (&inst_clone.id, elapsed_mins));
        let was_tracked = pids_ref.lock().remove(&inst_clone.id).is_some();
        let exit_code = status.ok().and_then(|s| s.code()).unwrap_or(-1);

        if let Some(win) = app_monitor.get_webview_window("main") {
            let _ = win.show();
            let _ = win.set_focus();
        }

        // Only show crash modal if it wasn't intentionally stopped by the user
        if exit_code != 0 && was_tracked {
            let crash_report = analyze_instance_crash(
                &inst_clone.id,
                &inst_clone.game_dir,
                exit_code,
                &captured_log,
            );
            let _ = app_monitor.emit("game-crashed", &crash_report);
        } else {
            let _ = app_monitor.emit("game-exited-cleanly", &inst_clone.id);
        }
    });

    Ok(EffectiveLaunchSettings {
        min_ram_mb: min_ram,
        max_ram_mb: max_ram,
        required_java_major: required_java,
        resolved_java_path: java_bin,
        jvm_flags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_java_selector_and_jvm_flags() {
        assert_eq!(required_java_version_for_mc("1.8.9"), 8);
        assert_eq!(required_java_version_for_mc("1.12.2"), 8);
        assert_eq!(required_java_version_for_mc("1.16.5"), 8);
        assert_eq!(required_java_version_for_mc("1.17.1"), 17);
        assert_eq!(required_java_version_for_mc("1.20.1"), 17);
        assert_eq!(required_java_version_for_mc("1.20.4"), 17);
        assert_eq!(required_java_version_for_mc("1.20.5"), 21);
        assert_eq!(required_java_version_for_mc("1.21.1"), 21);

        let natives = PathBuf::from("/tmp/natives");
        let zgc_flags = build_jvm_flags(2048, 8192, 21, &natives, &[], &[]);
        assert!(zgc_flags.contains(&"-XX:+UseZGC".to_string()));
        assert!(zgc_flags.contains(&"-XX:+ZGenerational".to_string()));
        assert!(zgc_flags.contains(&"-Dlog4j2.formatMsgNoLookups=true".to_string()));

        let aikar_flags = build_jvm_flags(1024, 4096, 17, &natives, &[], &[]);
        assert!(aikar_flags.contains(&"-XX:+UseG1GC".to_string()));
        assert!(aikar_flags.contains(&"-XX:G1NewSizePercent=30".to_string()));
    }
}
