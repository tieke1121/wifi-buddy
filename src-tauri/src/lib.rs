mod network;
mod oui;
mod storage;

use network::{HealthReport, NetworkOverview, RawLanDevice, WifiPasswordResult};
use oui::normalize_mac;
use serde::Serialize;
use std::path::PathBuf;
use storage::{load_store, now_unix_secs, save_store, DeviceMeta};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};

struct AppState {
    store_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceView {
    pub ip: String,
    pub mac: String,
    pub display_name: String,
    pub default_name: String,
    pub alias: Option<String>,
    pub vendor: String,
    pub hostname: Option<String>,
    pub device_type: String,
    pub is_self: bool,
    pub is_gateway: bool,
    pub is_randomized_mac: bool,
    pub ping_alive: bool,
    pub latency_ms: Option<u32>,
    pub trusted: bool,
    pub blocked: bool,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResponse {
    pub scanned_at: u64,
    pub total_clients: usize,
    pub new_count: usize,
    pub ping_alive_count: usize,
    pub devices: Vec<DeviceView>,
}

#[tauri::command]
fn get_network_overview() -> NetworkOverview {
    network::get_network_overview()
}

#[tauri::command]
fn scan_devices(state: State<'_, AppState>) -> Result<ScanResponse, String> {
    let raw_list: Vec<RawLanDevice> = network::scan_lan_devices();
    let mut store = load_store(&state.store_path);
    let now = now_unix_secs();

    let mut devices = Vec::with_capacity(raw_list.len());
    for raw in raw_list {
        let mac_key = normalize_mac(&raw.mac).unwrap_or_else(|| raw.mac.clone());
        let existing = store.devices.get(&mac_key).cloned();
        let is_new = match &existing {
            None => !raw.is_self && !raw.is_gateway,
            Some(meta) => !meta.trusted && !raw.is_self && !raw.is_gateway && meta.alias.is_none(),
        };

        let meta = match existing {
            Some(mut m) => {
                m.last_seen = now;
                m
            }
            None => DeviceMeta {
                mac: mac_key.clone(),
                alias: None,
                trusted: raw.is_self || raw.is_gateway,
                blocked: false,
                first_seen: now,
                last_seen: now,
            },
        };
        store.devices.insert(mac_key.clone(), meta.clone());

        let display_name = meta
            .alias
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| raw.default_name.clone());

        devices.push(DeviceView {
            ip: raw.ip,
            mac: mac_key,
            display_name,
            default_name: raw.default_name,
            alias: meta.alias,
            vendor: raw.vendor,
            hostname: raw.hostname,
            device_type: raw.device_type,
            is_self: raw.is_self,
            is_gateway: raw.is_gateway,
            is_randomized_mac: raw.is_randomized_mac,
            ping_alive: raw.ping_alive,
            latency_ms: raw.latency_ms,
            trusted: meta.trusted,
            blocked: meta.blocked,
            is_new,
        });
    }

    let _ = save_store(&state.store_path, &store);

    let total_clients = devices.iter().filter(|d| !d.is_gateway && !d.blocked).count();
    let new_count = devices.iter().filter(|d| d.is_new && !d.blocked).count();
    let ping_alive_count = devices
        .iter()
        .filter(|d| !d.is_gateway && !d.blocked && d.ping_alive)
        .count();

    Ok(ScanResponse {
        scanned_at: now,
        total_clients,
        new_count,
        ping_alive_count,
        devices,
    })
}

#[tauri::command]
fn save_device_meta(
    state: State<'_, AppState>,
    mac: String,
    alias: Option<String>,
    trusted: Option<bool>,
    blocked: Option<bool>,
) -> Result<(), String> {
    let key = normalize_mac(&mac).unwrap_or(mac);
    let mut store = load_store(&state.store_path);
    let now = now_unix_secs();
    let entry = store.devices.entry(key.clone()).or_insert_with(|| DeviceMeta {
        mac: key,
        alias: None,
        trusted: false,
        blocked: false,
        first_seen: now,
        last_seen: now,
    });
    if let Some(a) = alias {
        let trimmed = a.trim().to_string();
        entry.alias = if trimmed.is_empty() { None } else { Some(trimmed) };
        if entry.alias.is_some() {
            entry.trusted = true;
        }
    }
    if let Some(t) = trusted {
        entry.trusted = t;
    }
    if let Some(b) = blocked {
        entry.blocked = b;
    }
    entry.last_seen = now;
    save_store(&state.store_path, &store)
}

#[tauri::command]
fn get_saved_wifi_password(ssid: String) -> WifiPasswordResult {
    network::get_saved_wifi_password(&ssid)
}

#[tauri::command]
fn run_health_check() -> HealthReport {
    network::run_health_check()
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("wifi-buddy"));
            app.manage(AppState {
                store_path: data_dir.join("devices_store.json"),
            });

            if let Ok(show_i) = MenuItem::with_id(app, "show", "打开 Wi-Fi 小管家", true, None::<&str>) {
                if let Ok(quit_i) = MenuItem::with_id(app, "quit", "退出", true, None::<&str>) {
                    if let Ok(menu) = Menu::with_items(app, &[&show_i, &quit_i]) {
                        let mut builder = TrayIconBuilder::new()
                            .menu(&menu)
                            .tooltip("Wi-Fi 小管家 (wifi-buddy)")
                            .on_menu_event(|app, event| match event.id.as_ref() {
                                "show" => show_main_window(app),
                                "quit" => app.exit(0),
                                _ => {}
                            });
                        if let Some(icon) = app.default_window_icon() {
                            builder = builder.icon(icon.clone());
                        }
                        let _ = builder.build(app);
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_network_overview,
            scan_devices,
            save_device_meta,
            get_saved_wifi_password,
            run_health_check
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
