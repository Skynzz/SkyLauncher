use crate::downloader::{DownloadEngine, DownloadTask};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub const MOJANG_MANIFEST_V2_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const MINECRAFT_RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MojangManifest {
    pub latest: LatestVersions,
    pub versions: Vec<MojangVersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MojangVersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: Option<String>,
}

/// Fetches Mojang's official `version_manifest_v2.json` and optionally filters by type.
pub async fn fetch_version_manifest(
    engine: &DownloadEngine,
    filter_type: Option<&str>,
) -> Result<MojangManifest, String> {
    let resp = engine
        .client()
        .get(MOJANG_MANIFEST_V2_URL)
        .send()
        .await
        .map_err(|e| format!("Mojang sunucusuna bağlanılamadı: {}", e))?;

    let mut manifest: MojangManifest = resp
        .json()
        .await
        .map_err(|e| format!("Mojang sürüm listesi okunamadı: {}", e))?;

    if let Some(ft) = filter_type {
        if ft != "all" && !ft.is_empty() {
            manifest.versions.retain(|v| {
                if ft == "historical" {
                    v.version_type == "old_beta" || v.version_type == "old_alpha"
                } else {
                    v.version_type.eq_ignore_ascii_case(ft)
                }
            });
        }
    }

    Ok(manifest)
}

/// Evaluates Mojang `rules` array for the current OS and CPU architecture.
pub fn evaluate_rules(rules: Option<&Vec<Value>>) -> bool {
    let rules = match rules {
        Some(r) if !r.is_empty() => r,
        _ => return true,
    };

    let current_os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };

    let current_arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "arm64"
    };

    let mut allowed = false;
    for rule in rules {
        let action = rule
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("allow");

        if rule.get("features").is_some() {
            continue;
        }

        let mut matches = true;
        if let Some(os_obj) = rule.get("os") {
            if let Some(os_name) = os_obj.get("name").and_then(|v| v.as_str()) {
                if os_name != current_os {
                    matches = false;
                }
            }
            if let Some(arch_name) = os_obj.get("arch").and_then(|v| v.as_str()) {
                let norm_arch = if arch_name == "amd64" || arch_name == "x64" {
                    "x86_64"
                } else {
                    arch_name
                };
                if norm_arch != current_arch {
                    matches = false;
                }
            }
        }

        if matches {
            allowed = action == "allow";
        }
    }
    allowed
}

/// Checks if a library coordinate contains a foreign OS or CPU architecture native classifier
/// (e.g. rejects `natives-windows-x86` and `natives-windows-arm64` on 64-bit Windows).
pub fn is_compatible_native_coord(coord: &str) -> bool {
    let lower = coord.to_lowercase();
    if !lower.contains("natives-") {
        return true;
    }

    if cfg!(target_os = "windows") {
        if !lower.contains("natives-windows") {
            return false;
        }
        if cfg!(target_arch = "x86_64") {
            return !lower.contains("natives-windows-x86")
                && !lower.contains("natives-windows-arm64");
        }
    } else if cfg!(target_os = "macos") {
        if !lower.contains("natives-macos") && !lower.contains("natives-osx") {
            return false;
        }
    } else if !lower.contains("natives-linux") {
        return false;
    }

    true
}

/// Converts a Maven coordinate (`group:artifact:version[:classifier]`) into a key and relative path.
pub fn maven_coord_to_path(coord: &str) -> Option<(String, String)> {
    let clean = coord.split('@').next().unwrap_or(coord);
    let parts: Vec<&str> = clean.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();

    let key = match classifier {
        Some(c) => format!("{}:{}:{}", parts[0], artifact, c),
        None => format!("{}:{}", parts[0], artifact),
    };

    let filename = match classifier {
        Some(c) => format!("{}-{}-{}.jar", artifact, version, c),
        None => format!("{}-{}.jar", artifact, version),
    };

    let rel_path = format!("{}/{}/{}/{}", group, artifact, version, filename);
    Some((key, rel_path))
}

/// Resolves the Vanilla version JSON, asset index, asset objects, libraries, and natives.
pub async fn resolve_vanilla_version(
    engine: &DownloadEngine,
    mc_version: &str,
    shared_root: &Path,
) -> Result<(Value, Vec<DownloadTask>), String> {
    let version_dir = shared_root.join("versions").join(mc_version);
    let version_json_path = version_dir.join(format!("{}.json", mc_version));

    if !version_json_path.exists() {
        let manifest = fetch_version_manifest(engine, None).await?;
        let entry = manifest
            .versions
            .iter()
            .find(|v| v.id == mc_version)
            .ok_or_else(|| format!("Minecraft '{}' sürümü Mojang listesinde bulunamadı.", mc_version))?;

        let task = DownloadTask {
            url: entry.url.clone(),
            destination: version_json_path.clone(),
            expected_sha1: entry.sha1.clone(),
            expected_size: None,
            label: format!("{}.json", mc_version),
        };
        engine.download_single(&task).await?;
    }

    let raw_json = tokio::fs::read_to_string(&version_json_path)
        .await
        .map_err(|e| format!("{:?} okunamadı: {}", version_json_path, e))?;
    let version_json: Value =
        serde_json::from_str(&raw_json).map_err(|e| format!("Geçersiz sürüm JSON: {}", e))?;

    let mut tasks = Vec::new();

    // 1. Client JAR
    if let Some(client_dl) = version_json.pointer("/downloads/client") {
        if let Some(url) = client_dl.get("url").and_then(|v| v.as_str()) {
            let sha1 = client_dl
                .get("sha1")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let size = client_dl.get("size").and_then(|v| v.as_u64());
            let client_jar_path = version_dir.join(format!("{}.jar", mc_version));
            tasks.push(DownloadTask {
                url: url.to_string(),
                destination: client_jar_path,
                expected_sha1: sha1,
                expected_size: size,
                label: format!("Minecraft {}", mc_version),
            });
        }
    }

    // 2. Asset Index & Asset Objects Pipeline
    if let Some(asset_index) = version_json.get("assetIndex") {
        if let (Some(id), Some(url)) = (
            asset_index.get("id").and_then(|v| v.as_str()),
            asset_index.get("url").and_then(|v| v.as_str()),
        ) {
            let sha1 = asset_index
                .get("sha1")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let index_path = shared_root
                .join("assets")
                .join("indexes")
                .join(format!("{}.json", id));

            let index_task = DownloadTask {
                url: url.to_string(),
                destination: index_path.clone(),
                expected_sha1: sha1,
                expected_size: None,
                label: format!("Varlık İndeksi ({})", id),
            };
            engine.download_single(&index_task).await?;

            if let Ok(index_raw) = tokio::fs::read_to_string(&index_path).await {
                if let Ok(index_val) = serde_json::from_str::<Value>(&index_raw) {
                    if let Some(objects) = index_val.get("objects").and_then(|v| v.as_object()) {
                        for (asset_name, obj) in objects {
                            if let Some(hash) = obj.get("hash").and_then(|v| v.as_str()) {
                                if hash.len() >= 2 {
                                    let prefix = &hash[0..2];
                                    let obj_url =
                                        format!("{}/{}/{}", MINECRAFT_RESOURCES_URL, prefix, hash);
                                    let obj_dest = shared_root
                                        .join("assets")
                                        .join("objects")
                                        .join(prefix)
                                        .join(hash);
                                    tasks.push(DownloadTask {
                                        url: obj_url,
                                        destination: obj_dest,
                                        expected_sha1: Some(hash.to_string()),
                                        expected_size: obj.get("size").and_then(|v| v.as_u64()),
                                        label: asset_name.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((version_json, tasks))
}

/// Combines Vanilla and optional Modloader libraries, deduplicating by Maven key
/// (Modloader overrides Vanilla when versions conflict), and collects download tasks + classpath.
pub fn collect_libraries_and_classpath(
    vanilla_json: &Value,
    modloader_json: Option<&Value>,
    mc_version: &str,
    shared_root: &Path,
) -> (Vec<DownloadTask>, Vec<PathBuf>, Vec<PathBuf>) {
    let libraries_dir = shared_root.join("libraries");
    let mut ordered_keys: Vec<String> = Vec::new();
    let mut maven_map: HashMap<String, (PathBuf, Option<DownloadTask>)> = HashMap::new();
    let mut native_jars_to_extract = Vec::new();

    let os_classifier_key = if cfg!(target_os = "windows") {
        "natives-windows"
    } else if cfg!(target_os = "macos") {
        "natives-osx"
    } else {
        "natives-linux"
    };

    let mut process_lib_array = |libs: &Vec<Value>, default_base_url: &str| {
        for lib in libs {
            let rules = lib.get("rules").and_then(|v| v.as_array());
            if !evaluate_rules(rules) {
                continue;
            }

            let name = match lib.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => continue,
            };

            if !is_compatible_native_coord(name) {
                continue;
            }

            if let Some((maven_key, rel_path)) = maven_coord_to_path(name) {
                let dest = libraries_dir.join(&rel_path);
                let mut dl_sha1: Option<String> = None;
                let mut dl_size: Option<u64> = None;

                let dl_url: Option<String> =
                    if let Some(artifact) = lib.pointer("/downloads/artifact") {
                        dl_sha1 = artifact
                            .get("sha1")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        dl_size = artifact.get("size").and_then(|v| v.as_u64());
                        artifact
                            .get("url")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .or_else(|| {
                                // Fallback for Forge/NeoForge libraries where url is empty but exists on Maven
                                if dest.exists() {
                                    None
                                } else {
                                    let trimmed = default_base_url.trim_end_matches('/');
                                    Some(format!("{}/{}", trimmed, rel_path))
                                }
                            })
                    } else if dest.exists() {
                        None
                    } else {
                        let base = lib
                            .get("url")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .unwrap_or(default_base_url);
                        let trimmed = base.trim_end_matches('/');
                        Some(format!("{}/{}", trimmed, rel_path))
                    };

                if name.to_lowercase().contains("natives-") {
                    native_jars_to_extract.push(dest.clone());
                }

                let task = dl_url.map(|url| DownloadTask {
                    url,
                    destination: dest.clone(),
                    expected_sha1: dl_sha1,
                    expected_size: dl_size,
                    label: name.to_string(),
                });

                if !ordered_keys.contains(&maven_key) {
                    ordered_keys.push(maven_key.clone());
                }
                maven_map.insert(maven_key, (dest, task));
            }

            // Legacy classifier natives (MC <= 1.18.2)
            if let Some(classifiers) = lib
                .pointer("/downloads/classifiers")
                .and_then(|v| v.as_object())
            {
                let native_entry = classifiers
                    .get(os_classifier_key)
                    .or_else(|| classifiers.get("natives-windows-64"));

                if let Some(nat) = native_entry {
                    if let Some(url) = nat.get("url").and_then(|v| v.as_str()) {
                        let path_str = nat
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("natives.jar");
                        let dest = libraries_dir.join(path_str);
                        let sha1 = nat
                            .get("sha1")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let task = DownloadTask {
                            url: url.to_string(),
                            destination: dest.clone(),
                            expected_sha1: sha1,
                            expected_size: nat.get("size").and_then(|v| v.as_u64()),
                            label: format!("Native {}", name),
                        };
                        let nkey = format!("{}:native", name);
                        if !ordered_keys.contains(&nkey) {
                            ordered_keys.push(nkey.clone());
                        }
                        maven_map.insert(nkey, (dest.clone(), Some(task)));
                        native_jars_to_extract.push(dest);
                    }
                }
            }
        }
    };

    if let Some(vanilla_libs) = vanilla_json.get("libraries").and_then(|v| v.as_array()) {
        process_lib_array(vanilla_libs, "https://libraries.minecraft.net");
    }

    if let Some(ml_json) = modloader_json {
        if let Some(ml_libs) = ml_json.get("libraries").and_then(|v| v.as_array()) {
            let default_maven = if ml_json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("neoforge")
            {
                "https://maven.neoforged.net/releases"
            } else if ml_json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("forge")
            {
                "https://maven.minecraftforge.net"
            } else if ml_json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("quilt")
            {
                "https://maven.quiltmc.org/repository/release"
            } else {
                "https://maven.fabricmc.net"
            };
            process_lib_array(ml_libs, default_maven);
        }
    }

    let mut tasks = Vec::new();
    let mut classpath = Vec::new();

    for key in ordered_keys {
        if let Some((path, task_opt)) = maven_map.remove(&key) {
            classpath.push(path);
            if let Some(t) = task_opt {
                tasks.push(t);
            }
        }
    }

    // Append main Minecraft client.jar at the end of classpath
    let client_jar = shared_root
        .join("versions")
        .join(mc_version)
        .join(format!("{}.jar", mc_version));
    classpath.push(client_jar);

    (tasks, classpath, native_jars_to_extract)
}

/// Extracts `.dll` / `.so` / `.dylib` files from native JARs into `<natives_dir>`.
pub fn extract_natives(native_jars: &[PathBuf], natives_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(natives_dir).map_err(|e| e.to_string())?;

    for jar_path in native_jars {
        if !jar_path.exists() {
            continue;
        }
        let file = match File::open(jar_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let name = entry.name().to_string();
            if name.starts_with("META-INF/") || entry.is_dir() {
                continue;
            }
            if name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib") {
                let file_name = Path::new(&name)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&name);
                let out_path = natives_dir.join(file_name);
                let mut buf = Vec::new();
                if entry.read_to_end(&mut buf).is_ok() {
                    let _ = std::fs::write(out_path, buf);
                }
            }
        }
    }
    Ok(())
}
