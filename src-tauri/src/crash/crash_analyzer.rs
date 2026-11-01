use crate::instance::jar_parser::find_culprit_jar_by_mixin_or_modid;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashCategory {
    MissingDependency,
    MixinConflict,
    OutOfMemory,
    JavaVersionMismatch,
    CorruptedConfig,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryAction {
    pub action_type: String, // "increase_ram" | "switch_java" | "reset_config" | "disable_mod"
    pub label_tr: String,
    pub label_en: String,
    pub target_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAnalysisReport {
    pub instance_id: String,
    pub exit_code: i32,
    pub category: CrashCategory,
    pub title_tr: String,
    pub title_en: String,
    pub summary_tr: String,
    pub summary_en: String,
    pub culprit_mod_jar: Option<String>,
    pub missing_mod_id: Option<String>,
    pub required_version_range: Option<String>,
    pub failing_mixin_json: Option<String>,
    pub corrupted_config_file: Option<String>,
    pub required_java_version: Option<u32>,
    pub recovery_actions: Vec<RecoveryAction>,
    pub relevant_log_lines: Vec<String>,
    pub analyzed_at: DateTime<Utc>,
}

static RE_MISSING_DEP_FORGE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:ModResolutionException|Missing or unsupported mandatory dependencies|Mod \S+ requires)[\s\S]{0,320}?(?:Mod ID:\s*'([^']+)'|requires\s+'?([a-z0-9_\-]+)'?\s*(?:version\s*)?([^\s,;]+)?)(?:[\s\S]{0,160}?Expected Range:\s*'([^']+)')?").unwrap()
});

static RE_MISSING_DEP_FABRIC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:Mod\s+'[^']+'\s+\(([^)]+)\)\s+\S+\s+requires\s+(?:any version of|version\s+([^\s]+)\s+of)\s+([a-z0-9_\-]+)|requires\s+([a-z0-9_\-]+)\s+([^\s,]+),\s+which is missing)").unwrap()
});

static RE_MIXIN_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:MixinApplyError|MixinTransformerError|Mixin prepare failed|Critical injection failure)[\s\S]{0,350}?([a-z0-9_.\-]+\.mixins?\.json)(?:\s+from mod\s+([a-z0-9_\-]+))?").unwrap()
});

static RE_OOM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)java\.lang\.OutOfMemoryError(?::\s*([^\r\n]+))?").unwrap()
});

static RE_CLASS_VERSION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)UnsupportedClassVersionError:.*class file version\s+(\d+)\.\d+").unwrap()
});

static RE_CORRUPTED_CONFIG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:NightConfig|TomlParsingException|ParsingException|Failed loading config file)[\s\S]{0,240}?([a-z0-9_.\-/\\]+\.(?:toml|json))").unwrap()
});

/// Reads `logs/latest.log`, the newest file in `crash-reports/`, and the process output buffer,
/// then classifies the crash and builds one-click recovery actions.
pub fn analyze_instance_crash(
    instance_id: &str,
    game_dir: &Path,
    exit_code: i32,
    process_output: &str,
) -> CrashAnalysisReport {
    let mut combined = String::with_capacity(process_output.len() + 16384);
    combined.push_str(process_output);
    combined.push('\n');

    let latest_log_path = game_dir.join("logs").join("latest.log");
    if let Ok(log_content) = fs::read_to_string(&latest_log_path) {
        combined.push_str(&log_content);
        combined.push('\n');
    }

    if let Some(newest_crash) = read_newest_crash_report(&game_dir.join("crash-reports")) {
        combined.push_str(&newest_crash);
    }

    parse_crash_text(instance_id, Some(game_dir), exit_code, &combined)
}

fn read_newest_crash_report(crash_dir: &Path) -> Option<String> {
    let entries = fs::read_dir(crash_dir).ok()?;
    let mut newest_file = None;
    let mut newest_time = std::time::SystemTime::UNIX_EPOCH;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified > newest_time {
                        newest_time = modified;
                        newest_file = Some(path);
                    }
                }
            }
        }
    }

    newest_file.and_then(|p| fs::read_to_string(p).ok())
}

/// Pure text analyzer used both at runtime and in unit tests.
pub fn parse_crash_text(
    instance_id: &str,
    game_dir: Option<&Path>,
    exit_code: i32,
    raw_log: &str,
) -> CrashAnalysisReport {
    let relevant_log_lines: Vec<String> = raw_log
        .lines()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("exception")
                || lower.contains("error")
                || lower.contains("mixin")
                || lower.contains("mandatory dependencies")
                || lower.contains("requires")
                || lower.contains("nightconfig")
                || lower.contains("caused by")
        })
        .take(25)
        .map(|s| s.to_string())
        .collect();

    // 1. Check OutOfMemoryError
    if let Some(caps) = RE_OOM.captures(raw_log) {
        let detail = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("Java heap space");
        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::OutOfMemory,
            title_tr: "Yetersiz Bellek (OutOfMemoryError)".to_string(),
            title_en: "Out of Memory (OutOfMemoryError)".to_string(),
            summary_tr: format!(
                "Oyun JVM bellek sınırına ulaştı ({}). Maksimum RAM miktarını tek tıkla +2048 MB artırabilirsiniz.",
                detail
            ),
            summary_en: format!(
                "The JVM ran out of allocated memory ({}). You can increase Max RAM by +2048 MB with one click.",
                detail
            ),
            culprit_mod_jar: None,
            missing_mod_id: None,
            required_version_range: None,
            failing_mixin_json: None,
            corrupted_config_file: None,
            required_java_version: None,
            recovery_actions: vec![RecoveryAction {
                action_type: "increase_ram".to_string(),
                label_tr: "Maksimum RAM'i +2048 MB Artır ve Kaydet".to_string(),
                label_en: "Increase Max RAM by +2048 MB".to_string(),
                target_value: "2048".to_string(),
            }],
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    // 2. Check UnsupportedClassVersionError (Java Version Mismatch)
    if let Some(caps) = RE_CLASS_VERSION.captures(raw_log) {
        let class_major: u32 = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(65);
        let required_java = match class_major {
            65..=99 => 21,
            61..=64 => 17,
            _ => 8,
        };

        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::JavaVersionMismatch,
            title_tr: format!("Java Sürüm Uyuşmazlığı (Java {} Gerekli)", required_java),
            title_en: format!("Java Version Mismatch (Requires Java {})", required_java),
            summary_tr: format!(
                "Sınıf dosyası sürümü {}.0 algılandı. Bu sürüm veya modlar Java {} çalışma zamanı gerektiriyor.",
                class_major, required_java
            ),
            summary_en: format!(
                "Detected class file version {}.0. This Minecraft version or its mods require Java {}.",
                class_major, required_java
            ),
            culprit_mod_jar: None,
            missing_mod_id: None,
            required_version_range: None,
            failing_mixin_json: None,
            corrupted_config_file: None,
            required_java_version: Some(required_java),
            recovery_actions: vec![RecoveryAction {
                action_type: "switch_java".to_string(),
                label_tr: format!("Java {} Runtime'a Geçiş Yap (Gerekirse İndir)", required_java),
                label_en: format!("Switch to Java {} Runtime (Auto-Download if Missing)", required_java),
                target_value: required_java.to_string(),
            }],
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    // 3. Check MixinConflict
    if let Some(caps) = RE_MIXIN_ERROR.captures(raw_log) {
        let mixin_json = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown.mixins.json".to_string());
        let mod_hint = caps.get(2).map(|m| m.as_str().to_string());

        let mut culprit_jar = game_dir.and_then(|gd| {
            find_culprit_jar_by_mixin_or_modid(gd, &mixin_json).or_else(|| {
                mod_hint
                    .as_deref()
                    .and_then(|mh| find_culprit_jar_by_mixin_or_modid(gd, mh))
            })
        });

        if culprit_jar.is_none() {
            // Derive candidate JAR name from mixin json prefix if game_dir isn't available
            let prefix = mixin_json
                .split('.')
                .next()
                .unwrap_or("culprit-mod");
            culprit_jar = Some(format!("{}.jar", prefix));
        }

        let mut actions = Vec::new();
        if let Some(ref jar) = culprit_jar {
            actions.push(RecoveryAction {
                action_type: "disable_mod".to_string(),
                label_tr: format!("Suçlu Modu Devre Dışı Bırak ({}.disabled) & Yeniden Başlat", jar),
                label_en: format!("Disable Culprit Mod ({}.disabled) & Relaunch", jar),
                target_value: jar.clone(),
            });
        }

        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::MixinConflict,
            title_tr: format!("Mixin Enjeksiyon Çakışması ({})", mixin_json),
            title_en: format!("Mixin Injection Conflict ({})", mixin_json),
            summary_tr: format!(
                "'{}' yapılandırması bytecode enjeksiyonu sırasında çöktü. İlgili mod devre dışı bırakılarak oyun kurtarılabilir.",
                mixin_json
            ),
            summary_en: format!(
                "Mixin configuration '{}' failed during bytecode transformation. Disabling the culprit mod will resolve the crash.",
                mixin_json
            ),
            culprit_mod_jar: culprit_jar,
            missing_mod_id: None,
            required_version_range: None,
            failing_mixin_json: Some(mixin_json),
            corrupted_config_file: None,
            required_java_version: None,
            recovery_actions: actions,
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    // 4. Check MissingDependency (Forge/NeoForge & Fabric/Quilt)
    if let Some(caps) = RE_MISSING_DEP_FORGE.captures(raw_log) {
        let missing_id = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown_mod".to_string());
        let range = caps
            .get(4)
            .or_else(|| caps.get(3))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "*".to_string());

        let actions = vec![RecoveryAction {
            action_type: "install_missing_mod".to_string(),
            label_tr: format!(
                "Eksik Modu Otomatik İndir ({}) & Gereksinimleri Tamamla",
                missing_id
            ),
            label_en: format!(
                "Auto-Download Missing Mod ({}) & Heal Dependencies",
                missing_id
            ),
            target_value: missing_id.clone(),
        }];

        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::MissingDependency,
            title_tr: format!("Eksik Zorunlu Mod Bağımlılığı: {}", missing_id),
            title_en: format!("Missing Mandatory Dependency: {}", missing_id),
            summary_tr: format!(
                "Yüklü modlardan biri '{}' modunun '{}' sürüm aralığını zorunlu kılıyor ancak bu mod bulunamadı.",
                missing_id, range
            ),
            summary_en: format!(
                "An installed mod requires '{}' (version range '{}'), which is missing or incompatible.",
                missing_id, range
            ),
            culprit_mod_jar: None,
            missing_mod_id: Some(missing_id),
            required_version_range: Some(range),
            failing_mixin_json: None,
            corrupted_config_file: None,
            required_java_version: None,
            recovery_actions: actions,
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    if let Some(caps) = RE_MISSING_DEP_FABRIC.captures(raw_log) {
        let culprit_mod = caps.get(1).map(|m| m.as_str().to_string());
        let missing_id = caps
            .get(3)
            .or_else(|| caps.get(4))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown_dependency".to_string());
        let range = caps
            .get(2)
            .or_else(|| caps.get(5))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "*".to_string());

        let culprit_jar = culprit_mod.as_ref().and_then(|cm| {
            game_dir
                .and_then(|gd| find_culprit_jar_by_mixin_or_modid(gd, cm))
                .or_else(|| Some(format!("{}.jar", cm)))
        });

        let mut actions = vec![RecoveryAction {
            action_type: "install_missing_mod".to_string(),
            label_tr: format!(
                "Eksik Modu Otomatik İndir ({}) & Gereksinimleri Tamamla",
                missing_id
            ),
            label_en: format!(
                "Auto-Download Missing Mod ({}) & Heal Dependencies",
                missing_id
            ),
            target_value: missing_id.clone(),
        }];
        if let Some(ref jar) = culprit_jar {
            actions.push(RecoveryAction {
                action_type: "disable_mod".to_string(),
                label_tr: format!("Suçlu Modu Devre Dışı Bırak ({}.disabled) & Yeniden Başlat", jar),
                label_en: format!("Disable Culprit Mod ({}.disabled) & Relaunch", jar),
                target_value: jar.clone(),
            });
        }

        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::MissingDependency,
            title_tr: format!("Eksik Zorunlu Mod Bağımlılığı: {}", missing_id),
            title_en: format!("Missing Mandatory Dependency: {}", missing_id),
            summary_tr: format!(
                "'{}' modu çalışmak için '{}' ({}) bağımlılığına ihtiyaç duyuyor.",
                culprit_mod.as_deref().unwrap_or("Bir mod"),
                missing_id,
                range
            ),
            summary_en: format!(
                "Mod '{}' requires dependency '{}' (version {}).",
                culprit_mod.as_deref().unwrap_or("A mod"),
                missing_id,
                range
            ),
            culprit_mod_jar: culprit_jar,
            missing_mod_id: Some(missing_id),
            required_version_range: Some(range),
            failing_mixin_json: None,
            corrupted_config_file: None,
            required_java_version: None,
            recovery_actions: actions,
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    // 5. Check CorruptedConfig (NightConfig / TomlParsingException)
    if let Some(caps) = RE_CORRUPTED_CONFIG.captures(raw_log) {
        let cfg_file = caps
            .get(1)
            .map(|m| m.as_str().replace('\\', "/"))
            .unwrap_or_else(|| "unknown.toml".to_string());

        return CrashAnalysisReport {
            instance_id: instance_id.to_string(),
            exit_code,
            category: CrashCategory::CorruptedConfig,
            title_tr: format!("Bozuk Yapılandırma Dosyası ({})", cfg_file),
            title_en: format!("Corrupted Configuration File ({})", cfg_file),
            summary_tr: format!(
                "config/ klasöründeki '{}' dosyası bozulmuş (NightConfig / TOML ayrıştırma hatası). Tek tıkla yedeğini alıp sıfırlayabilirsiniz.",
                cfg_file
            ),
            summary_en: format!(
                "Configuration file '{}' in config/ is corrupted. You can back it up (.bak) and reset it with one click.",
                cfg_file
            ),
            culprit_mod_jar: None,
            missing_mod_id: None,
            required_version_range: None,
            failing_mixin_json: None,
            corrupted_config_file: Some(cfg_file.clone()),
            required_java_version: None,
            recovery_actions: vec![RecoveryAction {
                action_type: "reset_config".to_string(),
                label_tr: format!("'{}' Dosyasını Yedekle (.bak) ve Sıfırla", cfg_file),
                label_en: format!("Backup (.bak) & Reset '{}'", cfg_file),
                target_value: cfg_file,
            }],
            relevant_log_lines,
            analyzed_at: Utc::now(),
        };
    }

    CrashAnalysisReport {
        instance_id: instance_id.to_string(),
        exit_code,
        category: CrashCategory::Unknown,
        title_tr: format!("Beklenmeyen Oyun Çökmesi (Kod: {})", exit_code),
        title_en: format!("Unexpected Game Crash (Exit Code: {})", exit_code),
        summary_tr: "Oyun sıfır olmayan bir hata koduyla kapandı. Aşağıdaki günlük satırlarını inceleyebilirsiniz.".to_string(),
        summary_en: "The game process exited with a non-zero status code. Inspect the log excerpt below.".to_string(),
        culprit_mod_jar: None,
        missing_mod_id: None,
        required_version_range: None,
        failing_mixin_json: None,
        corrupted_config_file: None,
        required_java_version: None,
        recovery_actions: vec![],
        relevant_log_lines,
        analyzed_at: Utc::now(),
    }
}

/// Backs up a corrupted config file in `<game_dir>/config/` to `<filename>.bak` and removes the broken file
/// so the mod regenerates a clean default configuration on next launch.
pub fn backup_and_reset_config_file(game_dir: &Path, relative_or_name: &str) -> Result<String, String> {
    let clean = relative_or_name
        .trim_start_matches("config/")
        .trim_start_matches("config\\");
    let target = game_dir.join("config").join(clean);

    if !target.exists() {
        return Err(format!("Config file {:?} not found.", target));
    }

    let backup_path = target.with_extension(format!(
        "{}.bak",
        target
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("toml")
    ));

    fs::copy(&target, &backup_path)
        .map_err(|e| format!("Failed to create backup {:?}: {}", backup_path, e))?;
    fs::remove_file(&target)
        .map_err(|e| format!("Failed to reset corrupted config {:?}: {}", target, e))?;

    Ok(backup_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_analyzer_all_five_categories() {
        // 1. Missing Dependency (Forge ModResolutionException)
        let forge_dep_log = r#"
        [main/ERROR] [net.minecraftforge.fml.loading.ModSorter/LOADING]: Missing or unsupported mandatory dependencies:
            Mod ID: 'curios', Requested by: 'artifacts', Expected Range: '[5.9.0,)', Actual Version: '[MISSING]'
        net.minecraftforge.fml.loading.ModResolutionException: Mod Resolution Failed
        "#;
        let rep1 = parse_crash_text("test-inst", None, 1, forge_dep_log);
        assert_eq!(rep1.category, CrashCategory::MissingDependency);
        assert_eq!(rep1.missing_mod_id.as_deref(), Some("curios"));
        assert_eq!(rep1.required_version_range.as_deref(), Some("[5.9.0,)"));

        // 2. Mixin Conflict
        let mixin_log = r#"
        org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered
        Caused by: org.spongepowered.asm.mixin.throwables.MixinApplyError: Mixin [valkyrienskies-common.mixins.json:MixinMinecraft] from phase [DEFAULT] in config [valkyrienskies-common.mixins.json] from mod valkyrienskies FAILED
        "#;
        let rep2 = parse_crash_text("test-inst", None, 1, mixin_log);
        assert_eq!(rep2.category, CrashCategory::MixinConflict);
        assert_eq!(
            rep2.failing_mixin_json.as_deref(),
            Some("valkyrienskies-common.mixins.json")
        );
        assert!(rep2
            .recovery_actions
            .iter()
            .any(|a| a.action_type == "disable_mod"));

        // 3. Out of Memory
        let oom_log = "java.lang.OutOfMemoryError: Java heap space\n\tat java.base/java.util.Arrays.copyOf(Arrays.java:3512)";
        let rep3 = parse_crash_text("test-inst", None, 1, oom_log);
        assert_eq!(rep3.category, CrashCategory::OutOfMemory);
        assert_eq!(rep3.recovery_actions[0].action_type, "increase_ram");

        // 4. Java Version Mismatch
        let java_log = "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0";
        let rep4 = parse_crash_text("test-inst", None, 1, java_log);
        assert_eq!(rep4.category, CrashCategory::JavaVersionMismatch);
        assert_eq!(rep4.required_java_version, Some(21));

        // 5. Corrupted Config
        let cfg_log = "com.electronwill.nightconfig.core.io.ParsingException: Not enough data available at file config/mekanism/client.toml";
        let rep5 = parse_crash_text("test-inst", None, 1, cfg_log);
        assert_eq!(rep5.category, CrashCategory::CorruptedConfig);
        assert_eq!(
            rep5.corrupted_config_file.as_deref(),
            Some("config/mekanism/client.toml")
        );
    }
}
