pub mod curseforge_watcher;
pub mod jar_parser;
pub mod pack_share;

use crate::downloader::modloaders::ModloaderType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceSource {
    Custom,
    CurseForge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSettings {
    pub default_min_ram_mb: u32,
    pub default_max_ram_mb: u32,
    pub default_java_path: Option<String>,
    pub default_jvm_args: Vec<String>,
    pub curseforge_instances_dir: Option<String>,
    pub hide_launcher_on_launch: bool,
    pub language: String,
    pub active_theme_id: String,
    #[serde(default)]
    pub curseforge_api_key: Option<String>,
    #[serde(default)]
    pub auto_backup_worlds: bool,
    #[serde(default = "default_github_repo")]
    pub github_update_repo: String,
}

pub const OFFICIAL_GITHUB_REPO: &str = "Skynzz/SkyLauncher";

fn default_github_repo() -> String {
    OFFICIAL_GITHUB_REPO.to_string()
}

impl Default for GlobalSettings {
    fn default() -> Self {
        // Detect default CurseForge path if it exists on the user's machine
        let default_cf_dir = dirs::home_dir()
            .map(|h| h.join("curseforge").join("minecraft").join("Instances"))
            .filter(|p| p.exists())
            .and_then(|p| p.to_str().map(|s| s.to_string()));

        Self {
            default_min_ram_mb: 1024,
            default_max_ram_mb: 4096,
            default_java_path: None,
            default_jvm_args: Vec::new(),
            curseforge_instances_dir: default_cf_dir,
            hide_launcher_on_launch: false,
            language: "tr".to_string(),
            active_theme_id: "sky-slate".to_string(),
            curseforge_api_key: None,
            auto_backup_worlds: true,
            github_update_repo: default_github_repo(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub modloader: ModloaderType,
    pub modloader_version: Option<String>,
    pub source: InstanceSource,
    pub game_dir: PathBuf,
    pub icon_color: String,
    #[serde(default)]
    pub icon_data_url: Option<String>,
    pub override_memory: bool,
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub override_java: bool,
    pub java_path: Option<String>,
    pub custom_jvm_args: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_played: Option<DateTime<Utc>>,
    #[serde(default)]
    pub playtime_minutes: u64,
}

pub struct InstanceManager {
    app_data_dir: PathBuf,
    instances_dir: PathBuf,
    settings_path: PathBuf,
    registry_path: PathBuf,
}

impl InstanceManager {
    pub fn new(app_data_dir: &Path) -> Self {
        let instances_dir = app_data_dir.join("instances");
        let _ = fs::create_dir_all(&instances_dir);
        let settings_path = app_data_dir.join("global_settings.json");
        let registry_path = app_data_dir.join("instances_registry.json");

        let mgr = Self {
            app_data_dir: app_data_dir.to_path_buf(),
            instances_dir,
            settings_path,
            registry_path,
        };
        mgr.ensure_defaults();
        mgr
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    fn ensure_defaults(&self) {
        if !self.settings_path.exists() {
            let _ = self.save_global_settings(&GlobalSettings::default());
        }
        if !self.registry_path.exists() {
            let _ = self.save_instances(&[]);
        }
    }

    pub fn load_global_settings(&self) -> Result<GlobalSettings, String> {
        if !self.settings_path.exists() {
            return Ok(GlobalSettings::default());
        }
        let raw = fs::read_to_string(&self.settings_path).map_err(|e| e.to_string())?;
        let mut settings: GlobalSettings =
            serde_json::from_str(&raw).unwrap_or_else(|_| GlobalSettings::default());

        // Always lock the update repository to the official Skynzz/SkyLauncher repo
        settings.github_update_repo = OFFICIAL_GITHUB_REPO.to_string();

        // If CurseForge is not installed or the configured folder doesn't exist, gracefully clear it
        if let Some(ref cf_dir) = settings.curseforge_instances_dir {
            if cf_dir.trim().is_empty() || !Path::new(cf_dir).exists() {
                settings.curseforge_instances_dir = None;
            }
        }

        Ok(settings)
    }

    pub fn save_global_settings(&self, settings: &GlobalSettings) -> Result<(), String> {
        let mut locked = settings.clone();
        locked.github_update_repo = OFFICIAL_GITHUB_REPO.to_string();
        if let Some(ref cf_dir) = locked.curseforge_instances_dir {
            if cf_dir.trim().is_empty() || !Path::new(cf_dir).exists() {
                locked.curseforge_instances_dir = None;
            }
        }
        let raw = serde_json::to_string_pretty(&locked).map_err(|e| e.to_string())?;
        fs::write(&self.settings_path, raw).map_err(|e| e.to_string())
    }

    pub fn load_instances(&self) -> Result<Vec<InstanceConfig>, String> {
        if !self.registry_path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&self.registry_path).map_err(|e| e.to_string())?;
        let list: Vec<InstanceConfig> = serde_json::from_str(&raw).unwrap_or_default();

        // Keep all Custom instances (ensuring their directory exists) and only keep CurseForge instances if their folder exists on disk
        let filtered: Vec<InstanceConfig> = list
            .into_iter()
            .filter(|inst| match inst.source {
                InstanceSource::Custom => {
                    let _ = fs::create_dir_all(&inst.game_dir);
                    true
                }
                InstanceSource::CurseForge => inst.game_dir.exists(),
            })
            .collect();

        Ok(filtered)
    }

    pub fn save_instances(&self, instances: &[InstanceConfig]) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(instances).map_err(|e| e.to_string())?;
        fs::write(&self.registry_path, raw).map_err(|e| e.to_string())
    }

    pub fn get_instance(&self, instance_id: &str) -> Result<InstanceConfig, String> {
        let list = self.load_instances()?;
        list.into_iter()
            .find(|i| i.id == instance_id)
            .ok_or_else(|| format!("Instance '{}' not found.", instance_id))
    }

    /// Creates an isolated custom instance under `<appdata>/instances/<instance_id>/`
    /// with standard subdirectories (`mods/`, `resourcepacks/`, `shaderpacks/`, `config/`, `logs/`, `crash-reports/`).
    pub fn create_custom_instance(
        &self,
        name: &str,
        mc_version: &str,
        modloader: ModloaderType,
        modloader_version: Option<String>,
        override_memory: bool,
        min_ram_mb: Option<u32>,
        max_ram_mb: Option<u32>,
    ) -> Result<InstanceConfig, String> {
        let clean_name = name.trim();
        if clean_name.is_empty() {
            return Err("Instance name cannot be empty.".to_string());
        }

        let slug: String = clean_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let short_id = &Uuid::new_v4().simple().to_string()[0..6];
        let instance_id = format!("{}-{}", slug.trim_matches('-'), short_id);

        let game_dir = self.instances_dir.join(&instance_id);
        for subdir in [
            "mods",
            "resourcepacks",
            "shaderpacks",
            "config",
            "logs",
            "crash-reports",
        ] {
            fs::create_dir_all(game_dir.join(subdir)).map_err(|e| e.to_string())?;
        }

        let global = self.load_global_settings().unwrap_or_default();
        let palette = ["#6366f1", "#10b981", "#f59e0b", "#ec4899", "#06b6d4", "#8b5cf6"];
        let mut instances = self.load_instances()?;
        let icon_color = palette[instances.len() % palette.len()].to_string();

        let instance = InstanceConfig {
            id: instance_id,
            name: clean_name.to_string(),
            mc_version: mc_version.to_string(),
            modloader,
            modloader_version: modloader_version.filter(|s| !s.trim().is_empty()),
            source: InstanceSource::Custom,
            game_dir: game_dir.clone(),
            icon_color,
            icon_data_url: None,
            override_memory,
            min_ram_mb: min_ram_mb.unwrap_or(global.default_min_ram_mb),
            max_ram_mb: max_ram_mb.unwrap_or(global.default_max_ram_mb),
            override_java: false,
            java_path: None,
            custom_jvm_args: Vec::new(),
            created_at: Utc::now(),
            last_played: None,
            playtime_minutes: 0,
        };

        // Also save local instance.json inside the instance folder
        let instance_json_path = game_dir.join("instance.json");
        let _ = fs::write(
            instance_json_path,
            serde_json::to_string_pretty(&instance).unwrap_or_default(),
        );

        instances.push(instance.clone());
        self.save_instances(&instances)?;
        Ok(instance)
    }

    pub fn update_instance(&self, updated: InstanceConfig) -> Result<InstanceConfig, String> {
        let mut instances = self.load_instances()?;
        let pos = instances
            .iter()
            .position(|i| i.id == updated.id)
            .ok_or_else(|| format!("Instance '{}' not found.", updated.id))?;
        instances[pos] = updated.clone();
        self.save_instances(&instances)?;

        if updated.source == InstanceSource::Custom {
            let _ = fs::write(
                updated.game_dir.join("instance.json"),
                serde_json::to_string_pretty(&updated).unwrap_or_default(),
            );
        }
        Ok(updated)
    }

    pub fn delete_instance(&self, instance_id: &str, delete_files: bool) -> Result<(), String> {
        let mut instances = self.load_instances()?;
        if let Some(target) = instances.iter().find(|i| i.id == instance_id).cloned() {
            if delete_files && target.source == InstanceSource::Custom && target.game_dir.exists() {
                let _ = fs::remove_dir_all(&target.game_dir);
            }
        }
        instances.retain(|i| i.id != instance_id);
        self.save_instances(&instances)
    }

    /// Upserts a discovered CurseForge instance into the launcher registry without copying files.
    pub fn upsert_curseforge_instance(
        &self,
        discovered: InstanceConfig,
    ) -> Result<(InstanceConfig, bool), String> {
        let mut instances = self.load_instances()?;
        if let Some(existing) = instances
            .iter_mut()
            .find(|i| i.game_dir == discovered.game_dir || i.id == discovered.id)
        {
            existing.name = discovered.name;
            existing.mc_version = discovered.mc_version;
            existing.modloader = discovered.modloader;
            existing.modloader_version = discovered.modloader_version;
            let result = existing.clone();
            self.save_instances(&instances)?;
            Ok((result, false))
        } else {
            instances.push(discovered.clone());
            self.save_instances(&instances)?;
            Ok((discovered, true))
        }
    }
}
