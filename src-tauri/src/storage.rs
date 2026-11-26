use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceMeta {
    pub mac: String,
    pub alias: Option<String>,
    pub trusted: bool,
    pub blocked: bool,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
    pub timestamp: u64,
    pub kind: String, // "new_device" | "scan" | "router"
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub auto_scan_enabled: bool,
    pub scan_interval_secs: u64,
    pub notify_new_device: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_scan_enabled: true,
            scan_interval_secs: 180,
            notify_new_device: true,
            minimize_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouterBinding {
    pub gateway_ip: String,
    pub adapter_id: String,
    pub username: Option<String>,
    pub bound_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentalRule {
    pub id: String,
    pub mac: String,
    pub device_name: String,
    pub start_hhmm: String, // 例如 "23:00"
    pub end_hhmm: String,   // 例如 "07:00"
    pub enabled: bool,
    #[serde(default)]
    pub currently_enforced: bool,
    #[serde(default)]
    pub last_notified_day: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestWifiState {
    pub enabled: bool,
    pub ssid: String,
    pub password: String,
    pub auto_off_at: Option<u64>,
}

impl Default for GuestWifiState {
    fn default() -> Self {
        Self {
            enabled: false,
            ssid: "MyHome_Guest".to_string(),
            password: String::new(),
            auto_off_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreData {
    pub devices: HashMap<String, DeviceMeta>,
    pub onboarding_done: bool,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub events: Vec<NetworkEvent>,
    #[serde(default)]
    pub router_binding: Option<RouterBinding>,
    #[serde(default)]
    pub parental_rules: Vec<ParentalRule>,
    #[serde(default)]
    pub guest_wifi: GuestWifiState,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load_store(path: &PathBuf) -> StoreData {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => StoreData::default(),
    }
}

pub fn save_store(path: &PathBuf, data: &StoreData) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建数据目录失败: {}", e))?;
    }
    let json = serde_json::to_string_pretty(data).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(path, json).map_err(|e| format!("保存本地数据失败: {}", e))
}

#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "wifi-buddy-router";

/// 将路由器管理员密码保存至系统安全存储（macOS Keychain / Windows DPAPI）
pub fn save_router_password(data_dir: &Path, gateway_ip: &str, password: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        let status = Command::new("security")
            .args([
                "add-generic-password",
                "-U",
                "-a",
                gateway_ip,
                "-s",
                KEYCHAIN_SERVICE,
                "-w",
                password,
            ])
            .output()
            .map_err(|e| format!("调用 macOS 钥匙串失败: {}", e))?;
        if status.status.success() {
            return Ok(());
        }
        return Err("保存密码到 macOS 钥匙串失败".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        fs::create_dir_all(data_dir).map_err(|e| format!("创建凭据目录失败: {}", e))?;
        let cred_file = data_dir.join(format!(
            "router_{}.dpapi",
            gateway_ip.replace('.', "_")
        ));
        // 使用当前 Windows 登录用户 DPAPI 加密后落盘
        let ps = format!(
            "$sec = ConvertTo-SecureString -String '{}' -AsPlainText -Force; ConvertFrom-SecureString $sec | Set-Content -Path '{}' -Encoding UTF8",
            password.replace('\'', "''"),
            cred_file.to_string_lossy().replace('\'', "''")
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .map_err(|e| format!("调用 Windows 安全存储失败: {}", e))?;
        if out.status.success() {
            return Ok(());
        }
        return Err("保存密码到 Windows 安全存储失败".to_string());
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (data_dir, gateway_ip, password);
        Err("当前平台暂不支持安全存储路由器密码".to_string())
    }
}

pub fn load_router_password(data_dir: &Path, gateway_ip: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        let out = Command::new("security")
            .args([
                "find-generic-password",
                "-a",
                gateway_ip,
                "-s",
                KEYCHAIN_SERVICE,
                "-w",
            ])
            .output()
            .ok()?;
        if out.status.success() {
            let pw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !pw.is_empty() {
                return Some(pw);
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        let cred_file = data_dir.join(format!(
            "router_{}.dpapi",
            gateway_ip.replace('.', "_")
        ));
        if !cred_file.exists() {
            return None;
        }
        let ps = format!(
            "$enc = Get-Content -Path '{}' -Raw; $sec = ConvertTo-SecureString $enc; [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))",
            cred_file.to_string_lossy().replace('\'', "''")
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .ok()?;
        if out.status.success() {
            let pw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !pw.is_empty() {
                return Some(pw);
            }
        }
        None
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (data_dir, gateway_ip);
        None
    }
}

pub fn delete_router_password(data_dir: &Path, gateway_ip: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = data_dir;
        let _ = Command::new("security")
            .args([
                "delete-generic-password",
                "-a",
                gateway_ip,
                "-s",
                KEYCHAIN_SERVICE,
            ])
            .output();
    }

    #[cfg(target_os = "windows")]
    {
        let cred_file = data_dir.join(format!(
            "router_{}.dpapi",
            gateway_ip.replace('.', "_")
        ));
        let _ = fs::remove_file(cred_file);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (data_dir, gateway_ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_roundtrip_defaults() {
        let legacy = r#"{"devices":{},"onboarding_done":true}"#;
        let parsed: StoreData = serde_json::from_str(legacy).unwrap();
        assert!(parsed.onboarding_done);
        assert!(parsed.settings.auto_scan_enabled);
        assert_eq!(parsed.settings.scan_interval_secs, 180);
        assert!(parsed.settings.minimize_to_tray);
        assert!(parsed.router_binding.is_none());
    }
}
