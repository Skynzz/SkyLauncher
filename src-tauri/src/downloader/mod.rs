pub mod content_store;
pub mod modloaders;
pub mod mojang;

use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

pub const MAX_CONCURRENT_DOWNLOADS: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub url: String,
    pub destination: PathBuf,
    pub expected_sha1: Option<String>,
    pub expected_size: Option<u64>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    pub instance_id: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub current_label: String,
    pub phase: String,
    pub stage_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityReport {
    pub total_checked: usize,
    pub valid_files: usize,
    pub repaired_files: usize,
    pub corrupted_or_missing: Vec<String>,
}

pub struct DownloadEngine {
    client: Client,
    semaphore: Arc<Semaphore>,
    cancel_flag: Arc<AtomicBool>,
}

impl DownloadEngine {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("SkyLauncher/1.0.0")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .pool_max_idle_per_host(32)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn reset_cancel(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    pub fn trigger_cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    pub fn verify_file_sync(path: &Path, expected_sha1: Option<&str>) -> bool {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if meta.len() == 0 {
            return false;
        }
        if let Some(expected) = expected_sha1 {
            if expected.trim().is_empty() {
                return true;
            }
            let mut file = match std::fs::File::open(path) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let mut hasher = Sha1::new();
            if std::io::copy(&mut file, &mut hasher).is_err() {
                return false;
            }
            let actual = hex::encode(hasher.finalize());
            return actual.eq_ignore_ascii_case(expected.trim());
        }
        true
    }

    pub async fn download_single(&self, task: &DownloadTask) -> Result<(), String> {
        if self.is_cancelled() {
            return Err("İndirme kullanıcı tarafından iptal edildi.".to_string());
        }

        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;

        if self.is_cancelled() {
            return Err("İndirme kullanıcı tarafından iptal edildi.".to_string());
        }

        if Self::verify_file_sync(&task.destination, task.expected_sha1.as_deref()) {
            return Ok(());
        }

        if let Some(parent) = task.destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Klasör oluşturulamadı {:?}: {}", parent, e))?;
        }

        let unique_id = &Uuid::new_v4().simple().to_string()[0..8];
        let file_name = task
            .destination
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download");
        let part_path = task
            .destination
            .with_file_name(format!("{}.{}.part", file_name, unique_id));

        let mut last_err = String::new();
        for attempt in 1..=4 {
            if self.is_cancelled() {
                let _ = fs::remove_file(&part_path).await;
                return Err("İndirme kullanıcı tarafından iptal edildi.".to_string());
            }

            if Self::verify_file_sync(&task.destination, task.expected_sha1.as_deref()) {
                let _ = fs::remove_file(&part_path).await;
                return Ok(());
            }

            match self
                .stream_to_part(&task.url, &part_path, task.expected_sha1.as_deref())
                .await
            {
                Ok(()) => {
                    if let Err(rename_err) = fs::rename(&part_path, &task.destination).await {
                        if Self::verify_file_sync(&task.destination, task.expected_sha1.as_deref()) {
                            let _ = fs::remove_file(&part_path).await;
                            return Ok(());
                        }
                        match fs::copy(&part_path, &task.destination).await {
                            Ok(_) => {
                                let _ = fs::remove_file(&part_path).await;
                                return Ok(());
                            }
                            Err(_) => {
                                last_err = format!(
                                    "Dosya taşınamadı ({:?}): {}",
                                    task.destination, rename_err
                                );
                            }
                        }
                    } else {
                        return Ok(());
                    }
                }
                Err(e) => {
                    if self.is_cancelled() {
                        let _ = fs::remove_file(&part_path).await;
                        return Err("İndirme kullanıcı tarafından iptal edildi.".to_string());
                    }
                    last_err = format!("Deneme {}/4 ({}): {}", attempt, task.label, e);
                }
            }

            let _ = fs::remove_file(&part_path).await;
            tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
        }

        Err(last_err)
    }

    async fn stream_to_part(
        &self,
        url: &str,
        part_path: &Path,
        expected_sha1: Option<&str>,
    ) -> Result<(), String> {
        crate::security::validate_trusted_download_url(url)?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Ağ hatası: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {} ({})", response.status(), url));
        }

        let mut file = fs::File::create(part_path)
            .await
            .map_err(|e| format!("Geçici dosya açılamadı {:?}: {}", part_path, e))?;

        let mut hasher = Sha1::new();
        let mut byte_stream = response.bytes_stream();
        let mut total_bytes: u64 = 0;

        while let Some(chunk) = byte_stream.next().await {
            if self.is_cancelled() {
                drop(file);
                let _ = fs::remove_file(part_path).await;
                return Err("İndirme iptal edildi.".to_string());
            }
            let bytes = chunk.map_err(|e| format!("Veri okuma hatası: {}", e))?;
            hasher.update(&bytes);
            total_bytes += bytes.len() as u64;
            file.write_all(&bytes)
                .await
                .map_err(|e| format!("Disk yazma hatası: {}", e))?;
        }

        file.flush().await.map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);

        if total_bytes == 0 {
            return Err("0 baytlık boş dosya indirildi.".to_string());
        }

        if let Some(expected) = expected_sha1 {
            if !expected.trim().is_empty() {
                let actual = hex::encode(hasher.finalize());
                if !actual.eq_ignore_ascii_case(expected.trim()) {
                    return Err(format!(
                        "SHA-1 doğrulaması başarısız (beklenen: {}, gelen: {})",
                        expected, actual
                    ));
                }
            }
        }

        Ok(())
    }

    pub async fn verify_and_heal_batch<F>(
        &self,
        tasks: Vec<DownloadTask>,
        mut on_progress: F,
    ) -> Result<IntegrityReport, String>
    where
        F: FnMut(usize, usize, String) + Send,
    {
        let mut seen_destinations = HashSet::new();
        let mut unique_tasks = Vec::with_capacity(tasks.len());
        for task in tasks {
            if seen_destinations.insert(task.destination.clone()) {
                unique_tasks.push(task);
            }
        }

        let total_checked = unique_tasks.len();
        let mut to_download = Vec::new();
        let mut corrupted_or_missing = Vec::new();

        for task in unique_tasks {
            if self.is_cancelled() {
                return Err("İndirme kullanıcı tarafından iptal edildi.".to_string());
            }
            if !Self::verify_file_sync(&task.destination, task.expected_sha1.as_deref()) {
                corrupted_or_missing.push(task.label.clone());
                to_download.push(task);
            }
        }

        let valid_files = total_checked.saturating_sub(to_download.len());
        let total_to_fetch = to_download.len();
        if total_to_fetch == 0 {
            return Ok(IntegrityReport {
                total_checked,
                valid_files,
                repaired_files: 0,
                corrupted_or_missing,
            });
        }

        let completed = Arc::new(AtomicUsize::new(0));
        let mut stream = stream::iter(to_download.into_iter().map(|task| {
            let completed_ref = completed.clone();
            async move {
                let label = task.label.clone();
                let res = self.download_single(&task).await;
                let done = completed_ref.fetch_add(1, Ordering::SeqCst) + 1;
                (done, label, res)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DOWNLOADS);

        let mut repaired_files = 0;
        let mut errors = Vec::new();

        while let Some((done, label, res)) = stream.next().await {
            on_progress(done, total_to_fetch, label);
            match res {
                Ok(()) => repaired_files += 1,
                Err(e) => errors.push(e),
            }
        }

        if self.is_cancelled() {
            return Err("İşlem iptal edildi.".to_string());
        }

        if !errors.is_empty() {
            return Err(format!(
                "{} dosya indirilemedi: {}",
                errors.len(),
                errors[0]
            ));
        }

        Ok(IntegrityReport {
            total_checked,
            valid_files,
            repaired_files,
            corrupted_or_missing,
        })
    }
}
