use crate::downloader::{DownloadEngine, DownloadProgressEvent, DownloadTask};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::process::Command;
use tauri::{AppHandle, Emitter};

pub const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_title: String,
    pub changelog: String,
    pub published_at: String,
    pub download_url: Option<String>,
    pub asset_size_bytes: u64,
}

/// Cleans up `SkyLauncher.exe.old` left behind by a previous self-update swap on startup.
pub fn cleanup_old_executable() {
    if let Ok(exe_path) = env::current_exe() {
        let old_path = exe_path.with_extension("exe.old");
        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
    }
}

fn parse_semver_tuple(ver: &str) -> (u32, u32, u32) {
    let cleaned = ver.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty());
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    parse_semver_tuple(candidate) > parse_semver_tuple(current)
}

/// Checks GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`) and falls back to
/// raw `version.json` (`/repos/{owner}/{repo}/main/version.json`) for new SkyLauncher updates.
pub async fn check_github_update(
    client: &Client,
    github_repo: &str,
) -> Result<UpdateCheckResult, String> {
    let clean_repo = github_repo
        .trim()
        .trim_start_matches("https://github.com/")
        .trim_matches('/');

    if clean_repo.is_empty() || !clean_repo.contains('/') {
        return Err(
            "Geçerli bir GitHub deposu girin (Örn: kullaniciadi/SkyLauncher).".to_string(),
        );
    }

    // 1. Try GitHub Releases API (`/releases/latest`)
    let releases_url = format!("https://api.github.com/repos/{}/releases/latest", clean_repo);
    if let Ok(resp) = client
        .get(&releases_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<Value>().await {
                let tag_name = json
                    .get("tag_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(CURRENT_APP_VERSION)
                    .to_string();
                let release_title = json
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&tag_name)
                    .to_string();
                let changelog = json
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Yeni sürüm iyileştirmeleri ve hata düzeltmeleri.")
                    .to_string();
                let published_at = json
                    .get("published_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut download_url = None;
                let mut asset_size_bytes = 0u64;

                if let Some(assets) = json.get("assets").and_then(|v| v.as_array()) {
                    let exe_asset = assets
                        .iter()
                        .find(|a| {
                            a.get("name")
                                .and_then(|n| n.as_str())
                                .map(|n| n.to_lowercase().ends_with(".exe"))
                                .unwrap_or(false)
                        })
                        .or_else(|| assets.first());

                    if let Some(asset) = exe_asset {
                        download_url = asset
                            .get("browser_download_url")
                            .and_then(|u| u.as_str())
                            .map(|s| s.to_string());
                        asset_size_bytes =
                            asset.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                    }
                }

                if download_url.is_none() {
                    download_url = Some(format!(
                        "https://github.com/{}/raw/main/SkyLauncher.exe",
                        clean_repo
                    ));
                }

                let update_available = is_newer_version(CURRENT_APP_VERSION, &tag_name);
                return Ok(UpdateCheckResult {
                    current_version: CURRENT_APP_VERSION.to_string(),
                    latest_version: tag_name.trim_start_matches('v').to_string(),
                    update_available,
                    release_title,
                    changelog,
                    published_at,
                    download_url,
                    asset_size_bytes,
                });
            }
        }
    }

    // 2. Fallback: Check raw `version.json` on `main` branch
    let raw_version_url = format!(
        "https://raw.githubusercontent.com/{}/main/version.json",
        clean_repo
    );
    let resp = client
        .get(&raw_version_url)
        .send()
        .await
        .map_err(|e| format!("GitHub sunucusuna ulaşılamadı: {}", e))?;

    if !resp.status().is_success() {
        return Ok(UpdateCheckResult {
            current_version: CURRENT_APP_VERSION.to_string(),
            latest_version: CURRENT_APP_VERSION.to_string(),
            update_available: false,
            release_title: format!("SkyLauncher v{}", CURRENT_APP_VERSION),
            changelog: "Henüz GitHub deposunda yayınlanmış bir Release veya version.json bulunamadı (Sürümünüz güncel).".to_string(),
            published_at: String::new(),
            download_url: Some(format!(
                "https://github.com/{}/raw/main/SkyLauncher.exe",
                clean_repo
            )),
            asset_size_bytes: 0,
        });
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest_version = json
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or(CURRENT_APP_VERSION)
        .to_string();
    let changelog = json
        .get("notes")
        .or_else(|| json.get("changelog"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let download_url = json
        .get("downloadUrl")
        .and_then(|v| v.as_str())
        .filter(|s| s.starts_with("https://"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            format!(
                "https://github.com/{}/raw/main/SkyLauncher.exe",
                clean_repo
            )
        });

    Ok(UpdateCheckResult {
        current_version: CURRENT_APP_VERSION.to_string(),
        latest_version: latest_version.clone(),
        update_available: is_newer_version(CURRENT_APP_VERSION, &latest_version),
        release_title: format!("SkyLauncher v{}", latest_version),
        changelog,
        published_at: String::new(),
        download_url: Some(download_url),
        asset_size_bytes: 0,
    })
}

/// Downloads the new `SkyLauncher.exe` from GitHub, verifies its Windows PE header, swaps the running binary on disk, and restarts the launcher.
pub async fn download_and_apply_github_update(
    downloader: &DownloadEngine,
    app_handle: &AppHandle,
    download_url: &str,
) -> Result<String, String> {
    downloader.reset_cancel();
    let current_exe =
        env::current_exe().map_err(|e| format!("Mevcut uygulama yolu alınamadı: {}", e))?;
    let update_staging = current_exe.with_extension("exe.new");
    let old_backup = current_exe.with_extension("exe.old");

    let _ = app_handle.emit(
        "download-progress",
        DownloadProgressEvent {
            instance_id: "app-updater".to_string(),
            completed_files: 0,
            total_files: 1,
            current_label: "SkyLauncher.exe".to_string(),
            phase: "updater".to_string(),
            stage_text: "Yeni SkyLauncher güncellemesi GitHub üzerinden indiriliyor...".to_string(),
        },
    );

    downloader
        .download_single(&DownloadTask {
            url: download_url.to_string(),
            destination: update_staging.clone(),
            expected_sha1: None,
            expected_size: None,
            label: "SkyLauncher Güncellemesi".to_string(),
        })
        .await?;

    // Verify Windows PE ('MZ') signature before replacing running executable
    if let Err(e) = crate::security::verify_pe_executable(&update_staging) {
        let _ = fs::remove_file(&update_staging);
        let _ = app_handle.emit("download-finished", "app-updater");
        return Err(e);
    }

    let _ = app_handle.emit("download-finished", "app-updater");

    if old_backup.exists() {
        let _ = fs::remove_file(&old_backup);
    }

    // Windows allows renaming the currently running executable
    fs::rename(&current_exe, &old_backup)
        .map_err(|e| format!("Mevcut çalıştırılabilir dosya yedeklenemedi: {}", e))?;

    if let Err(e) = fs::rename(&update_staging, &current_exe) {
        // Rollback if moving new exe fails
        let _ = fs::rename(&old_backup, &current_exe);
        return Err(format!("Yeni güncelleme dosyası yerleştirilemedi: {}", e));
    }

    // Spawn updated executable and exit current process
    let _ = Command::new(&current_exe).spawn();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        assert!(is_newer_version("1.0.0", "v1.0.1"));
        assert!(is_newer_version("1.0.0", "1.2.0"));
        assert!(!is_newer_version("1.2.0", "v1.1.9"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }
}
