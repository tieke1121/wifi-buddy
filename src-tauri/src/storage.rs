use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreData {
    pub devices: HashMap<String, DeviceMeta>,
    pub onboarding_done: bool,
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
