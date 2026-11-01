use crate::downloader::modloaders::ModloaderType;
use crate::downloader::{DownloadEngine, DownloadProgressEvent, DownloadTask};
use crate::instance::jar_parser::scan_instance_mods;
use crate::instance::{InstanceConfig, InstanceManager};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter};

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com/v1";
// Public fallback key if user hasn't provided their own in settings
const DEFAULT_CF_API_KEY: &str = "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl ContentType {
    pub fn modrinth_project_type(&self) -> &'static str {
        match self {
            ContentType::Mod => "mod",
            ContentType::Modpack => "modpack",
            ContentType::ResourcePack => "resourcepack",
            ContentType::Shader => "shader",
        }
    }

    pub fn curseforge_class_id(&self) -> u32 {
        match self {
            ContentType::Mod => 6,
            ContentType::Modpack => 4471,
            ContentType::ResourcePack => 12,
            ContentType::Shader => 6552,
        }
    }

    pub fn target_subfolder(&self) -> &'static str {
        match self {
            ContentType::Mod => "mods",
            ContentType::Modpack => "mods",
            ContentType::ResourcePack => "resourcepacks",
            ContentType::Shader => "shaderpacks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoreProvider {
    Modrinth,
    CurseForge,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSearchItem {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub provider: String, // "modrinth" | "curseforge"
    pub content_type: ContentType,
    pub categories: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreDependencyBadge {
    pub project_id: String,
    pub dependency_type: String, // "required" | "optional"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreVersionEntry {
    pub version_id: String,
    pub version_number: String,
    pub name: String,
    pub file_name: String,
    pub download_url: String,
    pub sha1: Option<String>,
    pub size_bytes: u64,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub dependencies: Vec<StoreDependencyBadge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreProjectDetails {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub gallery_urls: Vec<String>,
    pub provider: String,
    pub content_type: ContentType,
    pub downloads: u64,
    pub followers: u64,
    pub updated_at: String,
    pub license: Option<String>,
    pub source_url: Option<String>,
    pub issues_url: Option<String>,
    pub wiki_url: Option<String>,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub versions: Vec<StoreVersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdateInfo {
    pub file_name: String,
    pub current_sha1: String,
    pub project_id: String,
    pub latest_version_id: String,
    pub latest_version_number: String,
    pub latest_file_name: String,
    pub download_url: String,
    pub latest_sha1: Option<String>,
}

/// Normalizes a mod/modpack title or slug into a canonical alphanumeric key so
/// identical projects across Modrinth and CurseForge can be deduplicated.
pub fn normalize_mod_key(input: &str) -> String {
    let lower = input.to_lowercase();
    let stripped = lower
        .trim_end_matches(" (fabric)")
        .trim_end_matches(" (forge)")
        .trim_end_matches(" (neoforge)")
        .trim_end_matches(" [fabric]")
        .trim_end_matches(" [forge]")
        .trim_end_matches(" [fabric/forge]");
    stripped
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Constructs a CurseForge direct CDN URL when `downloadUrl` is null due to 3rd-party distribution flags.
/// Rule: `https://edge.forgecdn.net/files/{id_part_1}/{id_part_2}/{file_name}`
/// where `id_part_1 = file_id / 1000` and `id_part_2 = file_id % 1000`.
pub fn resolve_curseforge_cdn_url(file_id: u64, file_name: &str) -> String {
    let part1 = file_id / 1000;
    let part2 = file_id % 1000;
    let encoded_name = file_name
        .replace(' ', "%20")
        .replace('+', "%2B")
        .replace('[', "%5B")
        .replace(']', "%5D");
    format!(
        "https://edge.forgecdn.net/files/{}/{}/{}",
        part1, part2, encoded_name
    )
}

/// Places a downloaded mod/resource into the Shared Mod Pool (`<appdata>/shared/mod_pool/`)
/// and links it into the target destination via OS hardlink (`std::fs::hard_link` with `copy` fallback).
pub fn link_from_shared_pool_or_copy(pool_file: &Path, target_file: &Path) -> Result<(), String> {
    if let Some(parent) = target_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if target_file.exists() {
        let _ = fs::remove_file(target_file);
    }
    if fs::hard_link(pool_file, target_file).is_err() {
        fs::copy(pool_file, target_file)
            .map_err(|e| format!("Dosya hedef klasöre kopyalanamadı: {}", e))?;
    }
    Ok(())
}

/// Searches Modrinth API v2 and/or CurseForge Core API v1 with category filter, pagination offset,
/// and cross-provider deduplication when `StoreProvider::Both` is active.
pub async fn search_store_content(
    client: &Client,
    query: &str,
    content_type: ContentType,
    provider: StoreProvider,
    sort_by: &str, // "downloads" | "updated" | "relevance"
    mc_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u32,
    cf_api_key: Option<&str>,
) -> Result<Vec<StoreSearchItem>, String> {
    let mut results = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    let query_modrinth = matches!(provider, StoreProvider::Modrinth | StoreProvider::Both);
    let query_cf = matches!(provider, StoreProvider::CurseForge | StoreProvider::Both);

    if query_modrinth {
        if let Ok(mr_items) = search_modrinth(
            client,
            query,
            content_type,
            sort_by,
            mc_version,
            loader,
            category,
            offset,
        )
        .await
        {
            for item in mr_items {
                let title_key = normalize_mod_key(&item.title);
                let slug_key = normalize_mod_key(&item.slug);
                if !title_key.is_empty() {
                    seen_keys.insert(title_key);
                }
                if !slug_key.is_empty() {
                    seen_keys.insert(slug_key);
                }
                results.push(item);
            }
        }
    }

    if query_cf {
        let key = cf_api_key
            .filter(|k| !k.trim().is_empty())
            .unwrap_or(DEFAULT_CF_API_KEY);
        if let Ok(cf_items) = search_curseforge(
            client,
            query,
            content_type,
            sort_by,
            mc_version,
            loader,
            category,
            offset,
            key,
        )
        .await
        {
            for item in cf_items {
                let title_key = normalize_mod_key(&item.title);
                let slug_key = normalize_mod_key(&item.slug);
                if (!title_key.is_empty() && seen_keys.contains(&title_key))
                    || (!slug_key.is_empty() && seen_keys.contains(&slug_key))
                {
                    // Skip duplicate already returned from Modrinth
                    continue;
                }
                if !title_key.is_empty() {
                    seen_keys.insert(title_key);
                }
                if !slug_key.is_empty() {
                    seen_keys.insert(slug_key);
                }
                results.push(item);
            }
        }
    }

    Ok(results)
}

async fn search_modrinth(
    client: &Client,
    query: &str,
    content_type: ContentType,
    sort_by: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u32,
) -> Result<Vec<StoreSearchItem>, String> {
    let mut facets: Vec<Vec<String>> = vec![vec![format!(
        "project_type:{}",
        content_type.modrinth_project_type()
    )]];

    if let Some(ver) = mc_version.filter(|s| !s.trim().is_empty()) {
        facets.push(vec![format!("versions:{}", ver.trim())]);
    }

    // Only lock loader facet for Mods and Modpacks (ResourcePacks and Shaders are loader-agnostic)
    if matches!(content_type, ContentType::Mod | ContentType::Modpack) {
        if let Some(ldr) = loader.filter(|s| !s.trim().is_empty() && *s != "vanilla") {
            facets.push(vec![format!("categories:{}", ldr.trim().to_lowercase())]);
        }
    }

    if let Some(cat) = category.filter(|s| !s.trim().is_empty() && *s != "all") {
        let cat_lower = cat.trim().to_lowercase();
        let mr_cat = match cat_lower.as_str() {
            "technology" => "technology",
            "magic" => "magic",
            "adventure" => "adventure",
            "optimization" => "optimization",
            "worldgen" => "worldgen",
            "decoration" => "decoration",
            "storage" => "storage",
            "utility" => "utility",
            "quests" => "quests",
            "library" => "library",
            "equipment" => "equipment",
            "mobs" => "mobs",
            other => other,
        };
        facets.push(vec![format!("categories:{}", mr_cat)]);
    }

    let facets_json = serde_json::to_string(&facets).unwrap_or_else(|_| "[]".to_string());
    let index = match sort_by {
        "updated" => "updated",
        "relevance" => "relevance",
        _ => "downloads",
    };
    let offset_str = offset.to_string();

    let resp = client
        .get(format!("{}/search", MODRINTH_API_BASE))
        .query(&[
            ("query", query),
            ("facets", &facets_json),
            ("index", index),
            ("limit", "24"),
            ("offset", &offset_str),
        ])
        .send()
        .await
        .map_err(|e| format!("Modrinth arama hatası: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth HTTP {}", resp.status()));
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut items = Vec::new();

    if let Some(hits) = json.get("hits").and_then(|v| v.as_array()) {
        for hit in hits {
            let project_id = hit
                .get("project_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if project_id.is_empty() {
                continue;
            }
            let slug = hit
                .get("slug")
                .and_then(|v| v.as_str())
                .unwrap_or(&project_id)
                .to_string();
            let title = hit
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(&slug)
                .to_string();
            let description = hit
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let author = hit
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("Modrinth")
                .to_string();
            let icon_url = hit
                .get("icon_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let downloads = hit.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0);
            let updated_at = hit
                .get("date_modified")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let categories = hit
                .get("display_categories")
                .or_else(|| hit.get("categories"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            items.push(StoreSearchItem {
                project_id,
                slug,
                title,
                description,
                author,
                icon_url,
                downloads,
                provider: "modrinth".to_string(),
                content_type,
                categories,
                updated_at,
            });
        }
    }

    Ok(items)
}

async fn search_curseforge(
    client: &Client,
    query: &str,
    content_type: ContentType,
    sort_by: &str,
    mc_version: Option<&str>,
    loader: Option<&str>,
    category: Option<&str>,
    offset: u32,
    api_key: &str,
) -> Result<Vec<StoreSearchItem>, String> {
    let class_id = content_type.curseforge_class_id().to_string();
    let sort_field = match sort_by {
        "updated" => "3",   // LastUpdated
        "relevance" => "1", // Featured
        _ => "6",           // TotalDownloads
    };

    let mut params = vec![
        ("gameId", "432".to_string()),
        ("classId", class_id),
        ("searchFilter", query.to_string()),
        ("sortField", sort_field.to_string()),
        ("sortOrder", "desc".to_string()),
        ("pageSize", "24".to_string()),
        ("index", offset.to_string()),
    ];

    if let Some(ver) = mc_version.filter(|s| !s.trim().is_empty()) {
        params.push(("gameVersion", ver.trim().to_string()));
    }

    if matches!(content_type, ContentType::Mod | ContentType::Modpack) {
        if let Some(ldr) = loader {
            let cf_loader_type = match ldr.to_lowercase().as_str() {
                "forge" => Some("1"),
                "fabric" => Some("4"),
                "quilt" => Some("5"),
                "neoforge" => Some("6"),
                _ => None,
            };
            if let Some(lt) = cf_loader_type {
                params.push(("modLoaderType", lt.to_string()));
            }
        }
    }

    if let Some(cat) = category.filter(|s| !s.trim().is_empty() && *s != "all") {
        let cf_cat_id = match (content_type, cat.trim().to_lowercase().as_str()) {
            (ContentType::Modpack, "technology") => Some("4472"),
            (ContentType::Modpack, "magic") => Some("4473"),
            (ContentType::Modpack, "adventure") => Some("4475"),
            (ContentType::Modpack, "quests") => Some("4478"),
            (ContentType::Modpack, "optimization") => Some("4481"),
            (_, "technology") => Some("412"),
            (_, "magic") => Some("419"),
            (_, "adventure") => Some("422"),
            (_, "worldgen") => Some("406"),
            (_, "decoration") => Some("424"),
            (_, "storage") => Some("420"),
            (_, "utility") => Some("5191"),
            (_, "library") => Some("421"),
            (_, "equipment") => Some("434"),
            (_, "mobs") => Some("411"),
            _ => None,
        };
        if let Some(cid) = cf_cat_id {
            params.push(("categoryId", cid.to_string()));
        }
    }

    let resp = client
        .get(format!("{}/mods/search", CURSEFORGE_API_BASE))
        .header("x-api-key", api_key)
        .query(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("CurseForge HTTP {}", resp.status()));
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut items = Vec::new();

    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
        for m in data {
            let id = m.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            if id == 0 {
                continue;
            }
            let title = m
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("CurseForge Pack")
                .to_string();
            let slug = m
                .get("slug")
                .and_then(|v| v.as_str())
                .unwrap_or(&title)
                .to_string();
            let description = m
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let author = m
                .pointer("/authors/0/name")
                .and_then(|v| v.as_str())
                .unwrap_or("CurseForge")
                .to_string();
            let icon_url = m
                .pointer("/logo/thumbnailUrl")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let downloads = m.get("downloadCount").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64;
            let updated_at = m
                .get("dateModified")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let categories = m
                .get("categories")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            items.push(StoreSearchItem {
                project_id: id.to_string(),
                slug,
                title,
                description,
                author,
                icon_url,
                downloads,
                provider: "curseforge".to_string(),
                content_type,
                categories,
                updated_at,
            });
        }
    }

    Ok(items)
}

/// Fetches detailed metadata, gallery screenshots, and filtered versions for a store project.
pub async fn get_store_project_details(
    client: &Client,
    project_id: &str,
    provider: &str,
    content_type: ContentType,
    mc_version: Option<&str>,
    loader: Option<&str>,
    cf_api_key: Option<&str>,
) -> Result<StoreProjectDetails, String> {
    if provider.eq_ignore_ascii_case("curseforge") {
        let key = cf_api_key
            .filter(|k| !k.trim().is_empty())
            .unwrap_or(DEFAULT_CF_API_KEY);
        return get_curseforge_project_details(
            client,
            project_id,
            content_type,
            mc_version,
            loader,
            key,
        )
        .await;
    }

    let proj_resp = client
        .get(format!("{}/project/{}", MODRINTH_API_BASE, project_id))
        .send()
        .await
        .map_err(|e| format!("Modrinth proje detayı alınamadı: {}", e))?;

    if !proj_resp.status().is_success() {
        return Err(format!("Modrinth proje HTTP {}", proj_resp.status()));
    }
    let proj_json: Value = proj_resp.json().await.map_err(|e| e.to_string())?;

    let slug = proj_json
        .get("slug")
        .and_then(|v| v.as_str())
        .unwrap_or(project_id)
        .to_string();
    let title = proj_json
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&slug)
        .to_string();
    let description = proj_json
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let body = proj_json
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or(&description)
        .to_string();
    let icon_url = proj_json
        .get("icon_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let downloads = proj_json
        .get("downloads")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let followers = proj_json
        .get("followers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let updated_at = proj_json
        .get("updated")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let license = proj_json
        .pointer("/license/id")
        .or_else(|| proj_json.pointer("/license/name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let source_url = proj_json
        .get("source_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let issues_url = proj_json
        .get("issues_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let wiki_url = proj_json
        .get("wiki_url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let categories = proj_json
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let game_versions = proj_json
        .get("game_versions")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let loaders = proj_json
        .get("loaders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let gallery_urls = proj_json
        .get("gallery")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|g| g.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let versions = fetch_modrinth_versions(client, project_id, content_type, mc_version, loader).await?;

    Ok(StoreProjectDetails {
        project_id: project_id.to_string(),
        slug,
        title,
        description,
        body,
        author: "Modrinth".to_string(),
        icon_url,
        gallery_urls,
        provider: "modrinth".to_string(),
        content_type,
        downloads,
        followers,
        updated_at,
        license,
        source_url,
        issues_url,
        wiki_url,
        categories,
        game_versions,
        loaders,
        versions,
    })
}

pub async fn fetch_modrinth_versions(
    client: &Client,
    project_id: &str,
    content_type: ContentType,
    mc_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<StoreVersionEntry>, String> {
    let mut query_params: Vec<(&str, String)> = Vec::new();
    if let Some(ver) = mc_version.filter(|s| !s.trim().is_empty()) {
        query_params.push(("game_versions", format!("[\"{}\"]", ver.trim())));
    }
    if matches!(content_type, ContentType::Mod | ContentType::Modpack) {
        if let Some(ldr) = loader.filter(|s| !s.trim().is_empty() && *s != "vanilla") {
            query_params.push(("loaders", format!("[\"{}\"]", ldr.trim().to_lowercase())));
        }
    }

    let resp = client
        .get(format!("{}/project/{}/version", MODRINTH_API_BASE, project_id))
        .query(&query_params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth sürümleri alınamadı: HTTP {}", resp.status()));
    }

    let arr: Vec<Value> = resp.json().await.map_err(|e| e.to_string())?;
    let mut entries = Vec::new();

    for v in arr.into_iter().take(25) {
        if let Some(parsed) = parse_modrinth_version_value(&v) {
            entries.push(parsed);
        }
    }

    Ok(entries)
}

fn parse_modrinth_version_value(v: &Value) -> Option<StoreVersionEntry> {
    let version_id = v.get("id")?.as_str()?.to_string();
    let version_number = v
        .get("version_number")
        .and_then(|x| x.as_str())
        .unwrap_or(&version_id)
        .to_string();
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or(&version_number)
        .to_string();

    let files = v.get("files")?.as_array()?;
    let primary_file = files
        .iter()
        .find(|f| f.get("primary").and_then(|p| p.as_bool()).unwrap_or(false))
        .or_else(|| files.first())?;

    let file_name = primary_file.get("filename")?.as_str()?.to_string();
    let download_url = primary_file.get("url")?.as_str()?.to_string();
    let sha1 = primary_file
        .pointer("/hashes/sha1")
        .and_then(|h| h.as_str())
        .map(|s| s.to_string());
    let size_bytes = primary_file
        .get("size")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let game_versions = v
        .get("game_versions")
        .and_then(|gv| gv.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let loaders = v
        .get("loaders")
        .and_then(|ld| ld.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut dependencies = Vec::new();
    if let Some(deps) = v.get("dependencies").and_then(|d| d.as_array()) {
        for d in deps {
            let dep_type = d
                .get("dependency_type")
                .and_then(|t| t.as_str())
                .unwrap_or("optional");
            if let Some(pid) = d
                .get("project_id")
                .and_then(|p| p.as_str())
                .filter(|s| !s.is_empty())
            {
                dependencies.push(StoreDependencyBadge {
                    project_id: pid.to_string(),
                    dependency_type: dep_type.to_string(),
                });
            } else if let Some(vid) = d
                .get("version_id")
                .and_then(|p| p.as_str())
                .filter(|s| !s.is_empty())
            {
                dependencies.push(StoreDependencyBadge {
                    project_id: format!("ver:{}", vid),
                    dependency_type: dep_type.to_string(),
                });
            }
        }
    }

    Some(StoreVersionEntry {
        version_id,
        version_number,
        name,
        file_name,
        download_url,
        sha1,
        size_bytes,
        game_versions,
        loaders,
        dependencies,
    })
}

async fn get_curseforge_project_details(
    client: &Client,
    project_id: &str,
    content_type: ContentType,
    mc_version: Option<&str>,
    loader: Option<&str>,
    api_key: &str,
) -> Result<StoreProjectDetails, String> {
    let resp = client
        .get(format!("{}/mods/{}", CURSEFORGE_API_BASE, project_id))
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("CurseForge HTTP {}", resp.status()));
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let data = json.get("data").ok_or("CurseForge data alanı boş.")?;

    let slug = data
        .get("slug")
        .and_then(|v| v.as_str())
        .unwrap_or(project_id)
        .to_string();
    let title = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(project_id)
        .to_string();
    let description = data
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let author = data
        .pointer("/authors/0/name")
        .and_then(|v| v.as_str())
        .unwrap_or("CurseForge")
        .to_string();
    let downloads = data
        .get("downloadCount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as u64;
    let followers = data
        .get("thumbsUpCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let updated_at = data
        .get("dateModified")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source_url = data
        .pointer("/links/sourceUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let issues_url = data
        .pointer("/links/issuesUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let wiki_url = data
        .pointer("/links/wikiUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let categories = data
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let icon_url = data
        .pointer("/logo/thumbnailUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let gallery_urls = data
        .get("screenshots")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("url").and_then(|u| u.as_str()).map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Also try fetching full HTML description from CurseForge
    let mut full_body = description.clone();
    if let Ok(desc_resp) = client
        .get(format!(
            "{}/mods/{}/description",
            CURSEFORGE_API_BASE, project_id
        ))
        .header("x-api-key", api_key)
        .send()
        .await
    {
        if desc_resp.status().is_success() {
            if let Ok(desc_json) = desc_resp.json::<Value>().await {
                if let Some(html) = desc_json.get("data").and_then(|v| v.as_str()) {
                    if !html.trim().is_empty() {
                        full_body = html.to_string();
                    }
                }
            }
        }
    }

    let mut query_params: Vec<(&str, String)> = vec![("pageSize", "20".to_string())];
    if let Some(ver) = mc_version.filter(|s| !s.trim().is_empty()) {
        query_params.push(("gameVersion", ver.trim().to_string()));
    }
    if matches!(content_type, ContentType::Mod | ContentType::Modpack) {
        if let Some(ldr) = loader {
            let cf_loader_type = match ldr.to_lowercase().as_str() {
                "forge" => Some("1"),
                "fabric" => Some("4"),
                "quilt" => Some("5"),
                "neoforge" => Some("6"),
                _ => None,
            };
            if let Some(lt) = cf_loader_type {
                query_params.push(("modLoaderType", lt.to_string()));
            }
        }
    }

    let files_resp = client
        .get(format!("{}/mods/{}/files", CURSEFORGE_API_BASE, project_id))
        .header("x-api-key", api_key)
        .query(&query_params)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut versions = Vec::new();
    let mut all_game_versions = Vec::new();
    let mut all_loaders = Vec::new();

    if files_resp.status().is_success() {
        if let Ok(files_json) = files_resp.json::<Value>().await {
            if let Some(arr) = files_json.get("data").and_then(|v| v.as_array()) {
                for f in arr {
                    let file_id = f.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let file_name = f
                        .get("fileName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("mod.jar")
                        .to_string();
                    let display_name = f
                        .get("displayName")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&file_name)
                        .to_string();
                    let download_url = f
                        .get("downloadUrl")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| resolve_curseforge_cdn_url(file_id, &file_name));

                    let mut sha1 = None;
                    if let Some(hashes) = f.get("hashes").and_then(|h| h.as_array()) {
                        for h in hashes {
                            // algo 1 = SHA1 in CurseForge API
                            if h.get("algo").and_then(|a| a.as_u64()) == Some(1) {
                                sha1 = h.get("value").and_then(|v| v.as_str()).map(|s| s.to_string());
                            }
                        }
                    }

                    let mut file_gvs = Vec::new();
                    let mut file_loaders = Vec::new();
                    if let Some(gvs) = f.get("gameVersions").and_then(|g| g.as_array()) {
                        for gv in gvs {
                            if let Some(s) = gv.as_str() {
                                let lower = s.to_lowercase();
                                if matches!(lower.as_str(), "forge" | "fabric" | "neoforge" | "quilt") {
                                    file_loaders.push(s.to_string());
                                    if !all_loaders.contains(&s.to_string()) {
                                        all_loaders.push(s.to_string());
                                    }
                                } else if s.starts_with("1.") || s.starts_with("2") {
                                    file_gvs.push(s.to_string());
                                    if !all_game_versions.contains(&s.to_string()) {
                                        all_game_versions.push(s.to_string());
                                    }
                                }
                            }
                        }
                    }

                    let mut dependencies = Vec::new();
                    if let Some(deps) = f.get("dependencies").and_then(|d| d.as_array()) {
                        for d in deps {
                            // relationType 3 == RequiredDependency
                            let rel = d.get("relationType").and_then(|r| r.as_u64()).unwrap_or(0);
                            if let Some(mod_id) = d.get("modId").and_then(|m| m.as_u64()) {
                                dependencies.push(StoreDependencyBadge {
                                    project_id: mod_id.to_string(),
                                    dependency_type: if rel == 3 {
                                        "required".to_string()
                                    } else {
                                        "optional".to_string()
                                    },
                                });
                            }
                        }
                    }

                    versions.push(StoreVersionEntry {
                        version_id: file_id.to_string(),
                        version_number: display_name.clone(),
                        name: display_name,
                        file_name,
                        download_url,
                        sha1,
                        size_bytes: f.get("fileLength").and_then(|l| l.as_u64()).unwrap_or(0),
                        game_versions: file_gvs,
                        loaders: file_loaders,
                        dependencies,
                    });
                }
            }
        }
    }

    Ok(StoreProjectDetails {
        project_id: project_id.to_string(),
        slug,
        title,
        description,
        body: full_body,
        author,
        icon_url,
        gallery_urls,
        provider: "curseforge".to_string(),
        content_type,
        downloads,
        followers,
        updated_at,
        license: None,
        source_url,
        issues_url,
        wiki_url,
        categories,
        game_versions: all_game_versions,
        loaders: all_loaders,
        versions,
    })
}

/// Recursively resolves a mod and its required dependencies, downloading into the Shared Mod Pool
/// and hardlinking into the target instance's subfolder (`mods/`, `resourcepacks/`, `shaderpacks/`).
pub async fn install_store_content_with_dependencies(
    instance: &InstanceConfig,
    app_data_dir: &Path,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    project_id: &str,
    provider: &str,
    content_type: ContentType,
    version_id: Option<&str>,
    cf_api_key: Option<&str>,
) -> Result<Vec<String>, String> {
    downloader.reset_cancel();
    let client = downloader.client();
    let pool_dir = app_data_dir.join("shared").join("mod_pool");
    let _ = fs::create_dir_all(&pool_dir);

    let target_dir = instance.game_dir.join(content_type.target_subfolder());
    let _ = fs::create_dir_all(&target_dir);

    // Gather existing mod IDs and filenames to skip already-installed dependencies
    let existing_mods = scan_instance_mods(&instance.game_dir).unwrap_or_default();
    let mut existing_filenames: HashSet<String> = existing_mods
        .iter()
        .map(|m| m.file_name.trim_end_matches(".disabled").to_lowercase())
        .collect();
    let mut existing_norm_ids: HashSet<String> = existing_mods
        .iter()
        .filter(|m| m.enabled)
        .flat_map(|m| {
            let norm = normalize_mod_identifier(&m.mod_id);
            let mut ids = vec![norm.clone()];
            if norm == "fabricapi" || norm == "fabric" {
                ids.push("fabricapi".to_string());
                ids.push("fabric".to_string());
            }
            ids
        })
        .collect();

    let mut visited_projects: HashSet<String> = HashSet::new();
    let mut pending_queue: Vec<(String, Option<String>, usize)> = vec![(
        project_id.to_string(),
        version_id.map(|s| s.to_string()),
        0,
    )];

    let mut resolved_entries: Vec<StoreVersionEntry> = Vec::new();
    let loader_str = instance.modloader.as_str();

    while let Some((raw_proj_id, curr_ver_id, depth)) = pending_queue.pop() {
        if depth > 6 {
            continue;
        }

        // Resolve "ver:{version_id}" to actual project_id if needed
        let (curr_proj_id, resolved_ver_id) = if let Some(vid) = raw_proj_id.strip_prefix("ver:") {
            if let Ok(resp) = client
                .get(format!("{}/version/{}", MODRINTH_API_BASE, vid))
                .send()
                .await
            {
                if let Ok(vjson) = resp.json::<Value>().await {
                    if let Some(pid) = vjson.get("project_id").and_then(|p| p.as_str()) {
                        (pid.to_string(), Some(vid.to_string()))
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            (raw_proj_id, curr_ver_id)
        };

        if !visited_projects.insert(curr_proj_id.clone()) {
            continue;
        }

        let details = match get_store_project_details(
            client,
            &curr_proj_id,
            provider,
            content_type,
            Some(&instance.mc_version),
            Some(loader_str),
            cf_api_key,
        )
        .await
        {
            Ok(d) => d,
            Err(e) => {
                if depth == 0 {
                    return Err(e);
                }
                continue;
            }
        };

        // Skip if a dependency with the same normalized slug/id is already installed
        let norm_slug = normalize_mod_identifier(&details.slug);
        if depth > 0 && existing_norm_ids.contains(&norm_slug) {
            continue;
        }

        let selected_ver = if let Some(ref vid) = resolved_ver_id {
            details
                .versions
                .iter()
                .find(|v| &v.version_id == vid)
                .or_else(|| details.versions.first())
                .cloned()
        } else {
            details.versions.first().cloned()
        };

        if let Some(ver) = selected_ver {
            if !existing_filenames.contains(&ver.file_name.to_lowercase()) {
                existing_filenames.insert(ver.file_name.to_lowercase());
                existing_norm_ids.insert(norm_slug);
                if content_type == ContentType::Mod {
                    for dep in &ver.dependencies {
                        if dep.dependency_type == "required"
                            && !visited_projects.contains(&dep.project_id)
                        {
                            pending_queue.push((dep.project_id.clone(), None, depth + 1));
                        }
                    }
                }
                resolved_entries.push(ver);
            }
        }
    }

    if resolved_entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut download_tasks = Vec::new();
    let mut link_pairs: Vec<(PathBuf, PathBuf, String)> = Vec::new();

    for ver in &resolved_entries {
        let final_dest = target_dir.join(&ver.file_name);
        if content_type == ContentType::Mod {
            let pool_name = if let Some(ref h) = ver.sha1 {
                format!("{}_{}", &h[0..8.min(h.len())], ver.file_name)
            } else {
                ver.file_name.clone()
            };
            let pool_path = pool_dir.join(pool_name);
            download_tasks.push(DownloadTask {
                url: ver.download_url.clone(),
                destination: pool_path.clone(),
                expected_sha1: ver.sha1.clone(),
                expected_size: if ver.size_bytes > 0 {
                    Some(ver.size_bytes)
                } else {
                    None
                },
                label: ver.file_name.clone(),
            });
            link_pairs.push((pool_path, final_dest, ver.file_name.clone()));
        } else {
            download_tasks.push(DownloadTask {
                url: ver.download_url.clone(),
                destination: final_dest.clone(),
                expected_sha1: ver.sha1.clone(),
                expected_size: None,
                label: ver.file_name.clone(),
            });
            link_pairs.push((final_dest.clone(), final_dest, ver.file_name.clone()));
        }
    }

    let inst_id = instance.id.clone();
    let app_clone = app_handle.clone();
    downloader
        .verify_and_heal_batch(download_tasks, move |done, total, label| {
            let _ = app_clone.emit(
                "download-progress",
                DownloadProgressEvent {
                    instance_id: inst_id.clone(),
                    completed_files: done,
                    total_files: total,
                    current_label: label,
                    phase: "store".to_string(),
                    stage_text: "Mağaza içeriği ve bağımlılıklar indiriliyor...".to_string(),
                },
            );
        })
        .await?;

    let mut installed_names = Vec::new();
    for (src, dst, name) in link_pairs {
        if src != dst {
            link_from_shared_pool_or_copy(&src, &dst)?;
        }
        installed_names.push(name);
    }

    let _ = app_handle.emit("download-finished", &instance.id);
    Ok(installed_names)
}

/// Installs a full Modpack (`.mrpack` or CurseForge `.zip`) from the in-app Store directly as a new Custom Instance.
pub async fn install_modpack_from_store(
    manager: &InstanceManager,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    project_id: &str,
    provider: &str,
    version_id: Option<&str>,
    cf_api_key: Option<&str>,
) -> Result<InstanceConfig, String> {
    downloader.reset_cancel();
    let details = get_store_project_details(
        downloader.client(),
        project_id,
        provider,
        ContentType::Modpack,
        None,
        None,
        cf_api_key,
    )
    .await?;

    let ver = if let Some(vid) = version_id {
        details
            .versions
            .iter()
            .find(|v| v.version_id == vid)
            .or_else(|| details.versions.first())
    } else {
        details.versions.first()
    }
    .ok_or("Bu mod paketi için indirilebilir sürüm bulunamadı.")?;

    let temp_dir = manager.app_data_dir().join("cache").join("modpack_temp");
    let _ = fs::create_dir_all(&temp_dir);
    let archive_path = temp_dir.join(&ver.file_name);

    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: "store-modpack".to_string(),
            completed_files: 0,
            total_files: 1,
            current_label: ver.file_name.clone(),
            phase: "modpack-archive".to_string(),
            stage_text: "Aşama 1/2: Mod paketi arşivi indiriliyor...".to_string(),
        },
    );

    downloader
        .download_single(&DownloadTask {
            url: ver.download_url.clone(),
            destination: archive_path.clone(),
            expected_sha1: ver.sha1.clone(),
            expected_size: None,
            label: ver.file_name.clone(),
        })
        .await?;

    let created = import_modpack_archive(
        manager,
        downloader,
        app_handle,
        &archive_path,
        Some(&details.title),
        cf_api_key,
    )
    .await?;

    let _ = fs::remove_file(&archive_path);
    let _ = app_handle.emit("download-finished", &created.id);
    Ok(created)
}

/// Parses and installs a `.mrpack` (`modrinth.index.json`) or CurseForge `.zip` (`manifest.json`) archive.
pub async fn import_modpack_archive(
    manager: &InstanceManager,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    archive_path: &Path,
    override_pack_name: Option<&str>,
    cf_api_key: Option<&str>,
) -> Result<InstanceConfig, String> {
    let raw_bytes =
        fs::read(archive_path).map_err(|e| format!("Mod paketi dosyası okunamadı: {}", e))?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&raw_bytes))
        .map_err(|e| format!("Geçersiz arşiv formatı: {}", e))?;

    // Check if it is a Modrinth `.mrpack` (`modrinth.index.json`)
    let mut mr_index_str = String::new();
    let is_mrpack = if let Ok(mut entry) = zip.by_name("modrinth.index.json") {
        let _ = entry.read_to_string(&mut mr_index_str);
        !mr_index_str.is_empty()
    } else {
        false
    };

    if is_mrpack {
        let idx_json: Value =
            serde_json::from_str(&mr_index_str).map_err(|e| e.to_string())?;
        let pack_name = override_pack_name
            .or_else(|| idx_json.get("name").and_then(|v| v.as_str()))
            .unwrap_or("Modrinth Paketi");

        let deps = idx_json.get("dependencies").ok_or("dependencies alanı eksik")?;
        let mc_version = deps
            .get("minecraft")
            .and_then(|v| v.as_str())
            .unwrap_or("1.20.1")
            .to_string();

        let (modloader, modloader_version) = if let Some(v) = deps.get("fabric-loader").and_then(|x| x.as_str()) {
            (ModloaderType::Fabric, Some(v.to_string()))
        } else if let Some(v) = deps.get("quilt-loader").and_then(|x| x.as_str()) {
            (ModloaderType::Quilt, Some(v.to_string()))
        } else if let Some(v) = deps.get("neoforge").and_then(|x| x.as_str()) {
            (ModloaderType::NeoForge, Some(v.to_string()))
        } else if let Some(v) = deps.get("forge").and_then(|x| x.as_str()) {
            (ModloaderType::Forge, Some(v.to_string()))
        } else {
            (ModloaderType::Vanilla, None)
        };

        let instance = manager.create_custom_instance(
            pack_name,
            &mc_version,
            modloader,
            modloader_version,
            true,
            Some(1024),
            Some(4096),
        )?;

        // Extract `overrides/` and `client-overrides/` safely
        extract_zip_prefix_safely(&mut zip, "overrides/", &instance.game_dir)?;
        extract_zip_prefix_safely(&mut zip, "client-overrides/", &instance.game_dir)?;
        drop(zip);

        let mut tasks = Vec::new();
        if let Some(files) = idx_json.get("files").and_then(|v| v.as_array()) {
            for f in files {
                // Skip server-only mods (`env.client == "unsupported"`)
                if f.pointer("/env/client").and_then(|c| c.as_str()) == Some("unsupported") {
                    continue;
                }
                let rel_path = match f.get("path").and_then(|p| p.as_str()) {
                    Some(p) => p,
                    None => continue,
                };
                if let Some(dest) = sanitize_relative_path(&instance.game_dir, rel_path) {
                    if let Some(url) = f
                        .get("downloads")
                        .and_then(|d| d.as_array())
                        .and_then(|a| a.first())
                        .and_then(|u| u.as_str())
                    {
                        let sha1 = f
                            .pointer("/hashes/sha1")
                            .and_then(|h| h.as_str())
                            .map(|s| s.to_string());
                        tasks.push(DownloadTask {
                            url: url.to_string(),
                            destination: dest,
                            expected_sha1: sha1,
                            expected_size: f.get("fileSize").and_then(|s| s.as_u64()),
                            label: rel_path.to_string(),
                        });
                    }
                }
            }
        }

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
                        phase: "mrpack-files".to_string(),
                        stage_text: "Aşama 2/2: Mod paketi içeriği indiriliyor...".to_string(),
                    },
                );
            })
            .await?;

        return Ok(instance);
    }

    // Check if it is a CurseForge `.zip` (`manifest.json`)
    let mut cf_manifest_str = String::new();
    if let Ok(mut entry) = zip.by_name("manifest.json") {
        let _ = entry.read_to_string(&mut cf_manifest_str);
    }
    if cf_manifest_str.is_empty() {
        return Err("Arşiv içinde modrinth.index.json veya manifest.json bulunamadı.".to_string());
    }

    let cf_json: Value = serde_json::from_str(&cf_manifest_str).map_err(|e| e.to_string())?;
    let pack_name = override_pack_name
        .or_else(|| cf_json.get("name").and_then(|v| v.as_str()))
        .unwrap_or("CurseForge Paketi");
    let mc_version = cf_json
        .pointer("/minecraft/version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.20.1")
        .to_string();

    let mut modloader = ModloaderType::Vanilla;
    let mut modloader_version = None;
    if let Some(loaders) = cf_json.pointer("/minecraft/modLoaders").and_then(|v| v.as_array()) {
        if let Some(first) = loaders.first() {
            if let Some(id_str) = first.get("id").and_then(|v| v.as_str()) {
                modloader = ModloaderType::from_str_loose(id_str);
                modloader_version = id_str.split('-').nth(1).map(|s| s.to_string());
            }
        }
    }

    let instance = manager.create_custom_instance(
        pack_name,
        &mc_version,
        modloader,
        modloader_version,
        true,
        Some(1024),
        Some(4096),
    )?;

    let overrides_folder = cf_json
        .get("overrides")
        .and_then(|v| v.as_str())
        .unwrap_or("overrides");
    let prefix = format!("{}/", overrides_folder.trim_end_matches('/'));
    extract_zip_prefix_safely(&mut zip, &prefix, &instance.game_dir)?;
    drop(zip);

    let key = cf_api_key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or(DEFAULT_CF_API_KEY);
    let mut tasks = Vec::new();
    if let Some(files) = cf_json.get("files").and_then(|v| v.as_array()) {
        for f in files {
            let pid = f.get("projectID").and_then(|v| v.as_u64()).unwrap_or(0);
            let fid = f.get("fileID").and_then(|v| v.as_u64()).unwrap_or(0);
            if pid == 0 || fid == 0 {
                continue;
            }
            if let Ok(resp) = downloader
                .client()
                .get(format!("{}/mods/{}/files/{}", CURSEFORGE_API_BASE, pid, fid))
                .header("x-api-key", key)
                .send()
                .await
            {
                if let Ok(fjson) = resp.json::<Value>().await {
                    if let Some(data) = fjson.get("data") {
                        let fname = data
                            .get("fileName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("mod.jar")
                            .to_string();
                        let url = data
                            .get("downloadUrl")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| resolve_curseforge_cdn_url(fid, &fname));
                        let sub = if fname.ends_with(".zip") {
                            "resourcepacks"
                        } else {
                            "mods"
                        };
                        tasks.push(DownloadTask {
                            url,
                            destination: instance.game_dir.join(sub).join(&fname),
                            expected_sha1: None,
                            expected_size: None,
                            label: fname,
                        });
                    }
                }
            }
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
                        phase: "cf-files".to_string(),
                        stage_text: "Aşama 2/2: Mod paketi modları indiriliyor...".to_string(),
                    },
                );
            })
            .await?;
    }

    Ok(instance)
}

pub fn sanitize_relative_path(base_dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut clean = base_dir.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None, // Reject `..`, root, or prefix components (Zip-Slip protection)
        }
    }
    Some(clean)
}

pub fn extract_zip_prefix_safely<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    prefix: &str,
    target_root: &Path,
) -> Result<(), String> {
    for i in 0..zip.len() {
        let mut entry = match zip.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.name().replace('\\', "/");
        if let Some(stripped) = name.strip_prefix(prefix) {
            if stripped.is_empty() {
                continue;
            }
            if let Some(dest_path) = sanitize_relative_path(target_root, stripped) {
                if entry.is_dir() || name.ends_with('/') {
                    let _ = fs::create_dir_all(&dest_path);
                } else {
                    if let Some(parent) = dest_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Ok(mut out) = File::create(&dest_path) {
                        let _ = std::io::copy(&mut entry, &mut out);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Checks SHA-1 hashes of installed `.jar` files in `<game_dir>/mods/` against Modrinth's update endpoint.
pub async fn check_instance_mod_updates(
    client: &Client,
    instance: &InstanceConfig,
) -> Result<Vec<ModUpdateInfo>, String> {
    let mods_dir = instance.game_dir.join("mods");
    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let mut hash_to_filename: HashMap<String, String> = HashMap::new();
    let mut hashes: Vec<String> = Vec::new();

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
            if let Ok(mut f) = File::open(&path) {
                let mut hasher = Sha1::new();
                if std::io::copy(&mut f, &mut hasher).is_ok() {
                    let sha = hex::encode(hasher.finalize());
                    hash_to_filename.insert(sha.clone(), fname);
                    hashes.push(sha);
                }
            }
        }
    }

    if hashes.is_empty() {
        return Ok(Vec::new());
    }

    let loader_str = instance.modloader.as_str();
    let payload = json!({
        "hashes": hashes,
        "algorithm": "sha1",
        "loaders": [loader_str],
        "game_versions": [instance.mc_version]
    });

    let resp = client
        .post(format!("{}/version_files/update", MODRINTH_API_BASE))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Mod güncelleme sorgusu başarısız: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth Update HTTP {}", resp.status()));
    }

    let map: HashMap<String, Value> = resp.json().await.map_err(|e| e.to_string())?;
    let mut updates = Vec::new();

    for (old_sha, ver_val) in map {
        if let Some(current_fname) = hash_to_filename.get(&old_sha) {
            if let Some(parsed) = parse_modrinth_version_value(&ver_val) {
                let is_newer = parsed
                    .sha1
                    .as_ref()
                    .map(|new_sha| !new_sha.eq_ignore_ascii_case(&old_sha))
                    .unwrap_or(false);
                if is_newer {
                    let project_id = ver_val
                        .get("project_id")
                        .and_then(|p| p.as_str())
                        .unwrap_or_default()
                        .to_string();
                    updates.push(ModUpdateInfo {
                        file_name: current_fname.clone(),
                        current_sha1: old_sha,
                        project_id,
                        latest_version_id: parsed.version_id,
                        latest_version_number: parsed.version_number,
                        latest_file_name: parsed.file_name,
                        download_url: parsed.download_url,
                        latest_sha1: parsed.sha1,
                    });
                }
            }
        }
    }

    Ok(updates)
}

/// Applies all discovered mod updates in parallel and removes the old `.jar` files.
pub async fn apply_instance_mod_updates(
    instance: &InstanceConfig,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    updates: Vec<ModUpdateInfo>,
) -> Result<usize, String> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mods_dir = instance.game_dir.join("mods");
    let mut tasks = Vec::new();
    for u in &updates {
        tasks.push(DownloadTask {
            url: u.download_url.clone(),
            destination: mods_dir.join(&u.latest_file_name),
            expected_sha1: u.latest_sha1.clone(),
            expected_size: None,
            label: u.latest_file_name.clone(),
        });
    }

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
                    phase: "mod-updates".to_string(),
                    stage_text: "Mod güncellemeleri indiriliyor...".to_string(),
                },
            );
        })
        .await?;

    let mut count = 0;
    for u in &updates {
        if u.file_name != u.latest_file_name {
            let old_path = mods_dir.join(&u.file_name);
            let _ = fs::remove_file(old_path);
        }
        count += 1;
    }

    let _ = app_handle.emit("download-finished", &instance.id);
    Ok(count)
}

/// Feature #5: 1-Click "FPS Boost Suite" Injector (`Sodium`/`Embeddium`, `Lithium`/`Radium`, `FerriteCore`, `EntityCulling`, `ImmediatelyFast`).
pub async fn install_fps_boost_suite(
    instance: &InstanceConfig,
    app_data_dir: &Path,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
) -> Result<Vec<String>, String> {
    if instance.modloader == ModloaderType::Vanilla {
        return Err(
            "FPS Optimizasyon modları için paketin Fabric, NeoForge, Forge veya Quilt olması gerekir."
                .to_string(),
        );
    }

    let candidate_slugs = match instance.modloader {
        ModloaderType::Fabric | ModloaderType::Quilt => vec![
            "sodium",
            "lithium",
            "ferrite-core",
            "entityculling",
            "immediatelyfast",
        ],
        ModloaderType::Forge | ModloaderType::NeoForge => vec![
            "embeddium",
            "ferrite-core",
            "entityculling",
            "immediatelyfast",
        ],
        ModloaderType::Vanilla => vec![],
    };

    let mut all_installed = Vec::new();
    for slug in candidate_slugs {
        if let Ok(mut installed) = install_store_content_with_dependencies(
            instance,
            app_data_dir,
            downloader,
            app_handle,
            slug,
            "modrinth",
            ContentType::Mod,
            None,
            None,
        )
        .await
        {
            all_installed.append(&mut installed);
        }
    }

    Ok(all_installed)
}

/// Feature #2: 1-Click "Play With Friends" P2P LAN Tunnel Injector (`e4mc`).
pub async fn install_lan_tunnel_mod(
    instance: &InstanceConfig,
    app_data_dir: &Path,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
) -> Result<Vec<String>, String> {
    if instance.modloader == ModloaderType::Vanilla {
        return Err(
            "Arkadaşla Oyna (e4mc) tünel modu için pakette bir Mod Yükleyici (Fabric/Forge/NeoForge/Quilt) seçili olmalıdır."
                .to_string(),
        );
    }

    install_store_content_with_dependencies(
        instance,
        app_data_dir,
        downloader,
        app_handle,
        "e4mc",
        "modrinth",
        ContentType::Mod,
        None,
        None,
    )
    .await
}

/// Feature #4: Offline Skin & Cape Manager (`CustomSkinLoader` local skin injection).
pub async fn apply_offline_skin_to_instance(
    instance: &InstanceConfig,
    app_data_dir: &Path,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    username: &str,
    skin_source_path: &Path,
) -> Result<String, String> {
    if !skin_source_path.exists() {
        return Err("Seçilen skin (.png) dosyası bulunamadı.".to_string());
    }

    let csl_dir = instance.game_dir.join("CustomSkinLoader");
    let local_skins_dir = csl_dir.join("LocalSkin").join("skins");
    let _ = fs::create_dir_all(&local_skins_dir);

    let dest_png = local_skins_dir.join(format!("{}.png", username));
    fs::copy(skin_source_path, &dest_png)
        .map_err(|e| format!("Skin dosyası kopyalanamadı: {}", e))?;

    let csl_config = json!({
        "enable": true,
        "loadlist": [
            {
                "name": "LocalSkin",
                "type": "Legacy",
                "checkPNG": false,
                "skin": "LocalSkin/skins/{USERNAME}.png",
                "model": "auto",
                "cape": "LocalSkin/capes/{USERNAME}.png",
                "elytra": "LocalSkin/elytras/{USERNAME}.png"
            },
            {
                "name": "Mojang",
                "type": "MojangAPI"
            }
        ]
    });

    let _ = fs::write(
        csl_dir.join("CustomSkinLoader.json"),
        serde_json::to_string_pretty(&csl_config).unwrap_or_default(),
    );

    if instance.modloader != ModloaderType::Vanilla {
        let _ = install_store_content_with_dependencies(
            instance,
            app_data_dir,
            downloader,
            app_handle,
            "customskinloader",
            "modrinth",
            ContentType::Mod,
            None,
            None,
        )
        .await;
    }

    Ok(dest_png.to_string_lossy().to_string())
}

pub fn normalize_mod_identifier(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

/// Maps internal Mod IDs (from `fabric.mod.json` / `mods.toml`) to known Modrinth project slugs.
pub fn resolve_mod_id_to_slug(mod_id: &str) -> Option<&'static str> {
    let norm = normalize_mod_identifier(mod_id);
    match norm.as_str() {
        // Built-in runtime / loader pseudo-dependencies (should not be downloaded as mods)
        "minecraft" | "java" | "fabricloader" | "quiltloader" | "forge" | "neoforge" | "fml"
        | "mixinextras" | "fabric" => {
            if norm == "fabric" {
                Some("fabric-api")
            } else {
                None
            }
        }
        "fabricapi" | "fabricrenderingv1" | "fabriclifecycleeventsv1" | "fabricnetworkingapiv1"
        | "fabricresourceconditionsapiv1" | "fabricitemgroupapiv1" | "fabriccommandapiv2" => {
            Some("fabric-api")
        }
        "qfapi" | "quiltedfabricapi" | "qsl" => Some("qsl"),
        "clothconfig" | "clothconfig2" | "clothconfigv11" => Some("cloth-config"),
        "architectury" | "architecturyapi" => Some("architectury-api"),
        "forgeconfigapiport" => Some("forge-config-api-port"),
        "kotlinforforge" => Some("kotlin-for-forge"),
        "fabriclanguagekotlin" => Some("fabric-language-kotlin"),
        "geckolib" | "geckolib3" | "geckolib4" => Some("geckolib"),
        "curios" | "curiosapi" => Some("curios"),
        "trinkets" => Some("trinkets"),
        "yacl" | "yetanotherconfiglib" | "yetanotherconfiglibv3" => Some("yacl"),
        "puzzleslib" => Some("puzzles-lib"),
        "balm" | "balmforge" | "balmfabric" | "balmneoforge" => Some("balm"),
        "moonlight" | "moonlightlib" | "selene" => Some("moonlight"),
        "citadel" => Some("citadel"),
        "bookshelf" => Some("bookshelf"),
        "placebo" => Some("placebo"),
        "resourcefullib" => Some("resourceful-lib"),
        "resourcefulconfig" => Some("resourceful-config"),
        "athena" => Some("athena"),
        "cristellib" => Some("cristel-lib"),
        "terrablender" => Some("terrablender"),
        "indium" => Some("indium"),
        "modmenu" => Some("modmenu"),
        "collective" => Some("collective"),
        "konkrete" => Some("konkrete"),
        "melody" => Some("melody"),
        "fancymenu" => Some("fancymenu"),
        "supermartijn642corelib" => Some("supermartijn642s-core-lib"),
        "supermartijn642configlib" => Some("supermartijn642s-config-lib"),
        "smartbrainlib" => Some("smartbrainlib"),
        "azurelib" => Some("azurelib"),
        "playeranimator" => Some("playeranimator"),
        "bendylib" => Some("bendy-lib"),
        "creativecore" => Some("creativecore"),
        "cupboard" => Some("cupboard"),
        "framework" => Some("framework"),
        "iceberg" => Some("iceberg"),
        "insanelib" => Some("insanelib"),
        "kiwi" | "kiwilib" => Some("kiwi"),
        "lodestone" | "lodestonelib" => Some("lodestone"),
        "midnightlib" => Some("midnightlib"),
        "necronomicon" => Some("necronomicon"),
        "owo" | "owolib" => Some("owo-lib"),
        "patchouli" => Some("patchouli"),
        "polylib" => Some("polylib"),
        "prickle" => Some("prickle"),
        "searchables" => Some("searchables"),
        "spectrelib" => Some("spectrelib"),
        "structuregel" | "structuregelapi" => Some("structure-gel-api"),
        "titanium" => Some("titanium"),
        "valhelsiacore" => Some("valhelsia-core"),
        "botarium" => Some("botarium"),
        "corgilib" => Some("corgilib"),
        "yungsapi" => Some("yungs-api"),
        "attributefix" => Some("attributefix"),
        "flywheel" => Some("flywheel"),
        "registrate" => Some("registrate"),
        _ => None,
    }
}

fn is_builtin_mod_id(mod_id: &str) -> bool {
    let norm = normalize_mod_identifier(mod_id);
    matches!(
        norm.as_str(),
        "" | "minecraft"
            | "java"
            | "fabricloader"
            | "quiltloader"
            | "forge"
            | "neoforge"
            | "fml"
            | "mixinextras"
            | "optifine"
    ) || norm.starts_with("fabricapi") && false // handled by alias
}

/// Scans all enabled `.jar` files in `<game_dir>/mods/`, detects missing required dependencies via both:
/// 1. Modrinth Bulk SHA-1 Hash Lookup (`POST /v2/version_files`) for exact online `required` dependency graphs.
/// 2. Local in-JAR manifest inspection (`fabric.mod.json` & `mods.toml` / `neoforge.mods.toml`) mapped via `resolve_mod_id_to_slug` or Modrinth search.
/// Automatically downloads and links all missing dependencies into `<game_dir>/mods/`.
pub async fn heal_missing_mod_dependencies(
    instance: &InstanceConfig,
    app_data_dir: &Path,
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    cf_api_key: Option<&str>,
) -> Result<Vec<String>, String> {
    if instance.modloader == ModloaderType::Vanilla {
        return Ok(Vec::new());
    }

    let mods_dir = instance.game_dir.join("mods");
    if !mods_dir.exists() {
        return Ok(Vec::new());
    }

    let existing_mods = scan_instance_mods(&instance.game_dir).unwrap_or_default();
    if existing_mods.is_empty() {
        return Ok(Vec::new());
    }

    let mut installed_norm_ids: HashSet<String> = HashSet::new();
    for m in existing_mods.iter().filter(|m| m.enabled) {
        let norm = normalize_mod_identifier(&m.mod_id);
        installed_norm_ids.insert(norm.clone());
        if norm == "fabricapi" || norm == "fabric" {
            installed_norm_ids.insert("fabricapi".to_string());
            installed_norm_ids.insert("fabric".to_string());
        }
        // Also normalize filename stem as fallback for jars without standard mod_id
        let stem = m
            .file_name
            .trim_end_matches(".disabled")
            .trim_end_matches(".jar")
            .split(|c: char| c == '-' || c == '_' || c == '+')
            .next()
            .unwrap_or("");
        if !stem.is_empty() {
            installed_norm_ids.insert(normalize_mod_identifier(stem));
        }
    }

    // 1. Collect SHA-1 hashes of all enabled .jar files
    let mut hashes: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                    if fname.ends_with(".jar") {
                        if let Ok(mut f) = File::open(&path) {
                            let mut hasher = Sha1::new();
                            if std::io::copy(&mut f, &mut hasher).is_ok() {
                                hashes.push(hex::encode(hasher.finalize()));
                            }
                        }
                    }
                }
            }
        }
    }

    let client = downloader.client();
    let mut installed_project_ids: HashSet<String> = HashSet::new();
    let mut missing_projects_to_install: Vec<String> = Vec::new();

    // Query Modrinth bulk `/v2/version_files` to get exact project_id and required dependencies
    if !hashes.is_empty() {
        let payload = json!({
            "hashes": hashes,
            "algorithm": "sha1"
        });
        if let Ok(resp) = client
            .post(format!("{}/version_files", MODRINTH_API_BASE))
            .json(&payload)
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(map) = resp.json::<HashMap<String, Value>>().await {
                    // First pass: record all installed project IDs
                    for ver_val in map.values() {
                        if let Some(pid) = ver_val.get("project_id").and_then(|p| p.as_str()) {
                            installed_project_ids.insert(pid.to_string());
                        }
                    }
                    // Second pass: collect required dependencies that are not installed
                    for ver_val in map.values() {
                        if let Some(parsed) = parse_modrinth_version_value(ver_val) {
                            for dep in parsed.dependencies {
                                if dep.dependency_type == "required"
                                    && !installed_project_ids.contains(&dep.project_id)
                                    && !missing_projects_to_install.contains(&dep.project_id)
                                {
                                    missing_projects_to_install.push(dep.project_id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Inspect local JAR manifest dependencies (`fabric.mod.json` / `mods.toml`)
    let mut missing_mod_ids: HashSet<String> = HashSet::new();
    for m in existing_mods.iter().filter(|m| m.enabled) {
        for raw_dep in &m.dependencies {
            let clean_dep = raw_dep
                .split(|c: char| c.is_whitespace() || c == '>' || c == '<' || c == '=' || c == '~' || c == '^')
                .next()
                .unwrap_or("")
                .trim();
            if is_builtin_mod_id(clean_dep) {
                continue;
            }
            // Skip internal Fabric API sub-modules if fabric-api is already present or queued
            if clean_dep.starts_with("fabric-")
                && clean_dep != "fabric-language-kotlin"
                && (installed_norm_ids.contains("fabricapi") || installed_norm_ids.contains("fabric"))
            {
                continue;
            }
            let norm_dep = normalize_mod_identifier(clean_dep);
            if !norm_dep.is_empty() && !installed_norm_ids.contains(&norm_dep) {
                if let Some(slug) = resolve_mod_id_to_slug(clean_dep) {
                    let norm_slug = normalize_mod_identifier(slug);
                    if !installed_norm_ids.contains(&norm_slug)
                        && !missing_projects_to_install.contains(&slug.to_string())
                    {
                        missing_projects_to_install.push(slug.to_string());
                    }
                } else if !clean_dep.starts_with("fabric_") && !clean_dep.starts_with("fabric-") {
                    missing_mod_ids.insert(clean_dep.to_string());
                }
            }
        }
    }

    // For unknown mod IDs not in the static dictionary, search Modrinth for an exact slug/mod_id match
    for unknown_id in missing_mod_ids {
        if let Ok(results) = search_store_content(
            client,
            &unknown_id,
            ContentType::Mod,
            StoreProvider::Modrinth,
            "relevance",
            Some(&instance.mc_version),
            Some(instance.modloader.as_str()),
            None,
            0,
            cf_api_key,
        )
        .await
        {
            let norm_target = normalize_mod_identifier(&unknown_id);
            if let Some(matched) = results.into_iter().find(|item| {
                let norm_slug = normalize_mod_identifier(&item.slug);
                let norm_title = normalize_mod_identifier(&item.title);
                norm_slug == norm_target || norm_title == norm_target || norm_slug.contains(&norm_target)
            }) {
                if !installed_project_ids.contains(&matched.project_id)
                    && !missing_projects_to_install.contains(&matched.project_id)
                {
                    missing_projects_to_install.push(matched.project_id);
                }
            }
        }
    }

    if missing_projects_to_install.is_empty() {
        return Ok(Vec::new());
    }

    let mut newly_installed_files: Vec<String> = Vec::new();
    for proj_or_slug in missing_projects_to_install {
        // Resolve "ver:{id}" if needed
        let target_proj = if let Some(vid) = proj_or_slug.strip_prefix("ver:") {
            if let Ok(resp) = client
                .get(format!("{}/version/{}", MODRINTH_API_BASE, vid))
                .send()
                .await
            {
                if let Ok(vjson) = resp.json::<Value>().await {
                    vjson
                        .get("project_id")
                        .and_then(|p| p.as_str())
                        .unwrap_or(vid)
                        .to_string()
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            proj_or_slug
        };

        if let Ok(mut files) = install_store_content_with_dependencies(
            instance,
            app_data_dir,
            downloader,
            app_handle,
            &target_proj,
            "modrinth",
            ContentType::Mod,
            None,
            cf_api_key,
        )
        .await
        {
            newly_installed_files.append(&mut files);
        }
    }

    Ok(newly_installed_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curseforge_cdn_fallback_url_math() {
        // Standard 7-digit file ID
        let url1 = resolve_curseforge_cdn_url(4567890, "jei-1.20.1-15.2.0.27.jar");
        assert_eq!(
            url1,
            "https://edge.forgecdn.net/files/4567/890/jei-1.20.1-15.2.0.27.jar"
        );

        // Remainder with leading zeroes stripped (`005` -> `5`) and spaces encoded
        let url2 = resolve_curseforge_cdn_url(5123005, "My Mod + Pack [1.20].jar");
        assert_eq!(
            url2,
            "https://edge.forgecdn.net/files/5123/5/My%20Mod%20%2B%20Pack%20%5B1.20%5D.jar"
        );
    }

    #[test]
    fn test_mod_id_to_slug_resolution() {
        assert_eq!(resolve_mod_id_to_slug("fabric"), Some("fabric-api"));
        assert_eq!(resolve_mod_id_to_slug("fabric-api"), Some("fabric-api"));
        assert_eq!(resolve_mod_id_to_slug("cloth-config2"), Some("cloth-config"));
        assert_eq!(resolve_mod_id_to_slug("architectury"), Some("architectury-api"));
        assert_eq!(resolve_mod_id_to_slug("minecraft"), None);
    }
}
