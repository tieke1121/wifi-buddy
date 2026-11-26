mod autostart;
mod network;
mod oui;
mod router;
mod ssdp;
mod storage;

use network::{HealthReport, NetworkOverview, RawLanDevice, SpeedTestResult, WifiPasswordResult};
use oui::normalize_mac;
use router::{get_adapter_by_id, RouterClientInfo, RouterSession};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use storage::{
    delete_router_password, load_router_password, load_store, now_unix_secs, save_router_password,
    save_store, AppSettings, DeviceMeta, GuestWifiState, NetworkEvent, ParentalRule, RouterBinding,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

struct AppState {
    data_dir: PathBuf,
    store_path: PathBuf,
    router_session: Mutex<Option<RouterSession>>,
    tray_status_item: Mutex<Option<MenuItem<tauri::Wry>>>,
    scan_in_progress: AtomicBool,
    quitting: AtomicBool,
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
    pub brand_new_devices: Vec<DeviceView>,
    pub events: Vec<NetworkEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppPreferences {
    pub onboarding_done: bool,
    pub settings: AppSettings,
    pub events: Vec<NetworkEvent>,
    pub router_binding: Option<RouterBinding>,
    pub has_saved_router_password: bool,
    pub router_connected: bool,
    pub parental_rules: Vec<ParentalRule>,
    pub guest_wifi: GuestWifiState,
    pub autostart_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouterBindResult {
    pub adapter_id: String,
    pub adapter_name: String,
    pub gateway_ip: String,
    pub message: String,
}

fn send_desktop_notification(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            esc(body),
            esc(title)
        );
        let _ = Command::new("osascript").args(["-e", &script]).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let esc = |s: &str| s.replace('\'', "''");
        let ps = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
             $tpl = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $nodes = $tpl.GetElementsByTagName('text'); \
             $nodes.Item(0).AppendChild($tpl.CreateTextNode('{}')) | Out-Null; \
             $nodes.Item(1).AppendChild($tpl.CreateTextNode('{}')) | Out-Null; \
             $toast = [Windows.UI.Notifications.ToastNotification]::new($tpl); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Wi-Fi 小管家').Show($toast)",
            esc(title),
            esc(body)
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .spawn();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (title, body);
    }
}

fn update_tray_indicator(app: &AppHandle, total_clients: usize, new_count: usize) {
    let tip = if new_count > 0 {
        format!(
            "Wi-Fi 小管家 · {} 台在线（{} 台新设备）",
            total_clients, new_count
        )
    } else {
        format!("Wi-Fi 小管家 · {} 台在线", total_clients)
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_tooltip(Some(tip.clone()));
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(guard) = state.tray_status_item.lock() {
            if let Some(item) = guard.as_ref() {
                let label = if new_count > 0 {
                    format!("⚠️ {} 台在线 · {} 台陌生设备", total_clients, new_count)
                } else {
                    format!("● {} 台在线 · 网络正常", total_clients)
                };
                let _ = item.set_text(label);
            }
        }
    }
}

fn perform_scan(app: &AppHandle, notify_if_new: bool) -> Result<ScanResponse, String> {
    let state = app.state::<AppState>();
    if state
        .scan_in_progress
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        // 已有扫描在进行时，稍等后重试一次或返回提示
        for _ in 0..30 {
            thread::sleep(Duration::from_millis(150));
            if !state.scan_in_progress.load(Ordering::SeqCst) {
                break;
            }
        }
        if state
            .scan_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("正在扫描中，请稍候片刻".to_string());
        }
    }

    let result = (|| -> Result<ScanResponse, String> {
        let raw_list: Vec<RawLanDevice> = network::scan_lan_devices();
        let mut store = load_store(&state.store_path);
        let now = now_unix_secs();
        let is_first_ever_scan = store.devices.is_empty();

        let mut devices = Vec::with_capacity(raw_list.len());
        let mut brand_new_devices = Vec::new();

        for raw in raw_list {
            let mac_key = normalize_mac(&raw.mac).unwrap_or_else(|| raw.mac.clone());
            let existing = store.devices.get(&mac_key).cloned();
            let brand_new_to_store = existing.is_none() && !raw.is_self && !raw.is_gateway;

            let is_new = match &existing {
                None => !raw.is_self && !raw.is_gateway,
                Some(meta) => {
                    !meta.trusted && !raw.is_self && !raw.is_gateway && meta.alias.is_none()
                }
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

            let view = DeviceView {
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
            };

            if brand_new_to_store && !is_first_ever_scan {
                brand_new_devices.push(view.clone());
            }
            devices.push(view);
        }

        for new_dev in &brand_new_devices {
            store.events.insert(
                0,
                NetworkEvent {
                    timestamp: now,
                    kind: "new_device".to_string(),
                    title: format!("发现新设备：{}", new_dev.display_name),
                    detail: format!("{} · {} ({})", new_dev.vendor, new_dev.ip, new_dev.mac),
                },
            );
        }
        store.events.truncate(50);

        let notify_enabled = store.settings.notify_new_device;
        let _ = save_store(&state.store_path, &store);

        let total_clients = devices
            .iter()
            .filter(|d| !d.is_gateway && !d.blocked)
            .count();
        let new_count = devices.iter().filter(|d| d.is_new && !d.blocked).count();
        let ping_alive_count = devices
            .iter()
            .filter(|d| !d.is_gateway && !d.blocked && d.ping_alive)
            .count();

        update_tray_indicator(app, total_clients, new_count);

        if notify_if_new && notify_enabled && !brand_new_devices.is_empty() {
            let names: Vec<String> = brand_new_devices
                .iter()
                .take(2)
                .map(|d| d.display_name.clone())
                .collect();
            let summary = if brand_new_devices.len() == 1 {
                format!("「{}」刚刚连入你的 Wi-Fi", names[0])
            } else {
                format!(
                    "「{}」等 {} 台新设备刚刚连入你的 Wi-Fi",
                    names.join("、"),
                    brand_new_devices.len()
                )
            };
            send_desktop_notification("Wi-Fi 小管家 · 发现陌生新设备", &summary);
        }

        Ok(ScanResponse {
            scanned_at: now,
            total_clients,
            new_count,
            ping_alive_count,
            devices,
            brand_new_devices,
            events: store.events,
        })
    })();

    state.scan_in_progress.store(false, Ordering::SeqCst);
    result
}

fn ensure_router_session(state: &State<'_, AppState>) -> Result<RouterSession, String> {
    if let Ok(guard) = state.router_session.lock() {
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }
    }

    let store = load_store(&state.store_path);
    let binding = store
        .router_binding
        .ok_or_else(|| "尚未绑定路由器管理员账号，请先在「路由器与守护」登录绑定".to_string())?;
    let pw = load_router_password(&state.data_dir, &binding.gateway_ip)
        .ok_or_else(|| "未保存路由器密码或登录已过期，请重新输入管理员密码绑定".to_string())?;
    let adapter = get_adapter_by_id(&binding.adapter_id)
        .ok_or_else(|| format!("不支持的路由器适配器: {}", binding.adapter_id))?;
    let session = adapter.authenticate(&binding.gateway_ip, binding.username.as_deref(), &pw)?;
    if let Ok(mut guard) = state.router_session.lock() {
        *guard = Some(session.clone());
    }
    Ok(session)
}

fn append_router_event(state: &State<'_, AppState>, title: String, detail: String) {
    let mut store = load_store(&state.store_path);
    store.events.insert(
        0,
        NetworkEvent {
            timestamp: now_unix_secs(),
            kind: "router".to_string(),
            title,
            detail,
        },
    );
    store.events.truncate(50);
    let _ = save_store(&state.store_path, &store);
}

#[tauri::command]
fn get_network_overview() -> NetworkOverview {
    network::get_network_overview()
}

#[tauri::command]
fn get_app_preferences(state: State<'_, AppState>) -> AppPreferences {
    let store = load_store(&state.store_path);
    let has_saved_router_password = store
        .router_binding
        .as_ref()
        .and_then(|b| load_router_password(&state.data_dir, &b.gateway_ip))
        .is_some();
    let router_connected = state
        .router_session
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false);

    AppPreferences {
        onboarding_done: store.onboarding_done,
        settings: store.settings,
        events: store.events,
        router_binding: store.router_binding,
        has_saved_router_password,
        router_connected,
        parental_rules: store.parental_rules,
        guest_wifi: store.guest_wifi,
        autostart_enabled: autostart::is_autostart_enabled(),
    }
}

#[tauri::command]
fn mark_onboarding_done(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = load_store(&state.store_path);
    store.onboarding_done = true;
    save_store(&state.store_path, &store)
}

#[tauri::command]
fn update_app_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let mut store = load_store(&state.store_path);
    store.settings = settings.clone();
    save_store(&state.store_path, &store)?;
    Ok(settings)
}

#[tauri::command]
fn clear_network_events(state: State<'_, AppState>) -> Result<(), String> {
    let mut store = load_store(&state.store_path);
    store.events.clear();
    save_store(&state.store_path, &store)
}

#[tauri::command]
fn scan_devices(app: AppHandle) -> Result<ScanResponse, String> {
    perform_scan(&app, true)
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
        entry.alias = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
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

#[tauri::command]
fn run_speed_test() -> SpeedTestResult {
    network::run_speed_test()
}

#[tauri::command]
fn bind_router_adapter(
    state: State<'_, AppState>,
    gateway_ip: String,
    adapter_id: String,
    username: Option<String>,
    password: String,
    save_credentials: bool,
) -> Result<RouterBindResult, String> {
    let adapter = get_adapter_by_id(&adapter_id)
        .ok_or_else(|| "当前路由器型号暂未提供控制适配器".to_string())?;
    let session = adapter.authenticate(&gateway_ip, username.as_deref(), &password)?;

    if save_credentials {
        let _ = save_router_password(&state.data_dir, &gateway_ip, &password);
    }

    let mut store = load_store(&state.store_path);
    store.router_binding = Some(RouterBinding {
        gateway_ip: gateway_ip.clone(),
        adapter_id: adapter_id.clone(),
        username: Some(session.username.clone()),
        bound_at: now_unix_secs(),
    });
    let _ = save_store(&state.store_path, &store);

    if let Ok(mut guard) = state.router_session.lock() {
        *guard = Some(session);
    }

    append_router_event(
        &state,
        format!("已连接路由器：{}", adapter.display_name()),
        format!("网关 {} 已验证管理员权限", gateway_ip),
    );

    Ok(RouterBindResult {
        adapter_id,
        adapter_name: adapter.display_name().to_string(),
        gateway_ip,
        message: format!("已成功连接并验证「{}」", adapter.display_name()),
    })
}

#[tauri::command]
fn forget_router_credentials(state: State<'_, AppState>, gateway_ip: String) -> Result<(), String> {
    delete_router_password(&state.data_dir, &gateway_ip);
    if let Ok(mut guard) = state.router_session.lock() {
        *guard = None;
    }
    let mut store = load_store(&state.store_path);
    store.router_binding = None;
    save_store(&state.store_path, &store)
}

#[tauri::command]
fn get_router_clients(state: State<'_, AppState>) -> Result<Vec<RouterClientInfo>, String> {
    let session = ensure_router_session(&state)?;
    let adapter = get_adapter_by_id(&session.adapter_id)
        .ok_or_else(|| "不支持的路由器适配器".to_string())?;
    adapter.list_clients(&session)
}

#[tauri::command]
fn update_router_wifi(
    state: State<'_, AppState>,
    ssid: String,
    password: String,
) -> Result<String, String> {
    let session = ensure_router_session(&state)?;
    let adapter = get_adapter_by_id(&session.adapter_id)
        .ok_or_else(|| "不支持的路由器适配器".to_string())?;
    let msg = adapter.update_wifi_config(&session, ssid.trim(), &password)?;
    append_router_event(
        &state,
        format!("已修改路由器 Wi-Fi：{}", ssid.trim()),
        msg.clone(),
    );
    Ok(msg)
}

#[tauri::command]
fn router_block_device(
    state: State<'_, AppState>,
    mac: String,
    block: bool,
) -> Result<String, String> {
    let session = ensure_router_session(&state)?;
    let adapter = get_adapter_by_id(&session.adapter_id)
        .ok_or_else(|| "不支持的路由器适配器".to_string())?;
    let msg = adapter.block_mac(&session, &mac, block)?;
    append_router_event(
        &state,
        if block {
            format!("已在路由器拉黑设备：{}", mac)
        } else {
            format!("已解除路由器拉黑：{}", mac)
        },
        msg.clone(),
    );
    Ok(msg)
}

#[tauri::command]
fn reboot_router(state: State<'_, AppState>) -> Result<String, String> {
    let session = ensure_router_session(&state)?;
    let adapter = get_adapter_by_id(&session.adapter_id)
        .ok_or_else(|| "不支持的路由器适配器".to_string())?;
    let msg = adapter.reboot(&session)?;
    append_router_event(
        &state,
        "已重启路由器".to_string(),
        format!("向 {} 发送重启指令", session.gateway_ip),
    );
    Ok(msg)
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    autostart::set_autostart_enabled(enabled)
}

#[tauri::command]
fn save_parental_rule(
    state: State<'_, AppState>,
    rule: ParentalRule,
) -> Result<Vec<ParentalRule>, String> {
    let norm_mac = normalize_mac(&rule.mac).unwrap_or_else(|| rule.mac.clone());
    let mut store = load_store(&state.store_path);
    let clean = ParentalRule {
        mac: norm_mac,
        ..rule
    };
    if let Some(pos) = store.parental_rules.iter().position(|r| r.id == clean.id) {
        store.parental_rules[pos] = clean;
    } else {
        store.parental_rules.push(clean);
    }
    save_store(&state.store_path, &store)?;
    Ok(store.parental_rules)
}

#[tauri::command]
fn delete_parental_rule(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<ParentalRule>, String> {
    let mut store = load_store(&state.store_path);
    store.parental_rules.retain(|r| r.id != id);
    save_store(&state.store_path, &store)?;
    Ok(store.parental_rules)
}

#[tauri::command]
fn set_guest_wifi(
    state: State<'_, AppState>,
    enabled: bool,
    ssid: Option<String>,
    password: Option<String>,
    duration_mins: Option<u64>,
) -> Result<GuestWifiState, String> {
    let session = ensure_router_session(&state)?;
    let adapter = get_adapter_by_id(&session.adapter_id)
        .ok_or_else(|| "不支持的路由器适配器".to_string())?;
    let clean_ssid = ssid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let clean_pw = password.as_deref().map(str::trim).map(String::from);
    if enabled {
        if let Some(ref pw) = clean_pw {
            if !pw.is_empty() && pw.len() < 8 {
                return Err("访客 Wi-Fi 如设置密码，至少需要 8 位字符".to_string());
            }
        }
    }

    let msg = adapter.set_guest_wifi(
        &session,
        enabled,
        clean_ssid.as_deref(),
        clean_pw.as_deref(),
    )?;

    let mut store = load_store(&state.store_path);
    let auto_off_at = if enabled {
        duration_mins
            .filter(|m| *m > 0)
            .map(|m| now_unix_secs() + m * 60)
    } else {
        None
    };
    store.guest_wifi = GuestWifiState {
        enabled,
        ssid: clean_ssid.unwrap_or_else(|| store.guest_wifi.ssid.clone()),
        password: clean_pw.unwrap_or_else(|| store.guest_wifi.password.clone()),
        auto_off_at,
    };
    save_store(&state.store_path, &store)?;
    append_router_event(
        &state,
        if enabled {
            format!("已开启访客 Wi-Fi：{}", store.guest_wifi.ssid)
        } else {
            "已关闭访客 Wi-Fi".to_string()
        },
        msg,
    );
    Ok(store.guest_wifi)
}

fn ensure_router_session_from_app(app: &AppHandle) -> Option<RouterSession> {
    let state = app.try_state::<AppState>()?;
    if let Ok(guard) = state.router_session.lock() {
        if let Some(s) = guard.as_ref() {
            return Some(s.clone());
        }
    }
    let store = load_store(&state.store_path);
    let binding = store.router_binding?;
    let pw = load_router_password(&state.data_dir, &binding.gateway_ip)?;
    let adapter = get_adapter_by_id(&binding.adapter_id)?;
    let session = adapter
        .authenticate(&binding.gateway_ip, binding.username.as_deref(), &pw)
        .ok()?;
    if let Ok(mut guard) = state.router_session.lock() {
        *guard = Some(session.clone());
    }
    Some(session)
}

fn local_day_and_hhmm() -> Option<(String, String)> {
    #[cfg(target_os = "windows")]
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-Date -Format 'yyyy-MM-dd HH:mm'"])
        .output()
        .ok()?;
    #[cfg(not(target_os = "windows"))]
    let out = Command::new("date").args(["+%Y-%m-%d %H:%M"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let (day, hhmm) = text.split_once(' ')?;
    Some((day.to_string(), hhmm.to_string()))
}

fn hhmm_to_mins(hhmm: &str) -> Option<u32> {
    let (h, m) = hhmm.trim().split_once(':')?;
    let h_val = h.parse::<u32>().ok()?;
    let m_val = m.parse::<u32>().ok()?;
    if h_val >= 24 || m_val >= 60 {
        return None;
    }
    Some(h_val * 60 + m_val)
}

pub fn is_in_time_window(now_hhmm: &str, start_hhmm: &str, end_hhmm: &str) -> bool {
    let (Some(now), Some(start), Some(end)) = (
        hhmm_to_mins(now_hhmm),
        hhmm_to_mins(start_hhmm),
        hhmm_to_mins(end_hhmm),
    ) else {
        return false;
    };
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        // 跨午夜时段，例如 23:00 -> 07:00
        now >= start || now < end
    }
}

fn process_guardian_schedules(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut store = load_store(&state.store_path);
    let mut dirty = false;
    let now = now_unix_secs();

    // 1. 访客 Wi-Fi 定时自动关闭
    if store.guest_wifi.enabled {
        if let Some(off_at) = store.guest_wifi.auto_off_at {
            if now >= off_at {
                if let Some(session) = ensure_router_session_from_app(app) {
                    if let Some(adapter) = get_adapter_by_id(&session.adapter_id) {
                        if adapter.set_guest_wifi(&session, false, None, None).is_ok() {
                            store.guest_wifi.enabled = false;
                            store.guest_wifi.auto_off_at = None;
                            store.events.insert(
                                0,
                                NetworkEvent {
                                    timestamp: now,
                                    kind: "router".to_string(),
                                    title: "访客 Wi-Fi 已到期自动关闭".to_string(),
                                    detail: format!("网络 {}", store.guest_wifi.ssid),
                                },
                            );
                            send_desktop_notification("Wi-Fi 小管家", "临时访客 Wi-Fi 已到时间自动关闭");
                            dirty = true;
                        }
                    }
                }
            }
        }
    }

    // 2. 夜间/家长断网规则检查
    if !store.parental_rules.is_empty() {
        if let Some((day, hhmm)) = local_day_and_hhmm() {
            let session_opt = ensure_router_session_from_app(app);
            let adapter_opt = session_opt
                .as_ref()
                .and_then(|s| get_adapter_by_id(&s.adapter_id));
            let can_block = adapter_opt
                .as_ref()
                .map(|a| a.capabilities().supports_block_mac)
                .unwrap_or(false);

            for rule in &mut store.parental_rules {
                if !rule.enabled {
                    continue;
                }
                let active = is_in_time_window(&hhmm, &rule.start_hhmm, &rule.end_hhmm);
                if active && !rule.currently_enforced {
                    if can_block {
                        if let (Some(session), Some(adapter)) = (&session_opt, &adapter_opt) {
                            if adapter.block_mac(session, &rule.mac, true).is_ok() {
                                rule.currently_enforced = true;
                                store.events.insert(
                                    0,
                                    NetworkEvent {
                                        timestamp: now,
                                        kind: "router".to_string(),
                                        title: format!("夜间守护：已暂停「{}」联网", rule.device_name),
                                        detail: format!("时段 {} ~ {}", rule.start_hhmm, rule.end_hhmm),
                                    },
                                );
                                dirty = true;
                            }
                        }
                    } else if rule.last_notified_day.as_deref() != Some(&day) {
                        rule.last_notified_day = Some(day.clone());
                        send_desktop_notification(
                            "Wi-Fi 小管家 · 夜间守护提醒",
                            &format!("已到「{}」的休息时段（{}），请留意设备用网", rule.device_name, rule.start_hhmm),
                        );
                        dirty = true;
                    }
                } else if !active && rule.currently_enforced && can_block {
                    if let (Some(session), Some(adapter)) = (&session_opt, &adapter_opt) {
                        if adapter.block_mac(session, &rule.mac, false).is_ok() {
                            rule.currently_enforced = false;
                            store.events.insert(
                                0,
                                NetworkEvent {
                                    timestamp: now,
                                    kind: "router".to_string(),
                                    title: format!("夜间守护结束：已恢复「{}」联网", rule.device_name),
                                    detail: format!("恢复时间 {}", hhmm),
                                },
                            );
                            dirty = true;
                        }
                    }
                }
            }
        }
    }

    if dirty {
        store.events.truncate(50);
        let _ = save_store(&state.store_path, &store);
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
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
            let store_path = data_dir.join("devices_store.json");

            let status_item =
                MenuItem::with_id(app, "status", "● Wi-Fi 小管家守护中", false, None::<&str>).ok();
            let show_i = MenuItem::with_id(app, "show", "打开 Wi-Fi 小管家", true, None::<&str>).ok();
            let scan_i = MenuItem::with_id(app, "scan_now", "立即全网巡检", true, None::<&str>).ok();
            let quit_i = MenuItem::with_id(app, "quit", "退出 Wi-Fi 小管家", true, None::<&str>).ok();

            if let (Some(st), Some(sh), Some(sc), Some(qt)) =
                (&status_item, &show_i, &scan_i, &quit_i)
            {
                if let Ok(menu) = Menu::with_items(app, &[st, sh, sc, qt]) {
                    let mut builder = TrayIconBuilder::with_id("main-tray")
                        .menu(&menu)
                        .tooltip("Wi-Fi 小管家 (wifi-buddy)")
                        .on_tray_icon_event(|tray, event| {
                            if let TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } = event
                            {
                                show_main_window(tray.app_handle());
                            }
                        })
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "show" => show_main_window(app),
                            "scan_now" => {
                                let handle = app.clone();
                                thread::spawn(move || {
                                    if let Ok(res) = perform_scan(&handle, true) {
                                        let _ = handle.emit("lan-scan-completed", res);
                                    }
                                });
                            }
                            "quit" => {
                                if let Some(s) = app.try_state::<AppState>() {
                                    s.quitting.store(true, Ordering::SeqCst);
                                }
                                app.exit(0);
                            }
                            _ => {}
                        });
                    if let Some(icon) = app.default_window_icon() {
                        builder = builder.icon(icon.clone());
                    }
                    let _ = builder.build(app);
                }
            }

            app.manage(AppState {
                data_dir,
                store_path: store_path.clone(),
                router_session: Mutex::new(None),
                tray_status_item: Mutex::new(status_item),
                scan_in_progress: AtomicBool::new(false),
                quitting: AtomicBool::new(false),
            });

            // 后台常驻静默巡检 & 家长/访客守护调度线程
            let bg_handle = app.handle().clone();
            thread::spawn(move || {
                let mut last_auto_scan = now_unix_secs();
                let mut last_schedule_check = 0u64;
                loop {
                    thread::sleep(Duration::from_secs(10));
                    let Some(state) = bg_handle.try_state::<AppState>() else {
                        break;
                    };
                    if state.quitting.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = now_unix_secs();
                    if now.saturating_sub(last_schedule_check) >= 30 {
                        last_schedule_check = now;
                        process_guardian_schedules(&bg_handle);
                    }
                    let store = load_store(&state.store_path);
                    if !store.settings.auto_scan_enabled {
                        continue;
                    }
                    let interval = store.settings.scan_interval_secs.max(30);
                    if now.saturating_sub(last_auto_scan) >= interval {
                        last_auto_scan = now;
                        if let Ok(res) = perform_scan(&bg_handle, true) {
                            let _ = bg_handle.emit("lan-scan-completed", res);
                        }
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                if let Some(state) = app.try_state::<AppState>() {
                    if state.quitting.load(Ordering::SeqCst) {
                        return;
                    }
                    let store = load_store(&state.store_path);
                    if store.settings.minimize_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_network_overview,
            get_app_preferences,
            mark_onboarding_done,
            update_app_settings,
            clear_network_events,
            scan_devices,
            save_device_meta,
            get_saved_wifi_password,
            run_health_check,
            run_speed_test,
            bind_router_adapter,
            forget_router_credentials,
            get_router_clients,
            update_router_wifi,
            router_block_device,
            reboot_router,
            set_autostart,
            save_parental_rule,
            delete_parental_rule,
            set_guest_wifi
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_time_window() {
        // 跨午夜时段 23:00 ~ 07:00
        assert!(is_in_time_window("23:30", "23:00", "07:00"));
        assert!(is_in_time_window("02:15", "23:00", "07:00"));
        assert!(!is_in_time_window("12:00", "23:00", "07:00"));
        assert!(!is_in_time_window("07:00", "23:00", "07:00"));
        // 日间时段 14:00 ~ 16:00
        assert!(is_in_time_window("14:30", "14:00", "16:00"));
        assert!(!is_in_time_window("16:01", "14:00", "16:00"));
    }
}
