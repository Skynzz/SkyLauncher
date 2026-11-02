use crate::downloader::modloaders::ModloaderType;
use crate::instance::{InstanceConfig, InstanceManager, InstanceSource};
use chrono::Utc;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Scans a CurseForge `Instances` directory and adapts all valid subfolders containing
/// `minecraftinstance.json` or `manifest.json`.
pub fn scan_curseforge_directory(
    cf_instances_root: &Path,
    manager: &InstanceManager,
) -> Result<Vec<InstanceConfig>, String> {
    if !cf_instances_root.exists() {
        return Ok(Vec::new());
    }

    let mut adapted = Vec::new();
    let entries = fs::read_dir(cf_instances_root).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let subfolder = entry.path();
        if !subfolder.is_dir() {
            continue;
        }
        if let Some(config) = parse_curseforge_instance_folder(&subfolder) {
            if let Ok((saved, _is_new)) = manager.upsert_curseforge_instance(config) {
                adapted.push(saved);
            }
        }
    }

    Ok(adapted)
}

/// Parses a single CurseForge instance folder (`minecraftinstance.json` or `manifest.json`)
/// to extract `baseModLoader`, `gameVersion`, instance name, and `allocatedMemory`.
pub fn parse_curseforge_instance_folder(folder: &Path) -> Option<InstanceConfig> {
    let mc_instance_json = folder.join("minecraftinstance.json");
    let manifest_json = folder.join("manifest.json");

    let folder_name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("CurseForge Pack")
        .to_string();

    let slug: String = folder_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let instance_id = format!("cf-{}", slug.trim_matches('-'));

    // Priority 1: `minecraftinstance.json` (Created directly by the CurseForge App)
    if mc_instance_json.exists() {
        if let Ok(raw) = fs::read_to_string(&mc_instance_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                let name = val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&folder_name)
                    .to_string();

                let mc_version = val
                    .get("gameVersion")
                    .and_then(|v| v.as_str())
                    .or_else(|| val.pointer("/baseModLoader/minecraftVersion").and_then(|v| v.as_str()))
                    .unwrap_or("1.20.1")
                    .to_string();

                let allocated_memory = val
                    .get("allocatedMemory")
                    .and_then(|v| v.as_u64())
                    .map(|m| m as u32)
                    .unwrap_or(4096);

                let is_memory_override = val
                    .get("isMemoryOverride")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let (modloader, modloader_version) =
                    if let Some(bml) = val.get("baseModLoader") {
                        let bml_name = bml.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let forge_ver = bml.get("forgeVersion").and_then(|v| v.as_str());
                        let loader_type = ModloaderType::from_str_loose(bml_name);

                        let ver = forge_ver
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| s.to_string())
                            .or_else(|| {
                                let lower = bml_name.to_lowercase();
                                let stripped = if let Some(rest) = lower.strip_prefix("neoforge-") {
                                    rest
                                } else if let Some(rest) = lower.strip_prefix("forge-") {
                                    rest
                                } else if let Some(rest) = lower.strip_prefix("fabric-") {
                                    rest
                                } else if let Some(rest) = lower.strip_prefix("quilt-") {
                                    rest
                                } else {
                                    &lower
                                };
                                let clean = stripped.strip_prefix(&format!("{}-", mc_version)).unwrap_or(stripped);
                                if clean.is_empty() {
                                    None
                                } else {
                                    Some(clean.to_string())
                                }
                            });
                        (loader_type, ver)
                    } else {
                        (ModloaderType::Vanilla, None)
                    };

                return Some(InstanceConfig {
                    id: instance_id,
                    name,
                    mc_version,
                    modloader,
                    modloader_version,
                    source: InstanceSource::CurseForge,
                    game_dir: folder.to_path_buf(),
                    icon_color: "#f97316".to_string(),
                    icon_data_url: None,
                    override_memory: is_memory_override,
                    min_ram_mb: 1024,
                    max_ram_mb: allocated_memory.max(2048),
                    override_java: false,
                    java_path: None,
                    custom_jvm_args: Vec::new(),
                    created_at: Utc::now(),
                    last_played: None,
                    playtime_minutes: 0,
                });
            }
        }
    }

    // Priority 2: `manifest.json` (Standard CurseForge modpack manifest)
    if manifest_json.exists() {
        if let Ok(raw) = fs::read_to_string(&manifest_json) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                let name = val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&folder_name)
                    .to_string();

                let mc_version = val
                    .pointer("/minecraft/version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.20.1")
                    .to_string();

                let recommended_ram = val
                    .pointer("/minecraft/recommendedRam")
                    .and_then(|v| v.as_u64())
                    .map(|m| m as u32)
                    .unwrap_or(4096);

                let mut modloader = ModloaderType::Vanilla;
                let mut modloader_version = None;

                if let Some(loaders) = val
                    .pointer("/minecraft/modLoaders")
                    .and_then(|v| v.as_array())
                {
                    if let Some(primary) = loaders.first() {
                        if let Some(id_str) = primary.get("id").and_then(|v| v.as_str()) {
                            modloader = ModloaderType::from_str_loose(id_str);
                            let lower = id_str.to_lowercase();
                            let stripped = if let Some(rest) = lower.strip_prefix("neoforge-") {
                                rest
                            } else if let Some(rest) = lower.strip_prefix("forge-") {
                                rest
                            } else if let Some(rest) = lower.strip_prefix("fabric-") {
                                rest
                            } else if let Some(rest) = lower.strip_prefix("quilt-") {
                                rest
                            } else {
                                &lower
                            };
                            let clean = stripped.strip_prefix(&format!("{}-", mc_version)).unwrap_or(stripped);
                            if !clean.is_empty() {
                                modloader_version = Some(clean.to_string());
                            }
                        }
                    }
                }

                return Some(InstanceConfig {
                    id: instance_id,
                    name,
                    mc_version,
                    modloader,
                    modloader_version,
                    source: InstanceSource::CurseForge,
                    game_dir: folder.to_path_buf(),
                    icon_color: "#f97316".to_string(),
                    icon_data_url: None,
                    override_memory: true,
                    min_ram_mb: 1024,
                    max_ram_mb: recommended_ram.max(2048),
                    override_java: false,
                    java_path: None,
                    custom_jvm_args: Vec::new(),
                    created_at: Utc::now(),
                    last_played: None,
                    playtime_minutes: 0,
                });
            }
        }
    }

    None
}

/// Spawns a real-time filesystem watcher using the `notify` crate on the user's CurseForge
/// `Instances` directory and emits `curseforge-instance-detected` events to the frontend.
pub fn start_curseforge_watcher(
    watch_dir: PathBuf,
    manager: Arc<InstanceManager>,
    app_handle: AppHandle,
) -> Option<RecommendedWatcher> {
    if !watch_dir.exists() {
        return None;
    }

    // Perform initial scan
    if let Ok(initial) = scan_curseforge_directory(&watch_dir, &manager) {
        for inst in initial {
            let _ = app_handle.emit("curseforge-instance-detected", &inst);
        }
    }

    let mgr_clone = manager.clone();
    let app_clone = app_handle.clone();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default();
                    if file_name == "minecraftinstance.json" || file_name == "manifest.json" {
                        if let Some(parent) = path.parent() {
                            if let Some(cfg) = parse_curseforge_instance_folder(parent) {
                                if let Ok((saved, _)) = mgr_clone.upsert_curseforge_instance(cfg) {
                                    let _ = app_clone.emit("curseforge-instance-detected", &saved);
                                }
                            }
                        }
                    }
                }
            }
        },
        Config::default(),
    )
    .ok()?;

    if watcher.watch(&watch_dir, RecursiveMode::Recursive).is_ok() {
        Some(watcher)
    } else {
        None
    }
}
