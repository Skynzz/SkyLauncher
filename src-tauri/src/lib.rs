pub mod auth;
pub mod crash;
pub mod downloader;
pub mod extensibility;
pub mod instance;
pub mod runner;
pub mod security;
pub mod updater;

use auth::{generate_offline_uuid, AuthStore, ProfileManager};
use crash::crash_analyzer::{
    analyze_instance_crash, backup_and_reset_config_file, parse_crash_text, CrashAnalysisReport,
};
use downloader::content_store::{
    apply_instance_mod_updates, apply_offline_skin_to_instance, check_instance_mod_updates,
    get_store_project_details, heal_missing_mod_dependencies, import_modpack_archive,
    install_fps_boost_suite, install_lan_tunnel_mod, install_modpack_from_store,
    install_store_content_with_dependencies, resolve_mod_id_to_slug, search_store_content,
    ContentType, ModUpdateInfo, StoreProjectDetails, StoreProvider, StoreSearchItem,
};
use downloader::modloaders::{
    download_adoptium_jre, fetch_modloader_versions, ModloaderType, ModloaderVersionItem,
};
use downloader::mojang::{fetch_version_manifest, MojangManifest};
use downloader::{DownloadEngine, DownloadProgressEvent, DownloadTask};
use extensibility::{ExtensibilityManager, PluginDescriptor, ThemeManifest};
use instance::curseforge_watcher::{scan_curseforge_directory, start_curseforge_watcher};
use instance::jar_parser::{
    delete_pack_resource, list_pack_resources, scan_instance_mods, toggle_mod_file, ModMetadata,
    PackResourceEntry,
};
use instance::pack_share::{
    base64_url_decode, clone_instance, create_world_backup, export_full_skypack,
    export_lightweight_skycode, export_server_pack, import_full_skypack,
    import_lightweight_skycode, list_instance_screenshots, list_instance_worlds,
    list_world_backups, optimize_shared_mod_pool, restore_world_backup,
    sync_options_txt_across_instances, ModPoolOptimizationReport, ScreenshotEntry,
    SkyCodeExportResult, WorldBackupEntry, WorldSaveEntry,
};
use instance::{GlobalSettings, InstanceConfig, InstanceManager, OFFICIAL_GITHUB_REPO};
use notify::RecommendedWatcher;
use parking_lot::Mutex;
use runner::{
    build_jvm_flags, discover_java_runtimes, launch_instance_process,
    resolve_effective_memory_and_java, stop_running_instance, DetectedJavaRuntime,
    EffectiveLaunchSettings,
};
use security::{scan_instance_security, verify_pe_executable, SecurityScanReport};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, State};
use updater::{
    check_github_update, cleanup_old_executable, download_and_apply_github_update,
    UpdateCheckResult, CURRENT_APP_VERSION,
};

pub struct AppState {
    pub app_data_dir: PathBuf,
    pub profile_manager: ProfileManager,
    pub instance_manager: Arc<InstanceManager>,
    pub download_engine: Arc<DownloadEngine>,
    pub extensibility_manager: ExtensibilityManager,
    pub cf_watcher: Mutex<Option<RecommendedWatcher>>,
    pub running_pids: Arc<Mutex<HashMap<String, u32>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemTelemetry {
    pub launcher_ram_mb: f64,
    pub system_total_ram_mb: u64,
    pub app_data_path: String,
}

#[tauri::command]
fn preview_offline_uuid(username: String) -> String {
    generate_offline_uuid(&username)
}

#[tauri::command]
fn get_auth_store(state: State<'_, AppState>) -> Result<AuthStore, String> {
    state.profile_manager.load_store()
}

#[tauri::command]
fn create_offline_profile(
    state: State<'_, AppState>,
    username: String,
    avatar_color: Option<String>,
) -> Result<AuthStore, String> {
    state
        .profile_manager
        .create_or_update_profile(&username, avatar_color)
}

#[tauri::command]
fn set_active_profile(state: State<'_, AppState>, profile_id: String) -> Result<AuthStore, String> {
    state.profile_manager.set_active_profile(&profile_id)
}

#[tauri::command]
fn delete_offline_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AuthStore, String> {
    state.profile_manager.delete_profile(&profile_id)
}

#[tauri::command]
async fn get_mojang_versions(
    state: State<'_, AppState>,
    filter_type: Option<String>,
) -> Result<MojangManifest, String> {
    fetch_version_manifest(&state.download_engine, filter_type.as_deref()).await
}

#[tauri::command]
async fn get_modloader_versions(
    state: State<'_, AppState>,
    loader_type: String,
    mc_version: String,
) -> Result<Vec<ModloaderVersionItem>, String> {
    let parsed_loader = ModloaderType::from_str_loose(&loader_type);
    fetch_modloader_versions(&state.download_engine, parsed_loader, &mc_version).await
}

#[tauri::command]
fn get_instances(state: State<'_, AppState>) -> Result<Vec<InstanceConfig>, String> {
    state.instance_manager.load_instances()
}

#[tauri::command]
fn create_instance(
    state: State<'_, AppState>,
    name: String,
    mc_version: String,
    modloader: String,
    modloader_version: Option<String>,
    override_memory: bool,
    min_ram_mb: Option<u32>,
    max_ram_mb: Option<u32>,
    icon_data_url: Option<String>,
) -> Result<InstanceConfig, String> {
    let loader_type = ModloaderType::from_str_loose(&modloader);
    let mut created = state.instance_manager.create_custom_instance(
        &name,
        &mc_version,
        loader_type,
        modloader_version,
        override_memory,
        min_ram_mb,
        max_ram_mb,
    )?;
    if let Some(icon) = icon_data_url.filter(|s| !s.trim().is_empty()) {
        created.icon_data_url = Some(icon);
        created = state.instance_manager.update_instance(created)?;
    }
    Ok(created)
}

#[tauri::command]
fn update_instance(
    state: State<'_, AppState>,
    instance: InstanceConfig,
) -> Result<InstanceConfig, String> {
    state.instance_manager.update_instance(instance)
}

#[tauri::command]
fn delete_instance(
    state: State<'_, AppState>,
    instance_id: String,
    delete_files: bool,
) -> Result<Vec<InstanceConfig>, String> {
    state
        .instance_manager
        .delete_instance(&instance_id, delete_files)?;
    state.instance_manager.load_instances()
}

#[tauri::command]
fn get_instance_mods(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<ModMetadata>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    scan_instance_mods(&inst.game_dir)
}

#[tauri::command]
fn toggle_instance_mod(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
) -> Result<Vec<ModMetadata>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let _new_name = toggle_mod_file(&inst.game_dir, &file_name)?;
    scan_instance_mods(&inst.game_dir)
}

#[tauri::command]
fn list_instance_resources(
    state: State<'_, AppState>,
    instance_id: String,
    category: String,
) -> Result<Vec<PackResourceEntry>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    list_pack_resources(&inst.game_dir, &category)
}

#[tauri::command]
fn get_global_settings(state: State<'_, AppState>) -> Result<GlobalSettings, String> {
    state.instance_manager.load_global_settings()
}

#[tauri::command]
fn save_global_settings(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    settings: GlobalSettings,
) -> Result<GlobalSettings, String> {
    state.instance_manager.save_global_settings(&settings)?;

    if let Some(ref cf_dir) = settings.curseforge_instances_dir {
        let path = PathBuf::from(cf_dir);
        if path.exists() {
            let new_watcher =
                start_curseforge_watcher(path, state.instance_manager.clone(), app_handle);
            *state.cf_watcher.lock() = new_watcher;
        }
    }

    Ok(settings)
}

#[tauri::command]
fn rescan_curseforge_instances(
    state: State<'_, AppState>,
    custom_dir: Option<String>,
) -> Result<Vec<InstanceConfig>, String> {
    let settings = state.instance_manager.load_global_settings()?;
    let target_dir = custom_dir
        .filter(|s| !s.trim().is_empty())
        .or(settings.curseforge_instances_dir)
        .or_else(|| {
            dirs::home_dir()
                .map(|h| h.join("curseforge").join("minecraft").join("Instances"))
                .filter(|p| p.exists())
                .and_then(|p| p.to_str().map(|s| s.to_string()))
        });
    if let Some(dir_str) = target_dir {
        let p = PathBuf::from(dir_str);
        if p.exists() {
            let _ = scan_curseforge_directory(&p, &state.instance_manager);
        }
    }
    state.instance_manager.load_instances()
}

#[tauri::command]
fn get_detected_java_runtimes(state: State<'_, AppState>) -> Vec<DetectedJavaRuntime> {
    discover_java_runtimes(&state.app_data_dir)
}

#[tauri::command]
fn preview_launch_flags(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<EffectiveLaunchSettings, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let global = state.instance_manager.load_global_settings()?;
    let (min_ram, max_ram, required_java, java_path) =
        resolve_effective_memory_and_java(&global, &inst, &state.app_data_dir);
    let natives_dir = state
        .app_data_dir
        .join("shared")
        .join("versions")
        .join(&inst.mc_version)
        .join("natives");
    let jvm_flags = build_jvm_flags(
        min_ram,
        max_ram,
        required_java,
        &natives_dir,
        &global.default_jvm_args,
        &inst.custom_jvm_args,
    );

    Ok(EffectiveLaunchSettings {
        min_ram_mb: min_ram,
        max_ram_mb: max_ram,
        required_java_major: required_java,
        resolved_java_path: java_path.unwrap_or_else(|| format!("Auto (Java {})", required_java)),
        jvm_flags,
    })
}

#[tauri::command]
fn delete_instance_resource(
    state: State<'_, AppState>,
    instance_id: String,
    category: String,
    file_name: String,
) -> Result<(), String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    delete_pack_resource(&inst.game_dir, &category, &file_name)
}

#[tauri::command]
async fn launch_instance(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    quick_play_world: Option<String>,
) -> Result<EffectiveLaunchSettings, String> {
    let mut inst = state.instance_manager.get_instance(&instance_id)?;
    inst.last_played = Some(chrono::Utc::now());
    let _ = state.instance_manager.update_instance(inst.clone());

    let global = state.instance_manager.load_global_settings()?;
    let profile = state.profile_manager.get_active_profile()?;

    // Pre-launch SkyShield security scan: block launch if critical malware/fractureiser signature is detected
    if let Ok(sec_report) = scan_instance_security(&inst.game_dir) {
        if !sec_report.passed {
            let threat_summary = sec_report
                .threats
                .iter()
                .map(|t| format!("• {}: {}", t.file_name, t.reason_tr))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "SkyShield Siber Güvenlik Koruması kritik tehdit tespit etti ve oyunu durdurdu:\n{}",
                threat_summary
            ));
        }
    }

    // Automatically heal any missing required mod dependencies before launching
    if inst.modloader != ModloaderType::Vanilla {
        let _ = heal_missing_mod_dependencies(
            &inst,
            &state.app_data_dir,
            &state.download_engine,
            &app_handle,
            global.curseforge_api_key.as_deref(),
        )
        .await;
    }

    // Feature #3: Optional automatic world backup before launch
    if global.auto_backup_worlds {
        if let Ok(worlds) = list_instance_worlds(&inst.game_dir) {
            if let Some(first_world) = worlds.first() {
                let _ = create_world_backup(&inst.game_dir, &first_world.folder_name);
            }
        }
    }

    launch_instance_process(
        app_handle,
        &state.download_engine,
        inst,
        global,
        profile,
        &state.app_data_dir,
        state.running_pids.clone(),
        quick_play_world,
    )
    .await
}

#[tauri::command]
fn record_playtime_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    added_minutes: u64,
) -> Result<InstanceConfig, String> {
    let mut inst = state.instance_manager.get_instance(&instance_id)?;
    inst.playtime_minutes = inst.playtime_minutes.saturating_add(added_minutes);
    state.instance_manager.update_instance(inst)
}

#[tauri::command]
fn cancel_launch(app_handle: AppHandle, state: State<'_, AppState>, instance_id: String) {
    state.download_engine.trigger_cancel();
    let _ = app_handle.emit("download-finished", &instance_id);
    let _ = app_handle.emit("game-exited-cleanly", &instance_id);
}

#[tauri::command]
fn stop_instance(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<bool, String> {
    state.download_engine.trigger_cancel();
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let stopped = stop_running_instance(&instance_id, &inst.game_dir, &state.running_pids);
    let _ = app_handle.emit("download-finished", &instance_id);
    let _ = app_handle.emit("game-exited-cleanly", &instance_id);
    Ok(stopped)
}

#[tauri::command]
async fn search_store(
    state: State<'_, AppState>,
    query: String,
    content_type: ContentType,
    provider: StoreProvider,
    sort_by: String,
    mc_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<StoreSearchItem>, String> {
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    search_store_content(
        state.download_engine.client(),
        &query,
        content_type,
        provider,
        &sort_by,
        mc_version.as_deref(),
        loader.as_deref(),
        category.as_deref(),
        offset.unwrap_or(0),
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn get_store_project(
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    content_type: ContentType,
    mc_version: Option<String>,
    loader: Option<String>,
) -> Result<StoreProjectDetails, String> {
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    get_store_project_details(
        state.download_engine.client(),
        &project_id,
        &provider,
        content_type,
        mc_version.as_deref(),
        loader.as_deref(),
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn install_store_item(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
    provider: String,
    content_type: ContentType,
    version_id: Option<String>,
) -> Result<Vec<String>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    install_store_content_with_dependencies(
        &inst,
        &state.app_data_dir,
        &state.download_engine,
        &app_handle,
        &project_id,
        &provider,
        content_type,
        version_id.as_deref(),
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn install_store_modpack_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    version_id: Option<String>,
) -> Result<InstanceConfig, String> {
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    install_modpack_from_store(
        &state.instance_manager,
        &state.download_engine,
        &app_handle,
        &project_id,
        &provider,
        version_id.as_deref(),
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn import_modpack_archive_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    archive_path: String,
    custom_name: Option<String>,
) -> Result<InstanceConfig, String> {
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    import_modpack_archive(
        &state.instance_manager,
        &state.download_engine,
        &app_handle,
        &PathBuf::from(archive_path),
        custom_name.as_deref(),
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
async fn check_mod_updates_cmd(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<ModUpdateInfo>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    check_instance_mod_updates(state.download_engine.client(), &inst).await
}

#[tauri::command]
async fn apply_mod_updates_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    updates: Vec<ModUpdateInfo>,
) -> Result<usize, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    apply_instance_mod_updates(&inst, &state.download_engine, &app_handle, updates).await
}

#[tauri::command]
fn export_skypack_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    target_path: String,
) -> Result<String, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    export_full_skypack(&inst, &PathBuf::from(target_path))
}

#[tauri::command]
fn import_skypack_cmd(
    state: State<'_, AppState>,
    archive_path: String,
) -> Result<InstanceConfig, String> {
    import_full_skypack(&state.instance_manager, &PathBuf::from(archive_path))
}

#[tauri::command]
async fn export_skycode_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    target_file: Option<String>,
) -> Result<SkyCodeExportResult, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let target_buf = target_file.filter(|s| !s.trim().is_empty()).map(PathBuf::from);
    export_lightweight_skycode(
        state.download_engine.client(),
        &inst,
        target_buf.as_deref(),
    )
    .await
}

#[tauri::command]
async fn import_skycode_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    code_or_path: String,
) -> Result<InstanceConfig, String> {
    import_lightweight_skycode(
        &state.instance_manager,
        &state.download_engine,
        &app_handle,
        &code_or_path,
    )
    .await
}

#[tauri::command]
async fn import_pack_from_bytes_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    file_name: String,
    bytes_base64: String,
    custom_name: Option<String>,
) -> Result<InstanceConfig, String> {
    let raw_bytes = base64_url_decode(bytes_base64.trim())
        .map_err(|e| format!("Dosya verisi çözümlenemedi: {}", e))?;

    let temp_dir = state.app_data_dir.join("cache").join("import_temp");
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let safe_name: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let temp_file = temp_dir.join(if safe_name.is_empty() {
        "imported_pack.zip".to_string()
    } else {
        safe_name.clone()
    });
    std::fs::write(&temp_file, &raw_bytes)
        .map_err(|e| format!("Geçici paket dosyası yazılamadı: {}", e))?;

    let lower = safe_name.to_lowercase();
    let res = if lower.ends_with(".skypack") {
        import_full_skypack(&state.instance_manager, &temp_file)
    } else if lower.ends_with(".skycode") {
        import_lightweight_skycode(
            &state.instance_manager,
            &state.download_engine,
            &app_handle,
            &temp_file.to_string_lossy(),
        )
        .await
    } else {
        let global = state.instance_manager.load_global_settings().unwrap_or_default();
        import_modpack_archive(
            &state.instance_manager,
            &state.download_engine,
            &app_handle,
            &temp_file,
            custom_name.as_deref(),
            global.curseforge_api_key.as_deref(),
        )
        .await
    };

    let _ = std::fs::remove_file(&temp_file);
    res
}

#[tauri::command]
fn optimize_mod_pool_cmd(
    state: State<'_, AppState>,
) -> Result<ModPoolOptimizationReport, String> {
    optimize_shared_mod_pool(&state.instance_manager)
}

#[tauri::command]
async fn install_fps_boost_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<String>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    install_fps_boost_suite(&inst, &state.app_data_dir, &state.download_engine, &app_handle).await
}

#[tauri::command]
async fn install_lan_tunnel_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<String>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    install_lan_tunnel_mod(&inst, &state.app_data_dir, &state.download_engine, &app_handle).await
}

#[tauri::command]
async fn apply_offline_skin_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    skin_path: String,
) -> Result<String, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let profile = state.profile_manager.get_active_profile()?;
    apply_offline_skin_to_instance(
        &inst,
        &state.app_data_dir,
        &state.download_engine,
        &app_handle,
        &profile.username,
        &PathBuf::from(skin_path),
    )
    .await
}

#[tauri::command]
fn clone_instance_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    new_name: Option<String>,
) -> Result<InstanceConfig, String> {
    clone_instance(&state.instance_manager, &instance_id, new_name)
}

#[tauri::command]
fn export_server_pack_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    target_zip_path: String,
) -> Result<usize, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    export_server_pack(&inst, &PathBuf::from(target_zip_path))
}

#[tauri::command]
fn sync_options_txt_cmd(
    state: State<'_, AppState>,
    source_instance_id: String,
) -> Result<usize, String> {
    sync_options_txt_across_instances(&state.instance_manager, &source_instance_id)
}

#[tauri::command]
fn list_worlds_cmd(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<WorldSaveEntry>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    list_instance_worlds(&inst.game_dir)
}

#[tauri::command]
fn list_world_backups_cmd(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<WorldBackupEntry>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    list_world_backups(&inst.game_dir)
}

#[tauri::command]
fn create_world_backup_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    folder_name: String,
) -> Result<WorldBackupEntry, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    create_world_backup(&inst.game_dir, &folder_name)
}

#[tauri::command]
fn restore_world_backup_cmd(
    state: State<'_, AppState>,
    instance_id: String,
    backup_file_name: String,
) -> Result<String, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    restore_world_backup(&inst.game_dir, &backup_file_name)
}

#[tauri::command]
fn list_screenshots_cmd(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<ScreenshotEntry>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    list_instance_screenshots(&inst.game_dir)
}

#[tauri::command]
async fn install_adoptium_runtime(
    state: State<'_, AppState>,
    java_major: u32,
) -> Result<String, String> {
    let path =
        download_adoptium_jre(&state.download_engine, java_major, &state.app_data_dir).await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn analyze_or_simulate_crash(
    state: State<'_, AppState>,
    instance_id: String,
    simulation_kind: Option<String>,
) -> Result<CrashAnalysisReport, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;

    if let Some(kind) = simulation_kind {
        let synthetic_log = match kind.as_str() {
            "missing_dep" => {
                r#"[main/ERROR] [net.minecraftforge.fml.loading.ModSorter/LOADING]: Missing or unsupported mandatory dependencies:
    Mod ID: 'geckolib', Requested by: 'alexsmobs', Expected Range: '[4.4.0,)', Actual Version: '[MISSING]'
net.minecraftforge.fml.loading.ModResolutionException: Mod Resolution Failed"#
            }
            "mixin_conflict" => {
                r#"org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered
Caused by: org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin [optifabric.mixins.json:MixinWorldRenderer] from phase [DEFAULT] in config [optifabric.mixins.json] from mod optifabric FAILED"#
            }
            "oom" => {
                "java.lang.OutOfMemoryError: Java heap space\n\tat net.minecraft.client.renderer.chunk.SectionRenderDispatcher.<init>(SectionRenderDispatcher.java:118)"
            }
            "java_mismatch" => {
                "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0"
            }
            "corrupted_config" => {
                "com.electronwill.nightconfig.core.io.ParsingException: Not enough data available at file config/create-client.toml"
            }
            _ => "",
        };

        return Ok(parse_crash_text(
            &inst.id,
            Some(&inst.game_dir),
            1,
            synthetic_log,
        ));
    }

    Ok(analyze_instance_crash(&inst.id, &inst.game_dir, 1, ""))
}

#[tauri::command]
async fn apply_crash_recovery(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    action_type: String,
    target_value: String,
) -> Result<InstanceConfig, String> {
    let mut inst = state.instance_manager.get_instance(&instance_id)?;

    match action_type.as_str() {
        "increase_ram" => {
            let add_mb: u32 = target_value.parse().unwrap_or(2048);
            inst.override_memory = true;
            inst.max_ram_mb = inst.max_ram_mb.saturating_add(add_mb);
            state.instance_manager.update_instance(inst)
        }
        "switch_java" => {
            let req_major: u32 = target_value.parse().unwrap_or(21);
            let runtimes = discover_java_runtimes(&state.app_data_dir);
            let java_bin =
                if let Some(found) = runtimes.iter().find(|r| r.major_version == req_major) {
                    found.path.clone()
                } else {
                    let downloaded = download_adoptium_jre(
                        &state.download_engine,
                        req_major,
                        &state.app_data_dir,
                    )
                    .await?;
                    downloaded.to_string_lossy().to_string()
                };
            inst.override_java = true;
            inst.java_path = Some(java_bin);
            state.instance_manager.update_instance(inst)
        }
        "reset_config" => {
            let cfg_path = inst.game_dir.join("config").join(
                target_value
                    .trim_start_matches("config/")
                    .trim_start_matches("config\\"),
            );
            if !cfg_path.exists() {
                if let Some(parent) = cfg_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&cfg_path, "# Corrupted file placeholder\n");
            }
            let _ = backup_and_reset_config_file(&inst.game_dir, &target_value)?;
            Ok(inst)
        }
        "disable_mod" => {
            let mod_path = inst.game_dir.join("mods").join(&target_value);
            if mod_path.exists() {
                let _ = toggle_mod_file(&inst.game_dir, &target_value)?;
            }
            Ok(inst)
        }
        "install_missing_mod" => {
            let global = state.instance_manager.load_global_settings().unwrap_or_default();
            let slug = resolve_mod_id_to_slug(&target_value).unwrap_or(target_value.as_str());
            let _ = install_store_content_with_dependencies(
                &inst,
                &state.app_data_dir,
                &state.download_engine,
                &app_handle,
                slug,
                "modrinth",
                ContentType::Mod,
                None,
                global.curseforge_api_key.as_deref(),
            )
            .await;
            let _ = heal_missing_mod_dependencies(
                &inst,
                &state.app_data_dir,
                &state.download_engine,
                &app_handle,
                global.curseforge_api_key.as_deref(),
            )
            .await;
            Ok(inst)
        }
        _ => Err(format!("Unknown recovery action '{}'", action_type)),
    }
}

#[tauri::command]
async fn heal_dependencies_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<String>, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    let global = state.instance_manager.load_global_settings().unwrap_or_default();
    heal_missing_mod_dependencies(
        &inst,
        &state.app_data_dir,
        &state.download_engine,
        &app_handle,
        global.curseforge_api_key.as_deref(),
    )
    .await
}

#[tauri::command]
fn scan_security_cmd(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<SecurityScanReport, String> {
    let inst = state.instance_manager.get_instance(&instance_id)?;
    scan_instance_security(&inst.game_dir)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupEnvironmentInfo {
    pub is_setup_mode: bool,
    pub default_install_dir: String,
    pub webview2_installed: bool,
    pub installed_java_majors: Vec<u32>,
    pub github_repo: String,
    pub app_version: String,
}

#[tauri::command]
fn get_setup_environment_cmd(state: State<'_, AppState>) -> SetupEnvironmentInfo {
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_lowercase()))
        .unwrap_or_default();
    let has_setup_arg = std::env::args().any(|a| a.eq_ignore_ascii_case("--setup"));
    let is_setup_mode = exe_name.contains("setup") || has_setup_arg;

    let base_local = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("C:\\SkyLauncher"));
    let default_install_dir = base_local
        .join("Programs")
        .join("SkyLauncher")
        .to_string_lossy()
        .to_string();

    let runtimes = discover_java_runtimes(&state.app_data_dir);
    let mut installed_java_majors: Vec<u32> = runtimes.into_iter().map(|r| r.major_version).collect();
    installed_java_majors.sort_unstable();
    installed_java_majors.dedup();

    SetupEnvironmentInfo {
        is_setup_mode,
        default_install_dir,
        webview2_installed: true,
        installed_java_majors,
        github_repo: OFFICIAL_GITHUB_REPO.to_string(),
        app_version: CURRENT_APP_VERSION.to_string(),
    }
}

#[tauri::command]
fn pick_folder_dialog_cmd(initial_dir: Option<String>) -> Result<Option<String>, String> {
    let init = initial_dir.unwrap_or_default().replace('\'', "''");
    let ps_script = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         Add-Type -AssemblyName System.Windows.Forms; \
         $dlg = New-Object System.Windows.Forms.FolderBrowserDialog; \
         $dlg.Description = 'SkyLauncher Kurulum Konumunu Seçin'; \
         $dlg.ShowNewFolderButton = $true; \
         if ('{}' -ne '' -and (Test-Path '{}')) {{ $dlg.SelectedPath = '{}' }}; \
         if ($dlg.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ Write-Output $dlg.SelectedPath }}",
        init, init, init
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Sta", "-Command", &ps_script])
        .output()
        .map_err(|e| format!("Klasör seçici açılamadı: {}", e))?;

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(selected))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInstallResult {
    pub installed_exe_path: String,
    pub installed_dir: String,
    pub downloaded_from_github: bool,
    pub java21_ready: bool,
    pub shortcuts_created: bool,
}

#[tauri::command]
async fn run_setup_installer_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    install_dir: String,
    _github_repo: Option<String>,
    install_java21: bool,
    create_desktop_shortcut: bool,
) -> Result<SetupInstallResult, String> {
    state.download_engine.reset_cancel();

    let target_dir = PathBuf::from(install_dir.trim());
    if target_dir.as_os_str().is_empty() {
        return Err("Lütfen geçerli bir kurulum klasörü seçin.".to_string());
    }
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Kurulum dizini oluşturulamadı ({}): {}", target_dir.display(), e))?;

    // Always lock GlobalSettings to OFFICIAL_GITHUB_REPO ("Skynzz/SkyLauncher")
    if let Ok(mut settings) = state.instance_manager.load_global_settings() {
        settings.github_update_repo = OFFICIAL_GITHUB_REPO.to_string();
        let _ = state.instance_manager.save_global_settings(&settings);
    }

    // Stage 1: Check / Install Adoptium Java 21 JRE if requested
    let mut java21_ready = discover_java_runtimes(&state.app_data_dir)
        .iter()
        .any(|r| r.major_version >= 21);

    if install_java21 && !java21_ready {
        let _ = app_handle.emit(
            "download-progress",
            DownloadProgressEvent {
                instance_id: "setup-installer".to_string(),
                completed_files: 1,
                total_files: 4,
                current_label: "Adoptium OpenJDK 21 JRE".to_string(),
                phase: "setup-java".to_string(),
                stage_text: "Aşama 1/3: Gerekli Bağımlılık (Adoptium Java 21 JRE) İndiriliyor..."
                    .to_string(),
            },
        );
        if download_adoptium_jre(&state.download_engine, 21, &state.app_data_dir)
            .await
            .is_ok()
        {
            java21_ready = true;
        }
    }

    // Stage 2: Download SkyLauncher.exe from official GitHub repository (Skynzz/SkyLauncher)
    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: "setup-installer".to_string(),
            completed_files: 2,
            total_files: 4,
            current_label: "SkyLauncher.exe".to_string(),
            phase: "setup-binary".to_string(),
            stage_text: "Aşama 2/3: SkyLauncher GitHub Deposundan (Skynzz/SkyLauncher) İndiriliyor..."
                .to_string(),
        },
    );

    let dest_exe = target_dir.join("SkyLauncher.exe");
    let staging_exe = target_dir.join("SkyLauncher.exe.download");
    let mut downloaded_from_github = false;

    let clean_repo = OFFICIAL_GITHUB_REPO;

    let candidate_url = if let Ok(check) =
        check_github_update(state.download_engine.client(), clean_repo).await
    {
        check.download_url.unwrap_or_else(|| {
            format!("https://github.com/{}/raw/main/SkyLauncher.exe", clean_repo)
        })
    } else {
        format!("https://github.com/{}/raw/main/SkyLauncher.exe", clean_repo)
    };

    if state
        .download_engine
        .download_single(&DownloadTask {
            url: candidate_url,
            destination: staging_exe.clone(),
            expected_sha1: None,
            expected_size: None,
            label: "SkyLauncher.exe (GitHub)".to_string(),
        })
        .await
        .is_ok()
    {
        if verify_pe_executable(&staging_exe).is_ok() {
            let _ = std::fs::remove_file(&dest_exe);
            if std::fs::rename(&staging_exe, &dest_exe).is_ok() {
                downloaded_from_github = true;
            }
        }
    }
    let _ = std::fs::remove_file(&staging_exe);

    // Self-contained fallback: copy the current executable engine as SkyLauncher.exe if offline
    if !downloaded_from_github {
        let current_bin = std::env::current_exe()
            .map_err(|e| format!("Kurulum motoru okunamadı: {}", e))?;
        if current_bin != dest_exe {
            std::fs::copy(&current_bin, &dest_exe)
                .map_err(|e| format!("SkyLauncher.exe hedefe kopyalanamadı: {}", e))?;
        }
    }

    // Stage 3: Create Start Menu & Desktop Shortcuts (.lnk)
    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: "setup-installer".to_string(),
            completed_files: 3,
            total_files: 4,
            current_label: "SkyLauncher.lnk".to_string(),
            phase: "setup-shortcuts".to_string(),
            stage_text: "Aşama 3/3: Masaüstü ve Başlat Menüsü Kısayolları Oluşturuluyor..."
                .to_string(),
        },
    );

    let exe_str = dest_exe.to_string_lossy().replace('\'', "''");
    let work_str = target_dir.to_string_lossy().replace('\'', "''");
    let shortcut_ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $sm = [Environment]::GetFolderPath('Programs') + '\\SkyLauncher.lnk'; \
         $s1 = $ws.CreateShortcut($sm); $s1.TargetPath = '{}'; $s1.WorkingDirectory = '{}'; $s1.Description = 'SkyLauncher Minecraft Launcher'; $s1.Save(); \
         if (${}) {{ \
           $dt = [Environment]::GetFolderPath('Desktop') + '\\SkyLauncher.lnk'; \
           $s2 = $ws.CreateShortcut($dt); $s2.TargetPath = '{}'; $s2.WorkingDirectory = '{}'; $s2.Description = 'SkyLauncher Minecraft Launcher'; $s2.Save(); \
         }}",
        exe_str,
        work_str,
        if create_desktop_shortcut { "true" } else { "false" },
        exe_str,
        work_str
    );

    let shortcuts_created = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &shortcut_ps])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let _ = app_handle.emit("download-finished", "setup-installer");

    Ok(SetupInstallResult {
        installed_exe_path: dest_exe.to_string_lossy().to_string(),
        installed_dir: target_dir.to_string_lossy().to_string(),
        downloaded_from_github,
        java21_ready,
        shortcuts_created,
    })
}

#[tauri::command]
fn launch_installed_app_cmd(exe_path: String) -> Result<(), String> {
    let p = PathBuf::from(&exe_path);
    if !p.exists() {
        return Err(format!("Uygulama bulunamadı: {}", exe_path));
    }
    if let Some(parent) = p.parent() {
        let _ = std::process::Command::new(&p).current_dir(parent).spawn();
    } else {
        let _ = std::process::Command::new(&p).spawn();
    }
    std::process::exit(0);
}

#[tauri::command]
fn get_themes(state: State<'_, AppState>) -> Result<Vec<ThemeManifest>, String> {
    state.extensibility_manager.load_themes()
}

#[tauri::command]
fn get_plugins(state: State<'_, AppState>) -> Result<Vec<PluginDescriptor>, String> {
    state.extensibility_manager.load_plugins()
}

#[tauri::command]
fn toggle_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    enabled: bool,
) -> Result<Vec<PluginDescriptor>, String> {
    state
        .extensibility_manager
        .toggle_plugin(&plugin_id, enabled)
}

#[tauri::command]
fn get_system_telemetry(state: State<'_, AppState>) -> SystemTelemetry {
    let mut sys = System::new();
    sys.refresh_memory();
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

    let launcher_ram_mb = sys
        .process(pid)
        .map(|p| (p.memory() as f64) / (1024.0 * 1024.0))
        .unwrap_or(14.2);
    let system_total_ram_mb = sys.total_memory() / (1024 * 1024);

    SystemTelemetry {
        launcher_ram_mb: (launcher_ram_mb * 10.0).round() / 10.0,
        system_total_ram_mb: system_total_ram_mb.max(8192),
        app_data_path: state.app_data_dir.to_string_lossy().to_string(),
    }
}

#[tauri::command]
async fn check_app_update_cmd(
    state: State<'_, AppState>,
    _custom_repo: Option<String>,
) -> Result<UpdateCheckResult, String> {
    check_github_update(state.download_engine.client(), OFFICIAL_GITHUB_REPO).await
}

#[tauri::command]
async fn install_app_update_cmd(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    download_url: String,
    _expected_sha256: Option<String>,
) -> Result<String, String> {
    download_and_apply_github_update(&state.download_engine, &app_handle, &download_url).await?;
    Ok("Güncelleme tamamlandı, SkyLauncher yeniden başlatılıyor...".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    cleanup_old_executable();

    let base_local = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let legacy_dir = base_local.join("AetherMCLauncher");
    let app_data_dir = base_local.join("SkyLauncher");

    if legacy_dir.exists() && !app_data_dir.exists() {
        let _ = std::fs::rename(&legacy_dir, &app_data_dir);
    }
    let _ = std::fs::create_dir_all(&app_data_dir);

    let profile_manager = ProfileManager::new(&app_data_dir);
    let instance_manager = Arc::new(InstanceManager::new(&app_data_dir));
    let download_engine = Arc::new(DownloadEngine::new());
    let extensibility_manager = ExtensibilityManager::new(&app_data_dir);

    if let Ok(existing) = instance_manager.load_instances() {
        if existing.is_empty() {
            let _ = instance_manager.create_custom_instance(
                "Vanilla 1.21.1",
                "1.21.1",
                ModloaderType::Vanilla,
                None,
                false,
                None,
                None,
            );
        }
    }

    tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            let settings = instance_manager.load_global_settings().unwrap_or_default();
            let watcher = settings
                .curseforge_instances_dir
                .as_ref()
                .map(PathBuf::from)
                .filter(|p| p.exists())
                .and_then(|p| start_curseforge_watcher(p, instance_manager.clone(), handle));

            app.manage(AppState {
                app_data_dir,
                profile_manager,
                instance_manager,
                download_engine,
                extensibility_manager,
                cf_watcher: Mutex::new(watcher),
                running_pids: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            preview_offline_uuid,
            get_auth_store,
            create_offline_profile,
            set_active_profile,
            delete_offline_profile,
            get_mojang_versions,
            get_modloader_versions,
            get_instances,
            create_instance,
            update_instance,
            delete_instance,
            get_instance_mods,
            toggle_instance_mod,
            list_instance_resources,
            delete_instance_resource,
            get_global_settings,
            save_global_settings,
            rescan_curseforge_instances,
            get_detected_java_runtimes,
            preview_launch_flags,
            launch_instance,
            record_playtime_cmd,
            cancel_launch,
            stop_instance,
            search_store,
            get_store_project,
            install_store_item,
            install_store_modpack_cmd,
            import_modpack_archive_cmd,
            check_mod_updates_cmd,
            apply_mod_updates_cmd,
            export_skypack_cmd,
            import_skypack_cmd,
            export_skycode_cmd,
            import_skycode_cmd,
            import_pack_from_bytes_cmd,
            optimize_mod_pool_cmd,
            install_fps_boost_cmd,
            install_lan_tunnel_cmd,
            apply_offline_skin_cmd,
            clone_instance_cmd,
            export_server_pack_cmd,
            sync_options_txt_cmd,
            list_worlds_cmd,
            list_world_backups_cmd,
            create_world_backup_cmd,
            restore_world_backup_cmd,
            list_screenshots_cmd,
            install_adoptium_runtime,
            analyze_or_simulate_crash,
            apply_crash_recovery,
            heal_dependencies_cmd,
            scan_security_cmd,
            get_setup_environment_cmd,
            pick_folder_dialog_cmd,
            run_setup_installer_cmd,
            launch_installed_app_cmd,
            get_themes,
            get_plugins,
            toggle_plugin,
            get_system_telemetry,
            check_app_update_cmd,
            install_app_update_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running SkyLauncher Tauri application");
}
