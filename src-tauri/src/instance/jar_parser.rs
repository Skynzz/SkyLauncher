use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModMetadata {
    pub file_name: String,
    pub enabled: bool,
    pub mod_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub loader: String,
    pub dependencies: Vec<String>,
    pub mixin_configs: Vec<String>,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackResourceEntry {
    pub file_name: String,
    pub category: String, // "resourcepacks" | "shaderpacks" | "config"
    pub size_bytes: u64,
    pub enabled: bool,
}

/// Scans an instance's `mods/` directory and parses metadata from `.jar` and `.jar.disabled`
/// files strictly in-memory using `zip::ZipArchive` without extracting anything to disk.
pub fn scan_instance_mods(game_dir: &Path) -> Result<Vec<ModMetadata>, String> {
    let mods_dir = game_dir.join("mods");
    if !mods_dir.exists() {
        let _ = fs::create_dir_all(&mods_dir);
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();
    let entries = fs::read_dir(&mods_dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let is_enabled = file_name.ends_with(".jar");
        let is_disabled = file_name.ends_with(".jar.disabled");
        if !is_enabled && !is_disabled {
            continue;
        }

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let parsed = match File::open(&path) {
            Ok(file) => parse_jar_from_reader(file, &file_name, is_enabled, size_bytes),
            Err(_) => fallback_metadata(&file_name, is_enabled, size_bytes),
        };
        mods.push(parsed);
    }

    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(mods)
}

/// Parses mod metadata in-memory from any `Read + Seek` stream (e.g., `File` or `Cursor<Vec<u8>>`).
/// Supports `fabric.mod.json`, `quilt.mod.json`, `META-INF/neoforge.mods.toml`, `META-INF/mods.toml`, and `mcmod.info`.
pub fn parse_jar_from_reader<R: Read + Seek>(
    reader: R,
    file_name: &str,
    enabled: bool,
    file_size_bytes: u64,
) -> ModMetadata {
    let mut archive = match ZipArchive::new(reader) {
        Ok(a) => a,
        Err(_) => return fallback_metadata(file_name, enabled, file_size_bytes),
    };

    // 1. Check `fabric.mod.json`
    if let Some(content) = read_zip_entry_string(&mut archive, "fabric.mod.json") {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            let mod_id = json
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let name = json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&mod_id)
                .to_string();
            let version = json
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0")
                .to_string();
            let description = json
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut authors = Vec::new();
            if let Some(arr) = json.get("authors").and_then(|v| v.as_array()) {
                for a in arr {
                    if let Some(s) = a.as_str() {
                        authors.push(s.to_string());
                    } else if let Some(n) = a.get("name").and_then(|v| v.as_str()) {
                        authors.push(n.to_string());
                    }
                }
            }

            let mut dependencies = Vec::new();
            if let Some(deps) = json.get("depends").and_then(|v| v.as_object()) {
                for (k, v) in deps {
                    let ver_str = v.as_str().unwrap_or("*");
                    dependencies.push(format!("{} ({})", k, ver_str));
                }
            }

            let mut mixin_configs = Vec::new();
            if let Some(mixins) = json.get("mixins").and_then(|v| v.as_array()) {
                for m in mixins {
                    if let Some(s) = m.as_str() {
                        mixin_configs.push(s.to_string());
                    } else if let Some(cfg) = m.get("config").and_then(|v| v.as_str()) {
                        mixin_configs.push(cfg.to_string());
                    }
                }
            }

            return ModMetadata {
                file_name: file_name.to_string(),
                enabled,
                mod_id,
                name,
                version,
                description,
                authors,
                loader: "Fabric".to_string(),
                dependencies,
                mixin_configs,
                file_size_bytes,
            };
        }
    }

    // 2. Check `quilt.mod.json`
    if let Some(content) = read_zip_entry_string(&mut archive, "quilt.mod.json") {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            if let Some(ql) = json.get("quilt_loader") {
                let mod_id = ql
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let version = ql
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.0.0")
                    .to_string();
                let name = ql
                    .pointer("/metadata/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&mod_id)
                    .to_string();
                let description = ql
                    .pointer("/metadata/description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut mixin_configs = Vec::new();
                if let Some(m_str) = json.get("mixin").and_then(|v| v.as_str()) {
                    mixin_configs.push(m_str.to_string());
                } else if let Some(m_arr) = json.get("mixin").and_then(|v| v.as_array()) {
                    for m in m_arr {
                        if let Some(s) = m.as_str() {
                            mixin_configs.push(s.to_string());
                        }
                    }
                }

                return ModMetadata {
                    file_name: file_name.to_string(),
                    enabled,
                    mod_id,
                    name,
                    version,
                    description,
                    authors: vec![],
                    loader: "Quilt".to_string(),
                    dependencies: vec![],
                    mixin_configs,
                    file_size_bytes,
                };
            }
        }
    }

    // 3. Check `META-INF/neoforge.mods.toml` or `META-INF/mods.toml`
    let toml_entry = read_zip_entry_string(&mut archive, "META-INF/neoforge.mods.toml")
        .map(|c| ("NeoForge", c))
        .or_else(|| {
            read_zip_entry_string(&mut archive, "META-INF/mods.toml").map(|c| ("Forge", c))
        });

    if let Some((loader_label, content)) = toml_entry {
        if let Ok(parsed_toml) = content.parse::<toml::Table>() {
            if let Some(mods_arr) = parsed_toml.get("mods").and_then(|v| v.as_array()) {
                if let Some(first_mod) = mods_arr.first().and_then(|v| v.as_table()) {
                    let mod_id = first_mod
                        .get("modId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let name = first_mod
                        .get("displayName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&mod_id)
                        .to_string();
                    let version = first_mod
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1.0.0")
                        .to_string();
                    let description = first_mod
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let authors = first_mod
                        .get("authors")
                        .and_then(|v| v.as_str())
                        .map(|s| vec![s.to_string()])
                        .unwrap_or_default();

                    let mut dependencies = Vec::new();
                    if let Some(deps_table) =
                        parsed_toml.get("dependencies").and_then(|v| v.as_table())
                    {
                        if let Some(dep_list) =
                            deps_table.get(&mod_id).and_then(|v| v.as_array())
                        {
                            for dep in dep_list {
                                if let Some(d_id) = dep.get("modId").and_then(|v| v.as_str()) {
                                    let range = dep
                                        .get("versionRange")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("*");
                                    dependencies.push(format!("{} {}", d_id, range));
                                }
                            }
                        }
                    }

                    let mut mixin_configs = Vec::new();
                    if let Some(mixins_arr) = parsed_toml.get("mixins").and_then(|v| v.as_array())
                    {
                        for m in mixins_arr {
                            if let Some(cfg) = m.get("config").and_then(|v| v.as_str()) {
                                mixin_configs.push(cfg.to_string());
                            }
                        }
                    }

                    // Also inspect META-INF/MANIFEST.MF for MixinConfigs header
                    if let Some(manifest_mf) =
                        read_zip_entry_string(&mut archive, "META-INF/MANIFEST.MF")
                    {
                        for line in manifest_mf.lines() {
                            if let Some(rest) = line.strip_prefix("MixinConfigs:") {
                                for cfg in rest.split(',') {
                                    let trimmed = cfg.trim();
                                    if !trimmed.is_empty()
                                        && !mixin_configs.contains(&trimmed.to_string())
                                    {
                                        mixin_configs.push(trimmed.to_string());
                                    }
                                }
                            }
                        }
                    }

                    return ModMetadata {
                        file_name: file_name.to_string(),
                        enabled,
                        mod_id,
                        name,
                        version,
                        description,
                        authors,
                        loader: loader_label.to_string(),
                        dependencies,
                        mixin_configs,
                        file_size_bytes,
                    };
                }
            }
        }
    }

    fallback_metadata(file_name, enabled, file_size_bytes)
}

fn read_zip_entry_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_name: &str,
) -> Option<String> {
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).ok()?;
    Some(buf)
}

fn fallback_metadata(file_name: &str, enabled: bool, file_size_bytes: u64) -> ModMetadata {
    let clean = file_name
        .trim_end_matches(".disabled")
        .trim_end_matches(".jar");
    ModMetadata {
        file_name: file_name.to_string(),
        enabled,
        mod_id: clean.to_lowercase(),
        name: clean.to_string(),
        version: "unknown".to_string(),
        description: "Metadata not embedded or legacy format.".to_string(),
        authors: vec![],
        loader: "Unknown".to_string(),
        dependencies: vec![],
        mixin_configs: vec![],
        file_size_bytes,
    }
}

/// Toggles a mod between `.jar` and `.jar.disabled` inside `<game_dir>/mods/`.
pub fn toggle_mod_file(game_dir: &Path, file_name: &str) -> Result<String, String> {
    let mods_dir = game_dir.join("mods");
    let current_path = mods_dir.join(file_name);
    if !current_path.exists() {
        return Err(format!("Mod file '{}' does not exist.", file_name));
    }

    let new_file_name = if file_name.ends_with(".jar.disabled") {
        file_name.trim_end_matches(".disabled").to_string()
    } else if file_name.ends_with(".jar") {
        format!("{}.disabled", file_name)
    } else {
        return Err("Unsupported mod file extension.".to_string());
    };

    let target_path = mods_dir.join(&new_file_name);
    fs::rename(&current_path, &target_path)
        .map_err(|e| format!("Failed to rename mod '{}': {}", file_name, e))?;

    Ok(new_file_name)
}

/// Lists files in `resourcepacks/`, `shaderpacks/`, and `config/` for the Mod Manager UI.
pub fn list_pack_resources(game_dir: &Path, category: &str) -> Result<Vec<PackResourceEntry>, String> {
    let allowed = ["resourcepacks", "shaderpacks", "config"];
    if !allowed.contains(&category) {
        return Err(format!("Invalid resource category '{}'", category));
    }

    let target_dir = game_dir.join(category);
    if !target_dir.exists() {
        let _ = fs::create_dir_all(&target_dir);
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&target_dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let enabled = !file_name.ends_with(".disabled") && !file_name.ends_with(".bak");
        items.push(PackResourceEntry {
            file_name,
            category: category.to_string(),
            size_bytes,
            enabled,
        });
    }

    items.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(items)
}

/// Deletes a mod or resource file from `mods/`, `resourcepacks/`, `shaderpacks/`, or `config/`.
pub fn delete_pack_resource(
    game_dir: &Path,
    category: &str,
    file_name: &str,
) -> Result<(), String> {
    let allowed = ["mods", "resourcepacks", "shaderpacks", "config"];
    if !allowed.contains(&category) {
        return Err(format!("Geçersiz kategori: '{}'", category));
    }
    if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        return Err("Geçersiz dosya adı.".to_string());
    }
    let target_path = game_dir.join(category).join(file_name);
    if !target_path.exists() {
        return Err(format!("Dosya bulunamadı: '{}'", file_name));
    }
    if target_path.is_dir() {
        fs::remove_dir_all(&target_path).map_err(|e| e.to_string())?;
    } else {
        fs::remove_file(&target_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Finds the culprit `.jar` file in `mods/` that owns a specific Mixin configuration file
/// (e.g. `sodium.mixins.json`) or mod ID.
pub fn find_culprit_jar_by_mixin_or_modid(game_dir: &Path, needle: &str) -> Option<String> {
    let mods = scan_instance_mods(game_dir).ok()?;
    let needle_lower = needle.trim().to_lowercase();

    for m in &mods {
        if !m.enabled {
            continue;
        }
        if m.mixin_configs
            .iter()
            .any(|cfg| cfg.to_lowercase() == needle_lower || needle_lower.contains(&cfg.to_lowercase()))
        {
            return Some(m.file_name.clone());
        }
        if m.mod_id.to_lowercase() == needle_lower
            || m.file_name.to_lowercase().contains(&needle_lower)
        {
            return Some(m.file_name.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_in_memory_jar_parser_fabric_and_forge() {
        // 1. Build an in-memory Fabric mod JAR without touching the disk
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
            let options = SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(
                br#"{
                    "schemaVersion": 1,
                    "id": "sodium",
                    "version": "0.5.8+mc1.20.6",
                    "name": "Sodium",
                    "description": "Modern rendering engine for Minecraft",
                    "authors": ["JellySquid"],
                    "mixins": ["sodium.mixins.json"],
                    "depends": {
                        "fabricloader": ">=0.15.0",
                        "minecraft": "~1.20.6"
                    }
                }"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }

        let meta = parse_jar_from_reader(
            Cursor::new(buffer.clone()),
            "sodium-fabric-0.5.8.jar",
            true,
            buffer.len() as u64,
        );
        assert_eq!(meta.mod_id, "sodium");
        assert_eq!(meta.name, "Sodium");
        assert_eq!(meta.loader, "Fabric");
        assert_eq!(meta.mixin_configs, vec!["sodium.mixins.json".to_string()]);
        assert!(meta.dependencies.iter().any(|d| d.contains("fabricloader")));

        // 2. Build an in-memory NeoForge mod JAR (`META-INF/neoforge.mods.toml`)
        let mut nf_buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut nf_buffer));
            let options = SimpleFileOptions::default();
            zip.start_file("META-INF/neoforge.mods.toml", options)
                .unwrap();
            zip.write_all(
                br#"
                modLoader="javafml"
                loaderVersion="[4,)"
                license="MIT"

                [[mods]]
                modId="jei"
                version="19.8.2"
                displayName="Just Enough Items"
                authors="mezz"
                description="Item and recipe viewing mod"

                [[mixins]]
                config="jei.mixins.json"
                "#,
            )
            .unwrap();
            zip.finish().unwrap();
        }

        let nf_meta = parse_jar_from_reader(
            Cursor::new(nf_buffer.clone()),
            "jei-1.21-neoforge-19.8.2.jar",
            true,
            nf_buffer.len() as u64,
        );
        assert_eq!(nf_meta.mod_id, "jei");
        assert_eq!(nf_meta.name, "Just Enough Items");
        assert_eq!(nf_meta.loader, "NeoForge");
        assert_eq!(nf_meta.mixin_configs, vec!["jei.mixins.json".to_string()]);
    }
}
