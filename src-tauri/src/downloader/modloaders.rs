use crate::downloader::DownloadEngine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModloaderType {
    Vanilla,
    Fabric,
    Quilt,
    NeoForge,
    Forge,
}

impl ModloaderType {
    pub fn from_str_loose(s: &str) -> Self {
        let lower = s.trim().to_lowercase();
        if lower.contains("neoforge") {
            Self::NeoForge
        } else if lower.contains("fabric") {
            Self::Fabric
        } else if lower.contains("quilt") {
            Self::Quilt
        } else if lower.contains("forge") {
            Self::Forge
        } else {
            Self::Vanilla
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::NeoForge => "neoforge",
            Self::Forge => "forge",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModloaderVersionItem {
    pub loader_type: ModloaderType,
    pub loader_version: String,
    pub mc_version: String,
    pub stable: bool,
}

pub async fn fetch_modloader_versions(
    engine: &DownloadEngine,
    loader_type: ModloaderType,
    mc_version: &str,
) -> Result<Vec<ModloaderVersionItem>, String> {
    match loader_type {
        ModloaderType::Vanilla => Ok(vec![]),
        ModloaderType::Fabric => {
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}",
                mc_version
            );
            let resp = engine
                .client()
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Fabric API hatası: {}", e))?;
            let arr: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
            let items = arr
                .into_iter()
                .filter_map(|item| {
                    let loader = item.get("loader")?;
                    let version = loader.get("version")?.as_str()?.to_string();
                    let stable = loader.get("stable").and_then(|v| v.as_bool()).unwrap_or(true);
                    Some(ModloaderVersionItem {
                        loader_type: ModloaderType::Fabric,
                        loader_version: version,
                        mc_version: mc_version.to_string(),
                        stable,
                    })
                })
                .collect();
            Ok(items)
        }
        ModloaderType::Quilt => {
            let url = format!(
                "https://meta.quiltmc.org/v3/versions/loader/{}",
                mc_version
            );
            let resp = engine
                .client()
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Quilt API hatası: {}", e))?;
            let arr: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
            let items = arr
                .into_iter()
                .filter_map(|item| {
                    let loader = item.get("loader")?;
                    let version = loader.get("version")?.as_str()?.to_string();
                    Some(ModloaderVersionItem {
                        loader_type: ModloaderType::Quilt,
                        loader_version: version,
                        mc_version: mc_version.to_string(),
                        stable: true,
                    })
                })
                .collect();
            Ok(items)
        }
        ModloaderType::NeoForge => {
            let url =
                "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
            let resp = engine
                .client()
                .get(url)
                .send()
                .await
                .map_err(|e| format!("NeoForge API hatası: {}", e))?;
            let data: Value = resp.json().await.map_err(|e| e.to_string())?;
            let prefix = mc_version.strip_prefix("1.").unwrap_or(mc_version);
            let mut items: Vec<ModloaderVersionItem> = data
                .get("versions")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter(|ver| ver.starts_with(prefix))
                        .map(|ver| ModloaderVersionItem {
                            loader_type: ModloaderType::NeoForge,
                            loader_version: ver.to_string(),
                            mc_version: mc_version.to_string(),
                            stable: !ver.contains("beta") && !ver.contains("alpha"),
                        })
                        .collect()
                })
                .unwrap_or_default();
            items.reverse();
            Ok(items)
        }
        ModloaderType::Forge => {
            let url =
                "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
            let resp = engine
                .client()
                .get(url)
                .send()
                .await
                .map_err(|e| format!("Forge API hatası: {}", e))?;
            let data: Value = resp.json().await.map_err(|e| e.to_string())?;
            let mut items = Vec::new();
            if let Some(promos) = data.get("promos").and_then(|v| v.as_object()) {
                let rec_key = format!("{}-recommended", mc_version);
                let lat_key = format!("{}-latest", mc_version);
                if let Some(rec) = promos.get(&rec_key).and_then(|v| v.as_str()) {
                    items.push(ModloaderVersionItem {
                        loader_type: ModloaderType::Forge,
                        loader_version: rec.to_string(),
                        mc_version: mc_version.to_string(),
                        stable: true,
                    });
                }
                if let Some(lat) = promos.get(&lat_key).and_then(|v| v.as_str()) {
                    if items.first().map(|i| i.loader_version.as_str()) != Some(lat) {
                        items.push(ModloaderVersionItem {
                            loader_type: ModloaderType::Forge,
                            loader_version: lat.to_string(),
                            mc_version: mc_version.to_string(),
                            stable: false,
                        });
                    }
                }
            }
            Ok(items)
        }
    }
}

pub async fn resolve_modloader_profile(
    engine: &DownloadEngine,
    loader_type: ModloaderType,
    mc_version: &str,
    loader_version: &str,
    shared_root: &Path,
    game_dir: &Path,
    java_bin: &str,
) -> Result<Option<Value>, String> {
    if loader_type == ModloaderType::Vanilla || loader_version.trim().is_empty() {
        return Ok(None);
    }

    // 1. If `game_dir/minecraftinstance.json` exists, check embedded `baseModLoader.versionJson` first!
    let mc_inst_path = game_dir.join("minecraftinstance.json");
    if mc_inst_path.exists() {
        if let Ok(raw) = tokio::fs::read_to_string(&mc_inst_path).await {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(ver_json_str) = val
                    .pointer("/baseModLoader/versionJson")
                    .and_then(|v| v.as_str())
                {
                    if let Ok(parsed_profile) = serde_json::from_str::<Value>(ver_json_str) {
                        return Ok(Some(parsed_profile));
                    }
                }
                if let Some(bml_name) = val.pointer("/baseModLoader/name").and_then(|v| v.as_str())
                {
                    let candidate = shared_root
                        .join("versions")
                        .join(bml_name)
                        .join(format!("{}.json", bml_name));
                    if candidate.exists() {
                        if let Ok(c_raw) = tokio::fs::read_to_string(&candidate).await {
                            if let Ok(c_json) = serde_json::from_str::<Value>(&c_raw) {
                                return Ok(Some(c_json));
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Check `<shared_root>/versions/` for any matching folder containing `loader_version`
    let versions_dir = shared_root.join("versions");
    if versions_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&versions_dir) {
            for entry in entries.flatten() {
                let folder_name = entry.file_name().to_string_lossy().to_string();
                if folder_name.contains(loader_version) {
                    let candidate_json = entry.path().join(format!("{}.json", folder_name));
                    if candidate_json.exists() {
                        if let Ok(raw) = tokio::fs::read_to_string(&candidate_json).await {
                            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                                return Ok(Some(json));
                            }
                        }
                    }
                }
            }
        }
    }

    let profile_id = format!("{}-{}-{}", loader_type.as_str(), mc_version, loader_version);
    let profile_path = shared_root
        .join("versions")
        .join(&profile_id)
        .join(format!("{}.json", profile_id));

    if let Some(parent) = profile_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }

    match loader_type {
        ModloaderType::Fabric => {
            let url = format!(
                "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
                mc_version, loader_version
            );
            let resp = engine
                .client()
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Fabric profili indirilemedi: {}", e))?;
            let json: Value = resp.json().await.map_err(|e| e.to_string())?;
            let _ = tokio::fs::write(
                &profile_path,
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            )
            .await;
            Ok(Some(json))
        }
        ModloaderType::Quilt => {
            let url = format!(
                "https://meta.quiltmc.org/v3/versions/loader/{}/{}/profile/json",
                mc_version, loader_version
            );
            let resp = engine
                .client()
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Quilt profili indirilemedi: {}", e))?;
            let json: Value = resp.json().await.map_err(|e| e.to_string())?;
            let _ = tokio::fs::write(
                &profile_path,
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            )
            .await;
            Ok(Some(json))
        }
        ModloaderType::NeoForge | ModloaderType::Forge => {
            let installer_url = if loader_type == ModloaderType::NeoForge {
                format!(
                    "https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
                    loader_version
                )
            } else {
                let full_ver = if loader_version.contains('-') {
                    loader_version.to_string()
                } else {
                    format!("{}-{}", mc_version, loader_version)
                };
                format!(
                    "https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar",
                    full_ver
                )
            };

            let installer_dest = shared_root
                .join("versions")
                .join(&profile_id)
                .join("installer.jar");

            let task = crate::downloader::DownloadTask {
                url: installer_url,
                destination: installer_dest.clone(),
                expected_sha1: None,
                expected_size: None,
                label: format!("{} {} kurulum paketi", loader_type.as_str(), loader_version),
            };
            engine.download_single(&task).await?;

            let profiles_stub = shared_root.join("launcher_profiles.json");
            if !profiles_stub.exists() {
                let _ = tokio::fs::write(&profiles_stub, r#"{"profiles":{}}"#).await;
            }

            let version_json =
                extract_installer_profile_and_maven(&installer_dest, &shared_root.join("libraries"))?;

            let _ = Command::new(java_bin)
                .arg("-jar")
                .arg(&installer_dest)
                .arg("--installClient")
                .arg(shared_root)
                .output()
                .await;

            let _ = tokio::fs::write(
                &profile_path,
                serde_json::to_string_pretty(&version_json).unwrap_or_default(),
            )
            .await;
            Ok(Some(version_json))
        }
        ModloaderType::Vanilla => Ok(None),
    }
}

fn extract_installer_profile_and_maven(
    installer_jar: &Path,
    libraries_dir: &Path,
) -> Result<Value, String> {
    let file = File::open(installer_jar).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        if let Ok(mut entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            if let Some(rel) = name.strip_prefix("maven/") {
                if !rel.is_empty() && !entry.is_dir() {
                    let out_path = libraries_dir.join(rel);
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let mut buf = Vec::new();
                    if entry.read_to_end(&mut buf).is_ok() {
                        let _ = std::fs::write(out_path, buf);
                    }
                }
            }
        }
    }

    let mut entry = archive
        .by_name("version.json")
        .map_err(|_| "Forge/NeoForge kurulum paketinde version.json bulunamadı.".to_string())?;

    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|e| e.to_string())?;

    serde_json::from_str(&content).map_err(|e| e.to_string())
}

pub async fn download_adoptium_jre(
    engine: &DownloadEngine,
    java_major: u32,
    app_data_dir: &Path,
) -> Result<PathBuf, String> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };

    let api_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{}/hotspot?architecture={}&image_type=jre&os={}",
        java_major, arch, os
    );

    let resp = engine
        .client()
        .get(&api_url)
        .send()
        .await
        .map_err(|e| format!("Java {} indirme sunucusuna ulaşılamadı: {}", java_major, e))?;

    let assets: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
    let first = assets
        .first()
        .ok_or_else(|| format!("Java {} paketi bulunamadı.", java_major))?;

    let pkg = first
        .pointer("/binary/package")
        .ok_or_else(|| "Adoptium paket bilgisi eksik.".to_string())?;

    let dl_url = pkg
        .get("link")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Adoptium indirme bağlantısı eksik.".to_string())?;

    let runtime_dir = app_data_dir
        .join("runtimes")
        .join(format!("java-{}", java_major));
    let archive_path = runtime_dir.join("jre.zip");

    let task = crate::downloader::DownloadTask {
        url: dl_url.to_string(),
        destination: archive_path.clone(),
        expected_sha1: None,
        expected_size: pkg.get("size").and_then(|v| v.as_u64()),
        label: format!("Java {} Çalışma Zamanı (JRE)", java_major),
    };
    engine.download_single(&task).await?;

    let file = File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|e| e.to_string())?;
    zip.extract(&runtime_dir).map_err(|e| e.to_string())?;
    let _ = tokio::fs::remove_file(&archive_path).await;

    let bin_name = if cfg!(target_os = "windows") {
        "javaw.exe"
    } else {
        "java"
    };
    if let Some(found) = find_binary_recursive(&runtime_dir, bin_name) {
        Ok(found)
    } else {
        Err(format!("Java {} indirildi ancak {} bulunamadı.", java_major, bin_name))
    }
}

fn find_binary_recursive(dir: &Path, binary_name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_binary_recursive(&path, binary_name) {
                return Some(found);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(binary_name))
            .unwrap_or(false)
        {
            return Some(path);
        }
    }
    None
}
