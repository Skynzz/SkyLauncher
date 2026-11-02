use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub variables: HashMap<String, String>,
    pub custom_css: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub enabled: bool,
    pub hooks: Vec<String>,
    pub script_code: String,
}

pub struct ExtensibilityManager {
    themes_dir: PathBuf,
    plugins_dir: PathBuf,
}

impl ExtensibilityManager {
    pub fn new(app_data_dir: &Path) -> Self {
        let themes_dir = app_data_dir.join("themes");
        let plugins_dir = app_data_dir.join("plugins");
        let _ = fs::create_dir_all(&themes_dir);
        let _ = fs::create_dir_all(&plugins_dir);

        let mgr = Self {
            themes_dir,
            plugins_dir,
        };
        mgr.seed_builtin_themes();
        mgr.seed_sample_plugins();
        mgr
    }

    fn seed_builtin_themes(&self) {
        let presets = [
            (
                "midnight-zinc",
                "Gece Siyahı (Varsayılan)",
                "SkyLauncher",
                "Minimalist derin siyah arayüz ve mavi vurgular.",
                vec![
                    ("--bg-primary", "#09090b"),
                    ("--bg-secondary", "#121215"),
                    ("--bg-card", "#18181c"),
                    ("--bg-elevated", "#222228"),
                    ("--accent-primary", "#3b82f6"),
                    ("--accent-secondary", "#60a5fa"),
                    ("--accent-glow", "rgba(59, 130, 246, 0.2)"),
                    ("--text-primary", "#fafafa"),
                    ("--text-secondary", "#a1a1aa"),
                    ("--text-muted", "#71717a"),
                    ("--border-subtle", "rgba(255, 255, 255, 0.09)"),
                ],
                "",
            ),
            (
                "snow-white",
                "Kar Beyazı",
                "SkyLauncher",
                "Ferah, aydınlık ve temiz beyaz modern tema.",
                vec![
                    ("--bg-primary", "#f8fafc"),
                    ("--bg-secondary", "#f1f5f9"),
                    ("--bg-card", "#ffffff"),
                    ("--bg-elevated", "#e2e8f0"),
                    ("--accent-primary", "#2563eb"),
                    ("--accent-secondary", "#3b82f6"),
                    ("--accent-glow", "rgba(37, 99, 235, 0.15)"),
                    ("--text-primary", "#0f172a"),
                    ("--text-secondary", "#475569"),
                    ("--text-muted", "#64748b"),
                    ("--border-subtle", "rgba(15, 23, 42, 0.08)"),
                ],
                "",
            ),
            (
                "sky-slate",
                "Sky Slate",
                "SkyLauncher",
                "Göz yormayan koyu arduvaz ve gök mavisi vurgu renkleri.",
                vec![
                    ("--bg-primary", "#0d1017"),
                    ("--bg-secondary", "#131720"),
                    ("--bg-card", "#181d27"),
                    ("--bg-elevated", "#202734"),
                    ("--accent-primary", "#0ea5e9"),
                    ("--accent-secondary", "#38bdf8"),
                    ("--accent-glow", "rgba(14, 165, 233, 0.2)"),
                    ("--text-primary", "#f1f5f9"),
                    ("--text-secondary", "#94a3b8"),
                    ("--text-muted", "#64748b"),
                    ("--border-subtle", "rgba(148, 163, 184, 0.12)"),
                ],
                "",
            ),
            (
                "emerald-forest",
                "Zümrüt Yeşili",
                "SkyLauncher",
                "Yumuşak koyu tonlar ve zümrüt yeşili vurgular.",
                vec![
                    ("--bg-primary", "#0b110e"),
                    ("--bg-secondary", "#101915"),
                    ("--bg-card", "#16211c"),
                    ("--bg-elevated", "#1d2c25"),
                    ("--accent-primary", "#10b981"),
                    ("--accent-secondary", "#34d399"),
                    ("--accent-glow", "rgba(16, 185, 129, 0.2)"),
                    ("--text-primary", "#f0fdf4"),
                    ("--text-secondary", "#a7f3d0"),
                    ("--text-muted", "#6ee7b7"),
                    ("--border-subtle", "rgba(16, 185, 129, 0.14)"),
                ],
                "",
            ),
        ];

        for (id, name, author, desc, vars, css) in presets {
            let folder = self.themes_dir.join(id);
            let json_path = folder.join("theme.json");
            let css_path = folder.join("theme.css");
            let _ = fs::create_dir_all(&folder);
            let mut map = HashMap::new();
            for (k, v) in vars {
                map.insert(k.to_string(), v.to_string());
            }
            let manifest = serde_json::json!({
                "id": id,
                "name": name,
                "author": author,
                "description": desc,
                "variables": map
            });
            let _ = fs::write(
                &json_path,
                serde_json::to_string_pretty(&manifest).unwrap_or_default(),
            );
            let _ = fs::write(&css_path, css);
        }
    }

    fn seed_sample_plugins(&self) {
        let sample_dir = self.plugins_dir.join("performance-guard");
        let json_path = sample_dir.join("plugin.json");
        let js_path = sample_dir.join("index.js");

        if !json_path.exists() {
            let _ = fs::create_dir_all(&sample_dir);
            let manifest = serde_json::json!({
                "id": "performance-guard",
                "name": "Performans & Çökme Koruması",
                "version": "1.0.0",
                "author": "SkyLauncher",
                "description": "Oyun açılışını ve bellek durumunu otomatik izler.",
                "enabled": true,
                "hooks": ["onAppInit", "onInstanceDetected", "onPreLaunch", "onGameLog", "onGameCrash"]
            });
            let _ = fs::write(
                &json_path,
                serde_json::to_string_pretty(&manifest).unwrap_or_default(),
            );

            let script = r#"export default {
  onAppInit(context) {
    context.log("Performans koruması aktif.");
  },
  onInstanceDetected(instance, context) {
    context.log(`Paket algılandı: ${instance.name}`);
  },
  onPreLaunch(instanceConfig, context) {
    context.log(`${instanceConfig.name} başlatılıyor...`);
  },
  onGameLog(line, context) {},
  onGameCrash(crashReport, context) {
    context.log(`Çökme yakalandı: ${crashReport.category}`);
  }
};
"#;
            let _ = fs::write(&js_path, script);
        }
    }

    pub fn load_themes(&self) -> Result<Vec<ThemeManifest>, String> {
        let mut themes = Vec::new();
        let entries = fs::read_dir(&self.themes_dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let json_path = dir.join("theme.json");
            let css_path = dir.join("theme.css");
            if !json_path.exists() {
                continue;
            }

            if let Ok(raw) = fs::read_to_string(&json_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let id = val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("custom")
                        .to_string();
                    // Skip legacy noisy themes from older build
                    if id == "cyberpunk-obsidian" || id == "emerald-matrix" || id == "crimson-forge" {
                        continue;
                    }
                    let name = val
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    let author = val
                        .get("author")
                        .and_then(|v| v.as_str())
                        .unwrap_or("SkyLauncher")
                        .to_string();
                    let description = val
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let mut variables = HashMap::new();
                    if let Some(vars_obj) = val.get("variables").and_then(|v| v.as_object()) {
                        for (k, v) in vars_obj {
                            if let Some(s) = v.as_str() {
                                variables.insert(k.clone(), s.to_string());
                            }
                        }
                    }

                    let custom_css = fs::read_to_string(&css_path).unwrap_or_default();
                    themes.push(ThemeManifest {
                        id,
                        name,
                        author,
                        description,
                        variables,
                        custom_css,
                    });
                }
            }
        }

        themes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(themes)
    }

    pub fn load_plugins(&self) -> Result<Vec<PluginDescriptor>, String> {
        let mut plugins = Vec::new();
        let entries = fs::read_dir(&self.plugins_dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let json_path = dir.join("plugin.json");
            let js_path = dir.join("index.js");
            if !json_path.exists() || !js_path.exists() {
                continue;
            }

            if let Ok(raw) = fs::read_to_string(&json_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let id = val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("plugin")
                        .to_string();
                    let name = val
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    let version = val
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1.0.0")
                        .to_string();
                    let author = val
                        .get("author")
                        .and_then(|v| v.as_str())
                        .unwrap_or("SkyLauncher")
                        .to_string();
                    let description = val
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let enabled = val
                        .get("enabled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    let hooks = val
                        .get("hooks")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    let script_code = fs::read_to_string(&js_path).unwrap_or_default();
                    plugins.push(PluginDescriptor {
                        id,
                        name,
                        version,
                        author,
                        description,
                        enabled,
                        hooks,
                        script_code,
                    });
                }
            }
        }

        plugins.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(plugins)
    }

    pub fn toggle_plugin(
        &self,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<Vec<PluginDescriptor>, String> {
        let json_path = self.plugins_dir.join(plugin_id).join("plugin.json");
        if !json_path.exists() {
            return Err(format!("Eklenti '{}' bulunamadı.", plugin_id));
        }
        let raw = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
        let mut val: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if let Some(obj) = val.as_object_mut() {
            obj.insert("enabled".to_string(), serde_json::Value::Bool(enabled));
        }
        fs::write(
            &json_path,
            serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        self.load_plugins()
    }
}
