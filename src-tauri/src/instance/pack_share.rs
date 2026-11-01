use crate::downloader::content_store::{link_from_shared_pool_or_copy, sanitize_relative_path};
use crate::downloader::modloaders::ModloaderType;
use crate::downloader::{DownloadEngine, DownloadProgressEvent, DownloadTask};
use crate::instance::{InstanceConfig, InstanceManager};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyCodeModEntry {
    pub file_name: String,
    pub sha1: String,
    pub size_bytes: u64,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyCodeConfigEntry {
    pub relative_path: String,
    pub content_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyCodeRecipe {
    pub format_version: u32,
    pub name: String,
    pub mc_version: String,
    pub modloader: ModloaderType,
    pub modloader_version: Option<String>,
    pub recommended_ram_mb: u32,
    pub mods: Vec<SkyCodeModEntry>,
    pub configs: Vec<SkyCodeConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyCodeExportResult {
    pub share_code: String,
    pub saved_file_path: Option<String>,
    pub total_mods: usize,
    pub resolved_online_mods: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPoolOptimizationReport {
    pub deduplicated_files: usize,
    pub saved_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSaveEntry {
    pub folder_name: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackupEntry {
    pub file_name: String,
    pub world_name: String,
    pub size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotEntry {
    pub file_name: String,
    pub data_url: String,
    pub size_bytes: u64,
}

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64_url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 4) / 3 + 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        }
    }
    out
}

pub fn base64_url_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut lut = [255u8; 256];
    for (i, &c) in B64_CHARS.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    // Also accept standard '+' and '/'
    lut[b'+' as usize] = 62;
    lut[b'/' as usize] = 63;

    let clean: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'=' && !b.is_ascii_whitespace())
        .collect();

    let mut out = Vec::with_capacity((clean.len() * 3) / 4);
    for chunk in clean.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let v0 = lut[chunk[0] as usize];
        let v1 = lut[chunk[1] as usize];
        if v0 == 255 || v1 == 255 {
            return Err("Geçersiz paylaşım kodu karakteri.".to_string());
        }
        out.push((v0 << 2) | (v1 >> 4));
        if chunk.len() > 2 {
            let v2 = lut[chunk[2] as usize];
            if v2 == 255 {
                return Err("Geçersiz paylaşım kodu karakteri.".to_string());
            }
            out.push(((v1 & 0x0F) << 4) | (v2 >> 2));
            if chunk.len() > 3 {
                let v3 = lut[chunk[3] as usize];
                if v3 == 255 {
                    return Err("Geçersiz paylaşım kodu karakteri.".to_string());
                }
                out.push(((v2 & 0x03) << 6) | v3);
            }
        }
    }
    Ok(out)
}

/// Compresses a `SkyCodeRecipe` using Deflate inside an in-memory ZIP container and encodes it as `SKY1-...`.
pub fn encode_recipe_to_share_code(recipe: &SkyCodeRecipe) -> Result<String, String> {
    let json_bytes = serde_json::to_vec(recipe).map_err(|e| e.to_string())?;
    let mut zip_buf = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut zip_buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file("r.json", opts)
            .map_err(|e| e.to_string())?;
        writer.write_all(&json_bytes).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(format!("SKY1-{}", base64_url_encode(&zip_buf)))
}

/// Decodes a `SKY1-...` share code string or raw JSON back into a `SkyCodeRecipe`.
pub fn decode_recipe_from_share_code(raw: &str) -> Result<SkyCodeRecipe, String> {
    let trimmed = raw.trim();
    if let Some(b64_part) = trimmed.strip_prefix("SKY1-") {
        let zip_bytes = base64_url_decode(b64_part)?;
        let mut archive = ZipArchive::new(Cursor::new(zip_bytes))
            .map_err(|e| format!("Paylaşım kodu açılamadı: {}", e))?;
        let mut entry = archive
            .by_name("r.json")
            .map_err(|e| format!("Reçete verisi okunamadı: {}", e))?;
        let mut json_str = String::new();
        entry
            .read_to_string(&mut json_str)
            .map_err(|e| e.to_string())?;
        return serde_json::from_str(&json_str).map_err(|e| e.to_string());
    }

    serde_json::from_str(trimmed).map_err(|e| format!("Geçersiz paket kodu veya dosya: {}", e))
}

/// Mode 1 (Full Offline Bundle): Exports the entire instance (`mods/`, `config/`, `resourcepacks/`, `shaderpacks/`)
/// into a standalone `.skypack` archive that requires zero internet downloading on the recipient's machine.
pub fn export_full_skypack(instance: &InstanceConfig, target_path: &Path) -> Result<String, String> {
    if let Some(parent) = target_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let out_file =
        File::create(target_path).map_err(|e| format!("Hedef dosya oluşturulamadı: {}", e))?;
    let mut zip = ZipWriter::new(out_file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let meta = json!({
        "format": "skypack-full-v1",
        "name": instance.name,
        "mcVersion": instance.mc_version,
        "modloader": instance.modloader,
        "modloaderVersion": instance.modloader_version,
        "maxRamMb": instance.max_ram_mb,
        "exportedAt": Utc::now().to_rfc3339(),
    });

    zip.start_file("skypack.json", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(
        serde_json::to_string_pretty(&meta)
            .unwrap_or_default()
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    for folder in ["mods", "config", "resourcepacks", "shaderpacks"] {
        let dir_path = instance.game_dir.join(folder);
        if dir_path.exists() {
            add_directory_to_zip(&mut zip, &instance.game_dir, &dir_path, opts)?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(target_path.to_string_lossy().to_string())
}

fn add_directory_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    base_dir: &Path,
    current_dir: &Path,
    opts: SimpleFileOptions,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            add_directory_to_zip(zip, base_dir, &path, opts)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(base_dir) {
                let zip_name = rel.to_string_lossy().replace('\\', "/");
                zip.start_file(zip_name, opts).map_err(|e| e.to_string())?;
                if let Ok(mut f) = File::open(&path) {
                    let _ = std::io::copy(&mut f, zip);
                }
            }
        }
    }
    Ok(())
}

/// Imports a Full Offline `.skypack` archive directly into a new Custom Instance without downloading mods.
pub fn import_full_skypack(
    manager: &InstanceManager,
    archive_path: &Path,
) -> Result<InstanceConfig, String> {
    let file = File::open(archive_path).map_err(|e| format!("Dosya açılamadı: {}", e))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("Geçersiz .skypack arşivi: {}", e))?;

    let mut meta_str = String::new();
    {
        let mut meta_entry = zip
            .by_name("skypack.json")
            .map_err(|_| "Arşiv içinde skypack.json bulunamadı.")?;
        meta_entry
            .read_to_string(&mut meta_str)
            .map_err(|e| e.to_string())?;
    }

    let meta: Value = serde_json::from_str(&meta_str).map_err(|e| e.to_string())?;
    let name = meta
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("İçe Aktarılan Paket");
    let mc_version = meta
        .get("mcVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("1.20.1");
    let modloader = meta
        .get("modloader")
        .and_then(|v| v.as_str())
        .map(ModloaderType::from_str_loose)
        .unwrap_or(ModloaderType::Fabric);
    let modloader_version = meta
        .get("modloaderVersion")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let max_ram = meta
        .get("maxRamMb")
        .and_then(|v| v.as_u64())
        .map(|m| m as u32)
        .unwrap_or(4096);

    let instance = manager.create_custom_instance(
        name,
        mc_version,
        modloader,
        modloader_version,
        true,
        Some(1024),
        Some(max_ram),
    )?;

    for i in 0..zip.len() {
        let mut entry = match zip.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let entry_name = entry.name().replace('\\', "/");
        if entry_name == "skypack.json" || entry_name.is_empty() {
            continue;
        }
        if let Some(dest) = sanitize_relative_path(&instance.game_dir, &entry_name) {
            if entry.is_dir() || entry_name.ends_with('/') {
                let _ = fs::create_dir_all(&dest);
            } else {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(mut out) = File::create(&dest) {
                    let _ = std::io::copy(&mut entry, &mut out);
                }
            }
        }
    }

    Ok(instance)
}

/// Mode 2 (Lightweight Recipe / Share Code): Generates a compact `.skycode` file and a single-line `SKY1-...` code.
/// Resolves official Modrinth CDN URLs via SHA-1 batch lookup so even manually dropped `.jar` files can be downloaded by friends.
pub async fn export_lightweight_skycode(
    client: &Client,
    instance: &InstanceConfig,
    target_file: Option<&Path>,
) -> Result<SkyCodeExportResult, String> {
    let mods_dir = instance.game_dir.join("mods");
    let mut mod_entries: Vec<SkyCodeModEntry> = Vec::new();
    let mut hashes: Vec<String> = Vec::new();

    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let fname = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) if n.ends_with(".jar") => n.to_string(),
                    _ => continue,
                };
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if let Ok(mut f) = File::open(&path) {
                    let mut hasher = Sha1::new();
                    if std::io::copy(&mut f, &mut hasher).is_ok() {
                        let sha = hex::encode(hasher.finalize());
                        hashes.push(sha.clone());
                        mod_entries.push(SkyCodeModEntry {
                            file_name: fname,
                            sha1: sha,
                            size_bytes,
                            download_url: None,
                        });
                    }
                }
            }
        }
    }

    // Batch resolve download URLs from Modrinth by SHA-1 (`POST /v2/version_files`)
    if !hashes.is_empty() {
        if let Ok(resp) = client
            .post(format!("{}/version_files", MODRINTH_API_BASE))
            .json(&json!({
                "hashes": hashes,
                "algorithm": "sha1"
            }))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(map) = resp.json::<HashMap<String, Value>>().await {
                    for m in &mut mod_entries {
                        if let Some(ver_obj) = map.get(&m.sha1) {
                            if let Some(files) = ver_obj.get("files").and_then(|f| f.as_array()) {
                                let primary = files
                                    .iter()
                                    .find(|f| {
                                        f.pointer("/hashes/sha1")
                                            .and_then(|h| h.as_str())
                                            .map(|h| h.eq_ignore_ascii_case(&m.sha1))
                                            .unwrap_or(false)
                                    })
                                    .or_else(|| files.first());
                                if let Some(url) =
                                    primary.and_then(|p| p.get("url")).and_then(|u| u.as_str())
                                {
                                    m.download_url = Some(url.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect lightweight config files (< 32 KB) so custom pack tweaks travel with the recipe
    let mut configs = Vec::new();
    let cfg_dir = instance.game_dir.join("config");
    if cfg_dir.exists() {
        if let Ok(entries) = fs::read_dir(&cfg_dir) {
            for entry in entries.flatten().take(25) {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = entry.metadata() {
                        if meta.len() > 0 && meta.len() <= 32_768 {
                            if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                                if let Ok(bytes) = fs::read(&path) {
                                    configs.push(SkyCodeConfigEntry {
                                        relative_path: format!("config/{}", fname),
                                        content_b64: base64_url_encode(&bytes),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let resolved_online_mods = mod_entries
        .iter()
        .filter(|m| m.download_url.is_some())
        .count();
    let total_mods = mod_entries.len();

    let recipe = SkyCodeRecipe {
        format_version: 1,
        name: instance.name.clone(),
        mc_version: instance.mc_version.clone(),
        modloader: instance.modloader,
        modloader_version: instance.modloader_version.clone(),
        recommended_ram_mb: instance.max_ram_mb,
        mods: mod_entries,
        configs,
    };

    let share_code = encode_recipe_to_share_code(&recipe)?;

    let mut saved_file_path = None;
    if let Some(target) = target_file {
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let pretty = serde_json::to_string_pretty(&recipe).map_err(|e| e.to_string())?;
        fs::write(target, pretty).map_err(|e| format!("Dosya yazılamadı: {}", e))?;
        saved_file_path = Some(target.to_string_lossy().to_string());
    }

    Ok(SkyCodeExportResult {
        share_code,
        saved_file_path,
        total_mods,
        resolved_online_mods,
    })
}

/// Imports a custom modpack from either a `SKY1-...` share code string OR a `.skycode` file path,
/// automatically downloading all mods in parallel.
pub async fn import_lightweight_skycode(
    manager: &InstanceManager,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    code_or_path: &str,
) -> Result<InstanceConfig, String> {
    downloader.reset_cancel();
    let trimmed = code_or_path.trim();
    let raw_content = if !trimmed.starts_with("SKY1-") && Path::new(trimmed).exists() {
        fs::read_to_string(trimmed).map_err(|e| format!("Dosya okunamadı: {}", e))?
    } else {
        trimmed.to_string()
    };

    let recipe = decode_recipe_from_share_code(&raw_content)?;
    let instance = manager.create_custom_instance(
        &recipe.name,
        &recipe.mc_version,
        recipe.modloader,
        recipe.modloader_version.clone(),
        true,
        Some(1024),
        Some(recipe.recommended_ram_mb.max(2048)),
    )?;

    // Restore lightweight config files
    for cfg in &recipe.configs {
        if let Some(dest) = sanitize_relative_path(&instance.game_dir, &cfg.relative_path) {
            if let Some(parent) = dest.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(bytes) = base64_url_decode(&cfg.content_b64) {
                let _ = fs::write(&dest, bytes);
            }
        }
    }

    let pool_dir = manager.app_data_dir().join("shared").join("mod_pool");
    let _ = fs::create_dir_all(&pool_dir);
    let mods_dir = instance.game_dir.join("mods");
    let _ = fs::create_dir_all(&mods_dir);

    let mut tasks = Vec::new();
    let mut links: Vec<(PathBuf, PathBuf)> = Vec::new();

    for m in &recipe.mods {
        if let Some(ref url) = m.download_url {
            let short_sha = &m.sha1[0..8.min(m.sha1.len())];
            let pool_path = pool_dir.join(format!("{}_{}", short_sha, m.file_name));
            let final_dest = mods_dir.join(&m.file_name);
            tasks.push(DownloadTask {
                url: url.clone(),
                destination: pool_path.clone(),
                expected_sha1: Some(m.sha1.clone()),
                expected_size: if m.size_bytes > 0 {
                    Some(m.size_bytes)
                } else {
                    None
                },
                label: m.file_name.clone(),
            });
            links.push((pool_path, final_dest));
        }
    }

    if !tasks.is_empty() {
        let inst_id = instance.id.clone();
        let app_clone = app_handle.clone();
        downloader
            .verify_and_heal_batch(tasks, move |done, total, label| {
                let _ = app_clone.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        instance_id: inst_id.clone(),
                        completed_files: done,
                        total_files: total,
                        current_label: label,
                        phase: "skycode-import".to_string(),
                        stage_text: "Paylaşılan paketin modları indiriliyor...".to_string(),
                    },
                );
            })
            .await?;

        for (src, dst) in links {
            let _ = link_from_shared_pool_or_copy(&src, &dst);
        }
        let _ = app_handle.emit("download-finished", &instance.id);
    }

    Ok(instance)
}

/// Feature #1: Scans all instances' `mods/` folders, moves unique `.jar` files into the Shared Mod Pool,
/// and replaces duplicate copies across instances with OS Hardlinks to save 60-75% disk space.
pub fn optimize_shared_mod_pool(manager: &InstanceManager) -> Result<ModPoolOptimizationReport, String> {
    let pool_dir = manager.app_data_dir().join("shared").join("mod_pool");
    let _ = fs::create_dir_all(&pool_dir);

    let instances = manager.load_instances()?;
    let mut deduplicated_files = 0usize;
    let mut saved_bytes = 0u64;
    let mut seen_hashes: HashMap<String, PathBuf> = HashMap::new();

    for inst in &instances {
        let mods_dir = inst.game_dir.join("mods");
        if !mods_dir.exists() {
            continue;
        }
        let entries = match fs::read_dir(&mods_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let fname = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) if n.ends_with(".jar") => n.to_string(),
                _ => continue,
            };
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if len == 0 {
                continue;
            }

            if let Ok(mut file) = File::open(&path) {
                let mut hasher = Sha1::new();
                if std::io::copy(&mut file, &mut hasher).is_ok() {
                    drop(file);
                    let sha = hex::encode(hasher.finalize());
                    let pool_file = pool_dir.join(format!("{}_{}", &sha[0..8], fname));
                    let already_in_pool = pool_file.exists() || seen_hashes.contains_key(&sha);

                    if !pool_file.exists() {
                        let _ = fs::copy(&path, &pool_file);
                    }
                    seen_hashes.insert(sha, pool_file.clone());

                    if already_in_pool {
                        let temp_bak = path.with_extension("jar.hl_tmp");
                        if fs::rename(&path, &temp_bak).is_ok() {
                            if fs::hard_link(&pool_file, &path).is_ok() {
                                let _ = fs::remove_file(&temp_bak);
                                deduplicated_files += 1;
                                saved_bytes += len;
                            } else {
                                let _ = fs::rename(&temp_bak, &path);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(ModPoolOptimizationReport {
        deduplicated_files,
        saved_bytes,
    })
}

/// Feature #7: Clones an existing instance into a new isolated Custom Instance using hardlinks for `mods/`.
pub fn clone_instance(manager: &InstanceManager, instance_id: &str, new_name: Option<String>) -> Result<InstanceConfig, String> {
    let source = manager.get_instance(instance_id)?;
    let target_name = new_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{} (Kopya)", source.name));

    let cloned = manager.create_custom_instance(
        &target_name,
        &source.mc_version,
        source.modloader,
        source.modloader_version.clone(),
        source.override_memory,
        Some(source.min_ram_mb),
        Some(source.max_ram_mb),
    )?;

    for sub in ["mods", "config", "resourcepacks", "shaderpacks"] {
        let src_dir = source.game_dir.join(sub);
        let dst_dir = cloned.game_dir.join(sub);
        if src_dir.exists() {
            copy_or_hardlink_dir_recursive(&src_dir, &dst_dir, sub == "mods")?;
        }
    }

    let src_opts = source.game_dir.join("options.txt");
    if src_opts.exists() {
        let _ = fs::copy(&src_opts, cloned.game_dir.join("options.txt"));
    }

    Ok(cloned)
}

fn copy_or_hardlink_dir_recursive(src: &Path, dst: &Path, use_hardlink: bool) -> Result<(), String> {
    let _ = fs::create_dir_all(dst);
    let entries = fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_or_hardlink_dir_recursive(&path, &target, use_hardlink)?;
        } else if path.is_file() {
            if use_hardlink {
                let _ = link_from_shared_pool_or_copy(&path, &target);
            } else {
                let _ = fs::copy(&path, &target);
            }
        }
    }
    Ok(())
}

/// Feature #8: Exports a ready-to-run Dedicated Server Pack (`.zip`), automatically filtering out client-only mods.
pub fn export_server_pack(instance: &InstanceConfig, target_zip_path: &Path) -> Result<usize, String> {
    if let Some(parent) = target_zip_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let out_file = File::create(target_zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(out_file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let client_only_keywords = [
        "sodium",
        "embeddium",
        "iris",
        "oculus",
        "entityculling",
        "immediatelyfast",
        "modmenu",
        "mousetweaks",
        "controlling",
        "customskinloader",
        "zoomify",
        "reeses-sodium",
        "sodium-extra",
        "sound-physics",
        "presence-footsteps",
        "lambdynamiclights",
        "citresewn",
        "continuity",
        "indium",
        "e4mc",
    ];

    let mut exported_mods = 0usize;
    let mods_dir = instance.game_dir.join("mods");
    if mods_dir.exists() {
        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let fname = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) if n.ends_with(".jar") => n.to_string(),
                    _ => continue,
                };
                let lower = fname.to_lowercase();
                if client_only_keywords.iter().any(|kw| lower.contains(kw)) {
                    continue;
                }
                zip.start_file(format!("mods/{}", fname), opts)
                    .map_err(|e| e.to_string())?;
                if let Ok(mut f) = File::open(&path) {
                    let _ = std::io::copy(&mut f, &mut zip);
                    exported_mods += 1;
                }
            }
        }
    }

    let cfg_dir = instance.game_dir.join("config");
    if cfg_dir.exists() {
        add_directory_to_zip(&mut zip, &instance.game_dir, &cfg_dir, opts)?;
    }

    zip.start_file("eula.txt", opts).map_err(|e| e.to_string())?;
    zip.write_all(b"# Generated by SkyLauncher Server Pack Builder\neula=true\n")
        .map_err(|e| e.to_string())?;

    let start_bat = format!(
        "@echo off\r\ntitle {} Server ({} - {})\r\necho Starting SkyLauncher Dedicated Server...\r\njava -Xms2048M -Xmx{}M -XX:+UseG1GC -jar server.jar nogui\r\npause\r\n",
        instance.name,
        instance.mc_version,
        instance.modloader.as_str(),
        instance.max_ram_mb
    );
    zip.start_file("start_server.bat", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(start_bat.as_bytes())
        .map_err(|e| e.to_string())?;

    zip.finish().map_err(|e| e.to_string())?;
    Ok(exported_mods)
}

/// Feature #9: Synchronizes `options.txt` (keybinds, FOV, audio, language) from a source instance to all other instances.
pub fn sync_options_txt_across_instances(
    manager: &InstanceManager,
    source_instance_id: &str,
) -> Result<usize, String> {
    let source = manager.get_instance(source_instance_id)?;
    let src_options = source.game_dir.join("options.txt");
    if !src_options.exists() {
        return Err(
            "Bu pakette henüz options.txt oluşturulmamış (paketi en az bir kez açmış olmanız gerekir)."
                .to_string(),
        );
    }

    let instances = manager.load_instances()?;
    let mut synced = 0usize;
    for inst in instances {
        if inst.id == source.id {
            continue;
        }
        let _ = fs::create_dir_all(&inst.game_dir);
        let dst = inst.game_dir.join("options.txt");
        if fs::copy(&src_options, &dst).is_ok() {
            synced += 1;
        }
    }
    Ok(synced)
}

/// Feature #3 & #6: Lists singleplayer worlds (`saves/`) and world snapshots (`backups/`).
pub fn list_instance_worlds(game_dir: &Path) -> Result<Vec<WorldSaveEntry>, String> {
    let saves_dir = game_dir.join("saves");
    if !saves_dir.exists() {
        let _ = fs::create_dir_all(&saves_dir);
        return Ok(Vec::new());
    }

    let mut worlds = Vec::new();
    for entry in fs::read_dir(&saves_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let folder_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let size_bytes = compute_dir_size(&path);
        worlds.push(WorldSaveEntry {
            display_name: folder_name.clone(),
            folder_name,
            size_bytes,
            last_modified: Utc::now().format("%Y-%m-%d %H:%M").to_string(),
        });
    }
    Ok(worlds)
}

pub fn list_world_backups(game_dir: &Path) -> Result<Vec<WorldBackupEntry>, String> {
    let backups_dir = game_dir.join("backups");
    if !backups_dir.exists() {
        let _ = fs::create_dir_all(&backups_dir);
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&backups_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let fname = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.ends_with(".zip") => n.to_string(),
            _ => continue,
        };
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let world_name = fname.split("__").next().unwrap_or(&fname).to_string();
        items.push(WorldBackupEntry {
            file_name: fname.clone(),
            world_name,
            size_bytes,
            created_at: fname,
        });
    }
    items.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(items)
}

pub fn create_world_backup(game_dir: &Path, folder_name: &str) -> Result<WorldBackupEntry, String> {
    if folder_name.contains("..") || folder_name.contains('/') || folder_name.contains('\\') {
        return Err("Geçersiz dünya klasörü adı.".to_string());
    }
    let world_dir = game_dir.join("saves").join(folder_name);
    if !world_dir.exists() {
        return Err(format!("Dünya bulunamadı: {}", folder_name));
    }

    let backups_dir = game_dir.join("backups");
    let _ = fs::create_dir_all(&backups_dir);

    let stamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let zip_name = format!("{}__{}.zip", folder_name, stamp);
    let zip_path = backups_dir.join(&zip_name);

    let file = File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    add_directory_to_zip(&mut zip, &world_dir, &world_dir, opts)?;
    zip.finish().map_err(|e| e.to_string())?;

    let size_bytes = fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    Ok(WorldBackupEntry {
        file_name: zip_name,
        world_name: folder_name.to_string(),
        size_bytes,
        created_at: stamp,
    })
}

pub fn restore_world_backup(game_dir: &Path, backup_file_name: &str) -> Result<String, String> {
    if backup_file_name.contains("..") {
        return Err("Geçersiz yedek dosya adı.".to_string());
    }
    let zip_path = game_dir.join("backups").join(backup_file_name);
    if !zip_path.exists() {
        return Err("Yedek dosyası bulunamadı.".to_string());
    }

    let world_folder = backup_file_name
        .split("__")
        .next()
        .unwrap_or("RestoredWorld")
        .trim_end_matches(".zip");
    let target_world_dir = game_dir.join("saves").join(world_folder);
    if target_world_dir.exists() {
        let _ = fs::remove_dir_all(&target_world_dir);
    }
    let _ = fs::create_dir_all(&target_world_dir);

    let file = File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..zip.len() {
        let mut entry = match zip.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.name().replace('\\', "/");
        if let Some(dest) = sanitize_relative_path(&target_world_dir, &name) {
            if entry.is_dir() || name.ends_with('/') {
                let _ = fs::create_dir_all(&dest);
            } else {
                if let Some(p) = dest.parent() {
                    let _ = fs::create_dir_all(p);
                }
                if let Ok(mut out) = File::create(&dest) {
                    let _ = std::io::copy(&mut entry, &mut out);
                }
            }
        }
    }

    Ok(world_folder.to_string())
}

/// Feature #6: Lists up to 12 screenshots in `<game_dir>/screenshots/` as inline base64 data URLs.
pub fn list_instance_screenshots(game_dir: &Path) -> Result<Vec<ScreenshotEntry>, String> {
    let dir = game_dir.join("screenshots");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten().take(12) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let fname = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.to_lowercase().ends_with(".png") => n.to_string(),
            _ => continue,
        };
        if let Ok(bytes) = fs::read(&path) {
            let b64 = standard_base64_encode(&bytes);
            out.push(ScreenshotEntry {
                file_name: fname,
                data_url: format!("data:image/png;base64,{}", b64),
                size_bytes: bytes.len() as u64,
            });
        }
    }
    Ok(out)
}

fn standard_base64_encode(data: &[u8]) -> String {
    const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() * 4) / 3 + 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(STD[((triple >> 18) & 0x3F) as usize] as char);
        out.push(STD[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(STD[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(STD[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += compute_dir_size(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skycode_share_code_roundtrip() {
        let recipe = SkyCodeRecipe {
            format_version: 1,
            name: "Sky Optimizer Pack".to_string(),
            mc_version: "1.20.1".to_string(),
            modloader: ModloaderType::Fabric,
            modloader_version: Some("0.16.9".to_string()),
            recommended_ram_mb: 4096,
            mods: vec![SkyCodeModEntry {
                file_name: "sodium-fabric-0.5.8.jar".to_string(),
                sha1: "aabbccddeeff00112233445566778899aabbccdd".to_string(),
                size_bytes: 950_000,
                download_url: Some("https://cdn.modrinth.com/data/AANobbMI/versions/sodium.jar".to_string()),
            }],
            configs: vec![SkyCodeConfigEntry {
                relative_path: "config/sodium-options.json".to_string(),
                content_b64: base64_url_encode(b"{\"quality\":\"high\"}"),
            }],
        };

        let code = encode_recipe_to_share_code(&recipe).expect("encode share code");
        assert!(code.starts_with("SKY1-"));

        let decoded = decode_recipe_from_share_code(&code).expect("decode share code");
        assert_eq!(decoded.name, "Sky Optimizer Pack");
        assert_eq!(decoded.mc_version, "1.20.1");
        assert_eq!(decoded.mods.len(), 1);
        assert_eq!(decoded.mods[0].file_name, "sodium-fabric-0.5.8.jar");
    }
}
