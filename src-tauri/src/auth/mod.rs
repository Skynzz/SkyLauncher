use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineProfile {
    pub id: String,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub avatar_color: String,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthStore {
    pub active_profile_id: Option<String>,
    pub profiles: Vec<OfflineProfile>,
}

/// Generates a deterministic RFC 4122 v3 UUID from the username using the standard
/// Minecraft offline algorithm: `MD5("OfflinePlayer:" + username)`.
pub fn generate_offline_uuid(username: &str) -> String {
    let input = format!("OfflinePlayer:{}", username.trim());
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let mut hash: [u8; 16] = hasher.finalize().into();

    // Set RFC 4122 version 3 (MD5 namespace) bits: 0011xxxx
    hash[6] = (hash[6] & 0x0f) | 0x30;
    // Set RFC 4122 variant bits: 10xxxxxx
    hash[8] = (hash[8] & 0x3f) | 0x80;

    Uuid::from_bytes(hash).hyphenated().to_string()
}

/// Returns launch arguments required for offline developer mode authentication.
pub fn build_offline_auth_args(profile: &OfflineProfile) -> Vec<(String, String)> {
    vec![
        ("--username".to_string(), profile.username.clone()),
        ("--uuid".to_string(), profile.uuid.clone()),
        ("--accessToken".to_string(), "0".to_string()),
        ("--userType".to_string(), "legacy".to_string()),
    ]
}

pub struct ProfileManager {
    store_path: PathBuf,
}

impl ProfileManager {
    pub fn new(app_data_dir: &Path) -> Self {
        let store_path = app_data_dir.join("profiles.json");
        let manager = Self { store_path };
        manager.ensure_default_profile();
        manager
    }

    fn ensure_default_profile(&self) {
        if !self.store_path.exists() {
            let default_username = "Developer";
            let uuid = generate_offline_uuid(default_username);
            let now = Utc::now();
            let profile = OfflineProfile {
                id: uuid.clone(),
                username: default_username.to_string(),
                uuid,
                access_token: "0".to_string(),
                user_type: "legacy".to_string(),
                avatar_color: "#6366f1".to_string(),
                created_at: now,
                last_used: now,
            };
            let store = AuthStore {
                active_profile_id: Some(profile.id.clone()),
                profiles: vec![profile],
            };
            let _ = self.save_store(&store);
        }
    }

    pub fn load_store(&self) -> Result<AuthStore, String> {
        if !self.store_path.exists() {
            self.ensure_default_profile();
        }
        let raw = fs::read_to_string(&self.store_path)
            .map_err(|e| format!("Failed to read profiles.json: {}", e))?;
        serde_json::from_str(&raw).map_err(|e| format!("Invalid profiles.json format: {}", e))
    }

    pub fn save_store(&self, store: &AuthStore) -> Result<(), String> {
        if let Some(parent) = self.store_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let serialized = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
        fs::write(&self.store_path, serialized).map_err(|e| e.to_string())
    }

    pub fn create_or_update_profile(
        &self,
        username: &str,
        avatar_color: Option<String>,
    ) -> Result<AuthStore, String> {
        let clean = username.trim();
        if clean.is_empty() || clean.len() > 16 {
            return Err("Username must be between 1 and 16 characters.".to_string());
        }
        if !clean
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("Username can only contain letters, numbers, and underscores.".to_string());
        }

        let mut store = self.load_store()?;
        let uuid = generate_offline_uuid(clean);
        let now = Utc::now();

        if let Some(existing) = store
            .profiles
            .iter_mut()
            .find(|p| p.username.eq_ignore_ascii_case(clean))
        {
            existing.username = clean.to_string();
            existing.uuid = uuid.clone();
            existing.last_used = now;
            if let Some(color) = avatar_color {
                existing.avatar_color = color;
            }
            store.active_profile_id = Some(existing.id.clone());
        } else {
            let palette = ["#6366f1", "#10b981", "#f59e0b", "#ec4899", "#06b6d4", "#8b5cf6"];
            let default_color = palette[store.profiles.len() % palette.len()].to_string();
            let profile = OfflineProfile {
                id: uuid.clone(),
                username: clean.to_string(),
                uuid,
                access_token: "0".to_string(),
                user_type: "legacy".to_string(),
                avatar_color: avatar_color.unwrap_or(default_color),
                created_at: now,
                last_used: now,
            };
            store.active_profile_id = Some(profile.id.clone());
            store.profiles.push(profile);
        }

        self.save_store(&store)?;
        Ok(store)
    }

    pub fn set_active_profile(&self, profile_id: &str) -> Result<AuthStore, String> {
        let mut store = self.load_store()?;
        let found = store.profiles.iter_mut().find(|p| p.id == profile_id);
        if let Some(profile) = found {
            profile.last_used = Utc::now();
            store.active_profile_id = Some(profile.id.clone());
            self.save_store(&store)?;
            Ok(store)
        } else {
            Err(format!("Profile '{}' not found.", profile_id))
        }
    }

    pub fn delete_profile(&self, profile_id: &str) -> Result<AuthStore, String> {
        let mut store = self.load_store()?;
        if store.profiles.len() <= 1 {
            return Err("At least one local profile must remain.".to_string());
        }
        store.profiles.retain(|p| p.id != profile_id);
        if store.active_profile_id.as_deref() == Some(profile_id) {
            store.active_profile_id = store.profiles.first().map(|p| p.id.clone());
        }
        self.save_store(&store)?;
        Ok(store)
    }

    pub fn get_active_profile(&self) -> Result<OfflineProfile, String> {
        let store = self.load_store()?;
        if let Some(active_id) = &store.active_profile_id {
            if let Some(p) = store.profiles.iter().find(|p| &p.id == active_id) {
                return Ok(p.clone());
            }
        }
        store
            .profiles
            .into_iter()
            .next()
            .ok_or_else(|| "No active profile configured.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_uuid_generation_rfc4122_v3() {
        // Standard Minecraft OfflinePlayer UUID for "Notch"
        // Java UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8)) -> b50ad385-829d-3141-a216-7e7d7539ba7f
        let notch_uuid = generate_offline_uuid("Notch");
        assert_eq!(notch_uuid, "b50ad385-829d-3141-a216-7e7d7539ba7f");

        let parsed = Uuid::parse_str(&notch_uuid).expect("Must be valid UUID");
        assert_eq!(parsed.get_version_num(), 3, "UUID version must be RFC 4122 v3");

        // Deterministic check
        assert_eq!(
            generate_offline_uuid("Developer"),
            generate_offline_uuid("Developer")
        );
    }
}
