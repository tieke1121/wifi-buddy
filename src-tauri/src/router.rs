use crate::oui::normalize_mac;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouterCapabilities {
    pub supports_clients: bool,
    pub supports_wifi_config: bool,
    pub supports_block_mac: bool,
    #[serde(default)]
    pub supports_guest_wifi: bool,
    pub supports_reboot: bool,
    pub default_username: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouterClientInfo {
    pub ip: Option<String>,
    pub mac: String,
    pub name: Option<String>,
    pub band: Option<String>,   // "2.4G" | "5G" | "有线"
    pub signal_dbm: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct RouterProbeInfo<'a> {
    #[allow(dead_code)]
    pub gateway_ip: &'a str,
    pub mac_vendor: Option<&'a str>,
    pub http_status_and_headers: &'a str,
    pub http_body: &'a str,
    pub page_title: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct RouterSession {
    pub gateway_ip: String,
    pub adapter_id: String,
    pub auth_token: String,
    pub cookie: Option<String>,
    pub username: String,
    pub mode: String, // "rpc" | "luci_web" | "zte_gch" | "zte_api"
}

pub trait RouterAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> RouterCapabilities;
    fn matches(&self, info: &RouterProbeInfo) -> bool;
    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String>;
    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String>;
    fn update_wifi_config(
        &self,
        session: &RouterSession,
        ssid: &str,
        password: &str,
    ) -> Result<String, String>;
    fn block_mac(
        &self,
        session: &RouterSession,
        mac: &str,
        block: bool,
    ) -> Result<String, String>;
    fn set_guest_wifi(
        &self,
        _session: &RouterSession,
        _enabled: bool,
        _ssid: Option<&str>,
        _password: Option<&str>,
    ) -> Result<String, String> {
        Err("当前路由器型号暂不支持通过接口开关访客 Wi-Fi，请前往网页后台操作。".to_string())
    }
    fn reboot(&self, session: &RouterSession) -> Result<String, String>;
}

pub struct OpenWrtAdapter;
pub struct ZteAdapter;
pub struct XiaomiAdapter;
pub struct TpLinkAdapter;
pub struct HuaweiAdapter;

pub fn get_adapter_by_id(id: &str) -> Option<Box<dyn RouterAdapter>> {
    match id {
        "openwrt" => Some(Box::new(OpenWrtAdapter)),
        "zte" => Some(Box::new(ZteAdapter)),
        "xiaomi" => Some(Box::new(XiaomiAdapter)),
        "tplink" => Some(Box::new(TpLinkAdapter)),
        "huawei" => Some(Box::new(HuaweiAdapter)),
        _ => None,
    }
}

pub fn match_router_adapter(info: &RouterProbeInfo) -> Option<Box<dyn RouterAdapter>> {
    let adapters: Vec<Box<dyn RouterAdapter>> = vec![
        Box::new(OpenWrtAdapter),
        Box::new(XiaomiAdapter),
        Box::new(TpLinkAdapter),
        Box::new(HuaweiAdapter),
        Box::new(ZteAdapter),
    ];
    for adapter in adapters {
        if adapter.matches(info) {
            return Some(adapter);
        }
    }
    None
}

#[derive(Debug)]
struct HttpResponse {
    status_code: u16,
    #[allow(dead_code)]
    headers: HashMap<String, String>,
    set_cookies: Vec<String>,
    body: String,
}

fn http_request(
    host: &str,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    cookie: Option<&str>,
    body: Option<&str>,
) -> Result<HttpResponse, String> {
    let addr_str = if host.contains(':') {
        host.to_string()
    } else {
        format!("{}:80", host)
    };
    let addr: SocketAddr = addr_str
        .parse()
        .map_err(|_| format!("无效的路由器地址: {}", host))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(1600))
        .map_err(|_| "无法连接路由器管理端口（请确认网络已连接）".to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(2200)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1600)));

    let body_bytes = body.unwrap_or("").as_bytes();
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: wifi-buddy/0.2\r\n",
        method, path, host
    );
    if let Some(ct) = content_type {
        req.push_str(&format!("Content-Type: {}\r\n", ct));
    }
    if let Some(ck) = cookie {
        if !ck.is_empty() {
            req.push_str(&format!("Cookie: {}\r\n", ck));
        }
    }
    if body.is_some() {
        req.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    req.push_str("\r\n");

    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("发送请求到路由器失败: {}", e))?;
    if !body_bytes.is_empty() {
        stream
            .write_all(body_bytes)
            .map_err(|e| format!("发送数据到路由器失败: {}", e))?;
    }

    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut total = 0usize;
    while total < 262_144 {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&chunk[..n]);
                total += n;
            }
            Err(_) => break,
        }
    }

    let raw_str = String::from_utf8_lossy(&raw).to_string();
    let (head, resp_body) = match raw_str.split_once("\r\n\r\n") {
        Some((h, b)) => (h, b.to_string()),
        None => match raw_str.split_once("\n\n") {
            Some((h, b)) => (h, b.to_string()),
            None => (raw_str.as_str(), String::new()),
        },
    };

    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    let mut headers = HashMap::new();
    let mut set_cookies = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let kl = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if kl == "set-cookie" {
                if let Some((kv, _)) = val.split_once(';') {
                    set_cookies.push(kv.trim().to_string());
                } else {
                    set_cookies.push(val.clone());
                }
            }
            headers.insert(kl, val);
        }
    }

    Ok(HttpResponse {
        status_code,
        headers,
        set_cookies,
        body: resp_body,
    })
}

fn form_urlencode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", pct_encode(k), pct_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ============================================================================
// OpenWrt LuCI 适配器（支持 LuCI JSON-RPC 与标准 LuCI Web UI）
// ============================================================================

impl OpenWrtAdapter {
    fn rpc_call(
        &self,
        gateway_ip: &str,
        endpoint: &str,
        token: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let path = if token.is_empty() {
            format!("/cgi-bin/luci/rpc/{}", endpoint)
        } else {
            format!("/cgi-bin/luci/rpc/{}?auth={}", endpoint, pct_encode(token))
        };
        let payload = serde_json::json!({
            "id": 1,
            "method": method,
            "params": params
        })
        .to_string();

        let resp = http_request(
            gateway_ip,
            "POST",
            &path,
            Some("application/json"),
            None,
            Some(&payload),
        )?;

        if resp.status_code == 404 || resp.status_code == 403 {
            return Err(format!("HTTP {}", resp.status_code));
        }

        let parsed: Value = serde_json::from_str(&resp.body)
            .map_err(|_| "路由器未启用 LuCI JSON-RPC 接口".to_string())?;
        if let Some(err) = parsed.get("error").filter(|v| !v.is_null()) {
            return Err(format!("路由器接口返回错误: {}", err));
        }
        Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
    }
}

impl RouterAdapter for OpenWrtAdapter {
    fn id(&self) -> &'static str {
        "openwrt"
    }

    fn display_name(&self) -> &'static str {
        "OpenWrt (LuCI)"
    }

    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            supports_clients: true,
            supports_wifi_config: true,
            supports_block_mac: true,
            supports_guest_wifi: true,
            supports_reboot: true,
            default_username: Some("root".to_string()),
            note: "已接入 OpenWrt LuCI 适配器：支持查看无线与 DHCP 终端、修改 Wi-Fi 密码、访客网络、拉黑设备及重启（写操作需路由器已安装 luci-mod-rpc）。".to_string(),
        }
    }

    fn matches(&self, info: &RouterProbeInfo) -> bool {
        let combined = format!(
            "{} {} {}",
            info.http_status_and_headers,
            info.http_body,
            info.page_title.unwrap_or("")
        )
        .to_lowercase();
        combined.contains("luci") || combined.contains("openwrt")
    }

    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String> {
        let user = username
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("root");

        // 1. 优先尝试 LuCI JSON-RPC (`/cgi-bin/luci/rpc/auth`)
        if let Ok(result) = self.rpc_call(
            gateway_ip,
            "auth",
            "",
            "login",
            serde_json::json!([user, password]),
        ) {
            if let Some(token) = result.as_str().filter(|s| !s.is_empty()) {
                return Ok(RouterSession {
                    gateway_ip: gateway_ip.to_string(),
                    adapter_id: "openwrt".to_string(),
                    auth_token: token.to_string(),
                    cookie: None,
                    username: user.to_string(),
                    mode: "rpc".to_string(),
                });
            }
            return Err("OpenWrt 管理员账号或密码不正确".to_string());
        }

        // 2. 降级为标准 LuCI Web 表单登录（sysauth Cookie）
        let form = form_urlencode(&[("luci_username", user), ("luci_password", password)]);
        let resp = http_request(
            gateway_ip,
            "POST",
            "/cgi-bin/luci/",
            Some("application/x-www-form-urlencoded"),
            None,
            Some(&form),
        )?;

        let cookie_str = resp
            .set_cookies
            .iter()
            .find(|c| c.starts_with("sysauth"))
            .cloned();

        if let Some(ck) = cookie_str {
            if !ck.ends_with('=') && (resp.status_code == 302 || resp.status_code == 200) {
                return Ok(RouterSession {
                    gateway_ip: gateway_ip.to_string(),
                    adapter_id: "openwrt".to_string(),
                    auth_token: String::new(),
                    cookie: Some(ck),
                    username: user.to_string(),
                    mode: "luci_web".to_string(),
                });
            }
        }

        Err("OpenWrt 登录失败：请核对管理员密码（默认账号通常为 root）".to_string())
    }

    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String> {
        if session.mode == "rpc" {
            if let Ok(val) = self.rpc_call(
                &session.gateway_ip,
                "sys",
                &session.auth_token,
                "net.arptable",
                serde_json::json!([]),
            ) {
                if let Some(arr) = val.as_array() {
                    let clients = parse_openwrt_rpc_clients(arr);
                    if !clients.is_empty() {
                        return Ok(clients);
                    }
                }
            }
        }

        // LuCI Web 状态接口 (`/cgi-bin/luci/admin/status/overview?status=1`)
        let resp = http_request(
            &session.gateway_ip,
            "GET",
            "/cgi-bin/luci/admin/status/overview?status=1",
            None,
            session.cookie.as_deref(),
            None,
        )?;
        if resp.status_code == 200 {
            if let Ok(json) = serde_json::from_str::<Value>(&resp.body) {
                return Ok(parse_openwrt_status_json(&json));
            }
        }

        Err("暂未读取到 OpenWrt 在线终端列表（可能需要安装 luci-mod-rpc）".to_string())
    }

    fn update_wifi_config(
        &self,
        session: &RouterSession,
        ssid: &str,
        password: &str,
    ) -> Result<String, String> {
        if password.len() < 8 {
            return Err("Wi-Fi 密码至少需要 8 位字符".to_string());
        }
        if session.mode != "rpc" {
            return Err("当前 OpenWrt 未开启 luci-mod-rpc 接口，请点击「在浏览器打开后台」手动保存新密码。".to_string());
        }

        // 查找 wireless wifi-iface 并更新 ssid / key
        let wireless = self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "get_all",
            serde_json::json!(["wireless"]),
        )?;

        let mut updated = 0usize;
        if let Some(obj) = wireless.as_object() {
            for (section, cfg) in obj {
                if cfg.get(".type").and_then(|v| v.as_str()) == Some("wifi-iface") {
                    let mode = cfg.get("mode").and_then(|v| v.as_str()).unwrap_or("ap");
                    if mode == "ap" {
                        let _ = self.rpc_call(
                            &session.gateway_ip,
                            "uci",
                            &session.auth_token,
                            "set",
                            serde_json::json!(["wireless", section, "ssid", ssid]),
                        )?;
                        let _ = self.rpc_call(
                            &session.gateway_ip,
                            "uci",
                            &session.auth_token,
                            "set",
                            serde_json::json!(["wireless", section, "key", password]),
                        )?;
                        updated += 1;
                    }
                }
            }
        }

        if updated == 0 {
            return Err("未在 OpenWrt 配置中找到可修改的 AP 无线接口".to_string());
        }

        self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "commit",
            serde_json::json!(["wireless"]),
        )?;
        let _ = self.rpc_call(
            &session.gateway_ip,
            "sys",
            &session.auth_token,
            "exec",
            serde_json::json!(["wifi reload"]),
        );

        Ok(format!(
            "已成功更新 OpenWrt {} 个无线频段的名称与密码，无线正在重启生效",
            updated
        ))
    }

    fn block_mac(
        &self,
        session: &RouterSession,
        mac: &str,
        block: bool,
    ) -> Result<String, String> {
        let norm = normalize_mac(mac).ok_or_else(|| "无效的 MAC 地址".to_string())?;
        if session.mode != "rpc" {
            return Err("拉黑终端需要 OpenWrt 安装 luci-mod-rpc，或请在网页后台「无线黑名单」中操作。".to_string());
        }

        let wireless = self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "get_all",
            serde_json::json!(["wireless"]),
        )?;

        let mut changed = 0usize;
        if let Some(obj) = wireless.as_object() {
            for (section, cfg) in obj {
                if cfg.get(".type").and_then(|v| v.as_str()) == Some("wifi-iface")
                    && cfg.get("mode").and_then(|v| v.as_str()).unwrap_or("ap") == "ap"
                {
                    let mut maclist: Vec<String> = cfg
                        .get("maclist")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|x| x.as_str().and_then(normalize_mac))
                                .collect()
                        })
                        .unwrap_or_default();

                    if block {
                        if !maclist.contains(&norm) {
                            maclist.push(norm.clone());
                        }
                        let _ = self.rpc_call(
                            &session.gateway_ip,
                            "uci",
                            &session.auth_token,
                            "set",
                            serde_json::json!(["wireless", section, "macfilter", "deny"]),
                        );
                    } else {
                        maclist.retain(|m| m != &norm);
                    }

                    let _ = self.rpc_call(
                        &session.gateway_ip,
                        "uci",
                        &session.auth_token,
                        "set",
                        serde_json::json!(["wireless", section, "maclist", maclist]),
                    );
                    changed += 1;
                }
            }
        }

        if changed == 0 {
            return Err("未找到无线接口配置".to_string());
        }
        self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "commit",
            serde_json::json!(["wireless"]),
        )?;
        let _ = self.rpc_call(
            &session.gateway_ip,
            "sys",
            &session.auth_token,
            "exec",
            serde_json::json!(["wifi reload"]),
        );

        Ok(if block {
            format!("已将 {} 加入 OpenWrt 无线黑名单", norm)
        } else {
            format!("已将 {} 移出 OpenWrt 无线黑名单", norm)
        })
    }

    fn set_guest_wifi(
        &self,
        session: &RouterSession,
        enabled: bool,
        ssid: Option<&str>,
        password: Option<&str>,
    ) -> Result<String, String> {
        if session.mode != "rpc" {
            return Err("开关访客网络需要 OpenWrt 安装 luci-mod-rpc，或请在网页后台操作。".to_string());
        }
        let wireless = self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "get_all",
            serde_json::json!(["wireless"]),
        )?;
        let mut guest_section: Option<String> = None;
        if let Some(obj) = wireless.as_object() {
            for (section, cfg) in obj {
                if cfg.get(".type").and_then(|v| v.as_str()) == Some("wifi-iface") {
                    let net = cfg.get("network").and_then(|v| v.as_str()).unwrap_or("");
                    let s_name = cfg.get("ssid").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
                    if section.to_lowercase().contains("guest") || net == "guest" || s_name.contains("guest") {
                        guest_section = Some(section.clone());
                        break;
                    }
                }
            }
        }
        let Some(sec) = guest_section else {
            return Err("未在 OpenWrt 无线配置中找到 guest 访客接口，请先在后台添加一个 guest 网络接口。".to_string());
        };

        let _ = self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "set",
            serde_json::json!(["wireless", sec, "disabled", if enabled { "0" } else { "1" }]),
        )?;
        if enabled {
            if let Some(s) = ssid.filter(|v| !v.trim().is_empty()) {
                let _ = self.rpc_call(
                    &session.gateway_ip,
                    "uci",
                    &session.auth_token,
                    "set",
                    serde_json::json!(["wireless", sec, "ssid", s.trim()]),
                )?;
            }
            if let Some(pw) = password.filter(|v| v.len() >= 8) {
                let _ = self.rpc_call(
                    &session.gateway_ip,
                    "uci",
                    &session.auth_token,
                    "set",
                    serde_json::json!(["wireless", sec, "key", pw]),
                )?;
            }
        }
        self.rpc_call(
            &session.gateway_ip,
            "uci",
            &session.auth_token,
            "commit",
            serde_json::json!(["wireless"]),
        )?;
        let _ = self.rpc_call(
            &session.gateway_ip,
            "sys",
            &session.auth_token,
            "exec",
            serde_json::json!(["wifi reload"]),
        );
        Ok(if enabled {
            "已开启 OpenWrt 访客 Wi-Fi".to_string()
        } else {
            "已关闭 OpenWrt 访客 Wi-Fi".to_string()
        })
    }

    fn reboot(&self, session: &RouterSession) -> Result<String, String> {
        if session.mode != "rpc" {
            return Err("当前 OpenWrt 未开启 luci-mod-rpc，请前往网页后台执行重启。".to_string());
        }
        self.rpc_call(
            &session.gateway_ip,
            "sys",
            &session.auth_token,
            "reboot",
            serde_json::json!([]),
        )?;
        Ok("已向 OpenWrt 发送重启指令，约 1~2 分钟后恢复联网".to_string())
    }
}

pub fn parse_openwrt_rpc_clients(arr: &[Value]) -> Vec<RouterClientInfo> {
    let mut out = Vec::new();
    for item in arr {
        let mac_raw = item
            .get("HW address")
            .or_else(|| item.get("mac"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(mac) = normalize_mac(mac_raw) {
            if mac == "00:00:00:00:00:00" {
                continue;
            }
            let ip = item
                .get("IP address")
                .or_else(|| item.get("ip"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            out.push(RouterClientInfo {
                ip,
                mac,
                name: None,
                band: None,
                signal_dbm: None,
            });
        }
    }
    out
}

pub fn parse_openwrt_status_json(json: &Value) -> Vec<RouterClientInfo> {
    let mut by_mac: HashMap<String, RouterClientInfo> = HashMap::new();

    if let Some(leases) = json.get("leases").and_then(|v| v.as_array()) {
        for l in leases {
            if let Some(mac) = l.get("macaddr").and_then(|v| v.as_str()).and_then(normalize_mac) {
                let ip = l.get("ipaddr").and_then(|v| v.as_str()).map(String::from);
                let name = l
                    .get("hostname")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty() && *s != "*")
                    .map(String::from);
                by_mac.insert(
                    mac.clone(),
                    RouterClientInfo {
                        ip,
                        mac,
                        name,
                        band: Some("有线/DHCP".to_string()),
                        signal_dbm: None,
                    },
                );
            }
        }
    }

    if let Some(wifinets) = json.get("wifinets").and_then(|v| v.as_array()) {
        for radio in wifinets {
            let radio_name = radio
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let band_guess = if radio_name.contains("5") || radio_name.contains("radio1") {
                "5G"
            } else {
                "2.4G"
            };
            if let Some(ifaces) = radio.get("networks").and_then(|v| v.as_array()) {
                for iface in ifaces {
                    let freq = iface.get("frequency").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let band = if freq > 4.0 { "5G" } else { band_guess };
                    if let Some(assoc) = iface.get("assoclist").and_then(|v| v.as_object()) {
                        for (mac_raw, info) in assoc {
                            if let Some(mac) = normalize_mac(mac_raw) {
                                let signal_dbm = info.get("signal").and_then(|v| v.as_i64()).map(|s| s as i32);
                                let entry = by_mac.entry(mac.clone()).or_insert_with(|| RouterClientInfo {
                                    ip: None,
                                    mac,
                                    name: None,
                                    band: None,
                                    signal_dbm: None,
                                });
                                entry.band = Some(band.to_string());
                                entry.signal_dbm = signal_dbm;
                            }
                        }
                    }
                }
            }
        }
    }

    by_mac.into_values().collect()
}

// ============================================================================
// 中兴 ZTE 适配器（zte.home 家用路由器 & 中兴光猫识别与控制）
// ============================================================================

impl ZteAdapter {
    fn extract_gch_token(body: &str) -> Option<String> {
        for marker in ["Frm_Logintoken", "_sessionTOKEN", "loginToken"] {
            if let Some(idx) = body.find(marker) {
                let slice = &body[idx..body.len().min(idx + 160)];
                if let Some(eq) = slice.find("value=") {
                    let after = slice[eq + 6..].trim_start_matches(['"', '\'', ' ']);
                    let token: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                        .collect();
                    if !token.is_empty() {
                        return Some(token);
                    }
                }
            }
        }
        None
    }

    fn is_isp_locked_modem(body: &str) -> bool {
        let low = body.to_lowercase();
        low.contains("chinanet")
            || low.contains("cmcc")
            || low.contains("chinaunicom")
            || body.contains("中国电信")
            || body.contains("中国移动")
            || body.contains("中国联通")
            || body.contains("天翼网关")
    }
}

impl RouterAdapter for ZteAdapter {
    fn id(&self) -> &'static str {
        "zte"
    }

    fn display_name(&self) -> &'static str {
        "中兴 (ZTE / zte.home)"
    }

    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            supports_clients: true,
            supports_wifi_config: true,
            supports_block_mac: false,
            supports_guest_wifi: false,
            supports_reboot: true,
            default_username: Some("admin".to_string()),
            note: "已接入中兴 zte.home 适配器：支持通用中兴管理页登录、读取已连终端、改密及重启；若为运营商定制光猫，受限于电信/移动/联通锁权，部分修改将自动切换为网页指引。".to_string(),
        }
    }

    fn matches(&self, info: &RouterProbeInfo) -> bool {
        let combined = format!(
            "{} {} {}",
            info.http_status_and_headers,
            info.http_body,
            info.page_title.unwrap_or("")
        )
        .to_lowercase();
        info.mac_vendor == Some("中兴 ZTE")
            || combined.contains("zte")
            || combined.contains("zxhn")
            || combined.contains("getpage.gch")
            || combined.contains("common_page/login.gch")
    }

    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String> {
        let user = username
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("admin");

        // 1. 检测首页特征（判断 .gch 传统固件 vs 现代 JSON API 固件）
        let home = http_request(gateway_ip, "GET", "/", None, None, None)?;

        // 尝试现代中兴路由器 JSON API (`/api/v1/login` 或 `/goform/goform_set_cmd_process`)
        let json_login = serde_json::json!({
            "username": user,
            "password": password,
            "method": "login"
        })
        .to_string();
        if let Ok(resp) = http_request(
            gateway_ip,
            "POST",
            "/api/v1/login",
            Some("application/json"),
            None,
            Some(&json_login),
        ) {
            if resp.status_code == 200
                && (resp.body.contains("success")
                    || resp.body.contains("token")
                    || !resp.set_cookies.is_empty())
            {
                let token = serde_json::from_str::<Value>(&resp.body)
                    .ok()
                    .and_then(|v| {
                        v.get("token")
                            .or_else(|| v.pointer("/data/token"))
                            .and_then(|t| t.as_str())
                            .map(String::from)
                    })
                    .unwrap_or_default();
                return Ok(RouterSession {
                    gateway_ip: gateway_ip.to_string(),
                    adapter_id: "zte".to_string(),
                    auth_token: token,
                    cookie: resp.set_cookies.first().cloned(),
                    username: user.to_string(),
                    mode: "zte_api".to_string(),
                });
            }
        }

        // 2. 尝试中兴经典 .gch 固件登录 (`/common_page/login.gch` 或 `/`)
        let login_page = http_request(gateway_ip, "GET", "/common_page/login.gch", None, None, None)
            .unwrap_or(home);
        let token = Self::extract_gch_token(&login_page.body).unwrap_or_default();

        let mut fields = vec![
            ("action", "login"),
            ("Username", user),
            ("Password", password),
        ];
        if !token.is_empty() {
            fields.push(("Frm_Logintoken", token.as_str()));
        }
        let form = form_urlencode(&fields);
        let post_path = if login_page.body.contains("common_page/login.gch") {
            "/common_page/login.gch"
        } else {
            "/"
        };

        let resp = http_request(
            gateway_ip,
            "POST",
            post_path,
            Some("application/x-www-form-urlencoded"),
            login_page.set_cookies.first().map(|s| s.as_str()),
            Some(&form),
        )?;

        let has_session_cookie = resp
            .set_cookies
            .iter()
            .any(|c| c.contains("SID") || c.contains("SESSION") || c.contains("sysauth"));
        let login_ok = has_session_cookie
            || (resp.status_code == 302 && !resp.body.contains("Password"))
            || (resp.status_code == 200 && resp.body.contains("top_menu"));

        if login_ok {
            return Ok(RouterSession {
                gateway_ip: gateway_ip.to_string(),
                adapter_id: "zte".to_string(),
                auth_token: token,
                cookie: resp
                    .set_cookies
                    .first()
                    .cloned()
                    .or_else(|| login_page.set_cookies.first().cloned()),
                username: user.to_string(),
                mode: "zte_gch".to_string(),
            });
        }

        if Self::is_isp_locked_modem(&login_page.body) {
            return Err("检测到这是运营商定制光猫，默认账号可能受限或需要光猫背面贴纸的专属密码。你仍可使用「打开网页后台」或直接用本应用生成扫码卡片。".to_string());
        }

        Err("中兴路由器登录未通过：请核对路由器背面贴纸上的管理账号和密码".to_string())
    }

    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String> {
        if session.mode == "zte_api" {
            if let Ok(resp) = http_request(
                &session.gateway_ip,
                "GET",
                "/api/v1/clients",
                None,
                session.cookie.as_deref(),
                None,
            ) {
                if resp.status_code == 200 {
                    if let Ok(json) = serde_json::from_str::<Value>(&resp.body) {
                        let list = parse_zte_clients_json(&json);
                        if !list.is_empty() {
                            return Ok(list);
                        }
                    }
                }
            }
        }

        // .gch 经典页面提取关联无线客户端
        if let Ok(resp) = http_request(
            &session.gateway_ip,
            "GET",
            "/getpage.gch?pid=1002&nextpage=net_wlan_assoc_t.gch",
            None,
            session.cookie.as_deref(),
            None,
        ) {
            if resp.status_code == 200 {
                let list = parse_zte_gch_clients(&resp.body);
                if !list.is_empty() {
                    return Ok(list);
                }
            }
        }

        Err("未从当前中兴固件读取到独立无线终端列表（可能为运营商精简版界面），请以「谁在用我的网」扫描结果为准。".to_string())
    }

    fn update_wifi_config(
        &self,
        session: &RouterSession,
        ssid: &str,
        password: &str,
    ) -> Result<String, String> {
        if password.len() < 8 {
            return Err("Wi-Fi 密码至少需要 8 位字符".to_string());
        }

        if session.mode == "zte_api" {
            let payload = serde_json::json!({
                "ssid": ssid,
                "password": password,
            })
            .to_string();
            let resp = http_request(
                &session.gateway_ip,
                "POST",
                "/api/v1/wifi",
                Some("application/json"),
                session.cookie.as_deref(),
                Some(&payload),
            )?;
            if resp.status_code == 200 && !resp.body.to_lowercase().contains("error") {
                return Ok("已提交中兴路由器 Wi-Fi 名称与密码修改，请使用新密码重新连接。".to_string());
            }
        }

        Err("当前中兴固件版本未开放直连改密接口，已为你保留管理员登录状态，请点击「打开网页后台」在 WLAN 设置中完成保存。".to_string())
    }

    fn block_mac(
        &self,
        _session: &RouterSession,
        _mac: &str,
        _block: bool,
    ) -> Result<String, String> {
        Err("当前中兴型号暂不支持一键踢出，请点击「打开网页后台」在终端管理中禁用设备。".to_string())
    }

    fn reboot(&self, session: &RouterSession) -> Result<String, String> {
        if session.mode == "zte_api" {
            let resp = http_request(
                &session.gateway_ip,
                "POST",
                "/api/v1/reboot",
                Some("application/json"),
                session.cookie.as_deref(),
                Some("{}"),
            )?;
            if resp.status_code == 200 {
                return Ok("已向中兴路由器发送重启指令，约 1~2 分钟后恢复上网。".to_string());
            }
        }

        let form = form_urlencode(&[
            ("IF_ACTION", "devrestart"),
            ("IF_ERRORSTR", "SUCC"),
            ("flag", "1"),
        ]);
        let resp = http_request(
            &session.gateway_ip,
            "POST",
            "/getpage.gch?pid=1002&nextpage=manager_dev_conf_t.gch",
            Some("application/x-www-form-urlencoded"),
            session.cookie.as_deref(),
            Some(&form),
        )?;
        if resp.status_code == 200 && !resp.body.contains("Frm_Logintoken") {
            return Ok("已向中兴设备发送重启请求，若未生效可在网页后台确认一次。".to_string());
        }

        Err("当前中兴固件限制了远程重启接口，请前往网页后台操作。".to_string())
    }
}

pub fn parse_zte_clients_json(json: &Value) -> Vec<RouterClientInfo> {
    let arr = json
        .as_array()
        .or_else(|| json.get("clients").and_then(|v| v.as_array()))
        .or_else(|| json.pointer("/data/clients").and_then(|v| v.as_array()));
    let Some(items) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        let mac_raw = item
            .get("mac")
            .or_else(|| item.get("MacAddress"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(mac) = normalize_mac(mac_raw) {
            let ip = item
                .get("ip")
                .or_else(|| item.get("IPAddress"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let name = item
                .get("name")
                .or_else(|| item.get("HostName"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let band = item.get("band").and_then(|v| v.as_str()).map(String::from);
            out.push(RouterClientInfo {
                ip,
                mac,
                name,
                band,
                signal_dbm: None,
            });
        }
    }
    out
}

pub fn parse_zte_gch_clients(html: &str) -> Vec<RouterClientInfo> {
    // 从 .gch 页面表格/JS 变量中提取形如 aa:bb:cc:dd:ee:ff 的 MAC
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for token in html.split(|c: char| !c.is_ascii_hexdigit() && c != ':' && c != '-') {
        if token.len() == 17 {
            if let Some(mac) = normalize_mac(token) {
                if mac != "ff:ff:ff:ff:ff:ff" && seen.insert(mac.clone()) {
                    out.push(RouterClientInfo {
                        ip: None,
                        mac,
                        name: None,
                        band: Some("Wi-Fi".to_string()),
                        signal_dbm: None,
                    });
                }
            }
        }
    }
    out
}

// ============================================================================
// 纯 Rust 轻量哈希辅助（SHA-1 / SHA-256，供小米/通用路由器登录挑战使用）
// ============================================================================

pub fn sha1_hex(input: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }
    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}

pub fn sha256_hex(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{:08x}", v)).collect()
}

// ============================================================================
// 小米 / Redmi 路由器适配器（MiWiFi LuCI JSON API）
// ============================================================================

impl XiaomiAdapter {
    const MIWIFI_KEY: &'static str = "a2ffa5c9be07488bbb04a3a47d3c5f6a";

    fn api_get(gateway_ip: &str, token: &str, path: &str) -> Result<Value, String> {
        let full = format!("/cgi-bin/luci/;stok={}/api/{}", token, path);
        let resp = http_request(gateway_ip, "GET", &full, None, None, None)?;
        serde_json::from_str(&resp.body).map_err(|_| "小米路由器返回格式异常".to_string())
    }

    fn api_post(
        gateway_ip: &str,
        token: &str,
        path: &str,
        pairs: &[(&str, &str)],
    ) -> Result<Value, String> {
        let full = format!("/cgi-bin/luci/;stok={}/api/{}", token, path);
        let body = form_urlencode(pairs);
        let resp = http_request(
            gateway_ip,
            "POST",
            &full,
            Some("application/x-www-form-urlencoded"),
            None,
            Some(&body),
        )?;
        serde_json::from_str(&resp.body).map_err(|_| "小米路由器返回格式异常".to_string())
    }
}

impl RouterAdapter for XiaomiAdapter {
    fn id(&self) -> &'static str {
        "xiaomi"
    }

    fn display_name(&self) -> &'static str {
        "小米 / Redmi 路由器 (MiWiFi)"
    }

    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            supports_clients: true,
            supports_wifi_config: true,
            supports_block_mac: true,
            supports_guest_wifi: true,
            supports_reboot: true,
            default_username: Some("admin".to_string()),
            note: "已接入小米 MiWiFi 适配器：支持读取终端列表、修改 Wi-Fi 密码、开关访客网络、拉黑设备及重启。".to_string(),
        }
    }

    fn matches(&self, info: &RouterProbeInfo) -> bool {
        let combined = format!(
            "{} {} {}",
            info.http_status_and_headers,
            info.http_body,
            info.page_title.unwrap_or("")
        )
        .to_lowercase();
        info.mac_vendor == Some("小米 Xiaomi")
            || combined.contains("miwifi")
            || combined.contains("xiaoqiang")
            || combined.contains("小米路由器")
    }

    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String> {
        let user = username
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("admin");

        let init_resp = http_request(
            gateway_ip,
            "GET",
            "/cgi-bin/luci/api/xqsystem/init_info",
            None,
            None,
            None,
        )
        .ok();
        let (device_id, encrypt_mode) = init_resp
            .and_then(|r| serde_json::from_str::<Value>(&r.body).ok())
            .map(|j| {
                let id = j
                    .get("id")
                    .or_else(|| j.get("routerId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("00:00:00:00:00:00")
                    .to_string();
                let mode = j.get("newEncryptMode").and_then(|v| v.as_i64()).unwrap_or(0);
                (id, mode)
            })
            .unwrap_or_else(|| ("00:00:00:00:00:00".to_string(), 0));

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1700000000);
        let nonce = format!("0_{}_{}_{}", device_id, ts, (ts % 9000) + 1000);

        let pwd_hash = if encrypt_mode == 1 {
            let inner = sha256_hex(format!("{}{}", password, Self::MIWIFI_KEY).as_bytes());
            sha256_hex(format!("{}{}", nonce, inner).as_bytes())
        } else {
            let inner = sha1_hex(format!("{}{}", password, Self::MIWIFI_KEY).as_bytes());
            sha1_hex(format!("{}{}", nonce, inner).as_bytes())
        };

        let form = form_urlencode(&[
            ("username", user),
            ("password", &pwd_hash),
            ("logtype", "2"),
            ("nonce", &nonce),
        ]);
        let resp = http_request(
            gateway_ip,
            "POST",
            "/cgi-bin/luci/api/xqsystem/login",
            Some("application/x-www-form-urlencoded"),
            None,
            Some(&form),
        )?;
        let json: Value = serde_json::from_str(&resp.body)
            .map_err(|_| "小米路由器登录接口响应异常".to_string())?;
        if json.get("code").and_then(|c| c.as_i64()) == Some(0) {
            if let Some(token) = json.get("token").and_then(|v| v.as_str()) {
                return Ok(RouterSession {
                    gateway_ip: gateway_ip.to_string(),
                    adapter_id: "xiaomi".to_string(),
                    auth_token: token.to_string(),
                    cookie: None,
                    username: user.to_string(),
                    mode: "miwifi".to_string(),
                });
            }
        }
        Err("小米路由器密码校验失败，请核对管理密码（也可参考米家 App）".to_string())
    }

    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String> {
        let json = Self::api_get(&session.gateway_ip, &session.auth_token, "misystem/devicelist")?;
        if json.get("code").and_then(|c| c.as_i64()) == Some(0) {
            return Ok(parse_xiaomi_devicelist_json(&json));
        }
        Err("未能读取小米路由器终端列表".to_string())
    }

    fn update_wifi_config(
        &self,
        session: &RouterSession,
        ssid: &str,
        password: &str,
    ) -> Result<String, String> {
        if password.len() < 8 {
            return Err("Wi-Fi 密码至少需要 8 位字符".to_string());
        }
        let mut updated = 0;
        for idx in ["1", "2"] {
            let res = Self::api_post(
                &session.gateway_ip,
                &session.auth_token,
                "xqnetwork/set_wifi",
                &[
                    ("wifiIndex", idx),
                    ("on", "1"),
                    ("ssid", ssid),
                    ("pwd", password),
                    ("encryption", "mixed-psk"),
                ],
            );
            if let Ok(v) = res {
                if v.get("code").and_then(|c| c.as_i64()) == Some(0) {
                    updated += 1;
                }
            }
        }
        if updated > 0 {
            return Ok(format!("已更新小米路由器 {} 个频段的 Wi-Fi 名称与密码", updated));
        }
        Err("小米路由器未接受修改请求，请检查账号权限或前往网页后台保存".to_string())
    }

    fn block_mac(
        &self,
        session: &RouterSession,
        mac: &str,
        block: bool,
    ) -> Result<String, String> {
        let norm = normalize_mac(mac).ok_or_else(|| "无效的 MAC 地址".to_string())?;
        let upper = norm.to_uppercase();
        let res = Self::api_post(
            &session.gateway_ip,
            &session.auth_token,
            "xqsystem/mac_filter",
            &[
                ("mac", &upper),
                ("wan", if block { "0" } else { "1" }),
            ],
        )?;
        if res.get("code").and_then(|c| c.as_i64()) == Some(0) {
            return Ok(if block {
                format!("已在小米路由器禁止 {} 上网", upper)
            } else {
                format!("已恢复 {} 的上网权限", upper)
            });
        }
        Err("小米路由器黑名单设置未成功，请在米家 App 或网页后台确认".to_string())
    }

    fn set_guest_wifi(
        &self,
        session: &RouterSession,
        enabled: bool,
        ssid: Option<&str>,
        password: Option<&str>,
    ) -> Result<String, String> {
        let mut pairs = vec![("on", if enabled { "1" } else { "0" })];
        if let Some(s) = ssid.filter(|v| !v.trim().is_empty()) {
            pairs.push(("ssid", s.trim()));
        }
        if let Some(pw) = password.filter(|v| v.len() >= 8) {
            pairs.push(("pwd", pw));
            pairs.push(("encryption", "mixed-psk"));
        } else if enabled {
            pairs.push(("encryption", "none"));
        }
        let res = Self::api_post(
            &session.gateway_ip,
            &session.auth_token,
            "xqnetwork/set_wifi_guest",
            &pairs,
        )?;
        if res.get("code").and_then(|c| c.as_i64()) == Some(0) {
            return Ok(if enabled {
                "已开启小米路由器访客 Wi-Fi".to_string()
            } else {
                "已关闭小米路由器访客 Wi-Fi".to_string()
            });
        }
        Err("访客 Wi-Fi 设置未成功，可能当前固件不支持该接口".to_string())
    }

    fn reboot(&self, session: &RouterSession) -> Result<String, String> {
        let res = Self::api_get(
            &session.gateway_ip,
            &session.auth_token,
            "xqsystem/reboot?client=web",
        )?;
        if res.get("code").and_then(|c| c.as_i64()) == Some(0) {
            return Ok("已向小米路由器发送重启指令，约 1~2 分钟后恢复上网".to_string());
        }
        Err("小米路由器重启请求未成功".to_string())
    }
}

pub fn parse_xiaomi_devicelist_json(json: &Value) -> Vec<RouterClientInfo> {
    let Some(list) = json.get("list").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in list {
        let mac_raw = item.get("mac").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(mac) = normalize_mac(mac_raw) {
            let ip = item
                .get("ip")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|obj| obj.get("ip"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let name = item
                .get("name")
                .or_else(|| item.get("oname"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            let dev_type = item.get("type").and_then(|v| v.as_i64()).unwrap_or(0);
            let band = match dev_type {
                1 => Some("2.4G".to_string()),
                2 | 6 => Some("5G".to_string()),
                0 => Some("有线".to_string()),
                _ => Some("Wi-Fi".to_string()),
            };
            out.push(RouterClientInfo {
                ip,
                mac,
                name,
                band,
                signal_dbm: None,
            });
        }
    }
    out
}

// ============================================================================
// TP-Link 适配器（Stok JSON /ds API）
// ============================================================================

impl TpLinkAdapter {
    fn ds_call(gateway_ip: &str, stok: &str, payload: &Value) -> Result<Value, String> {
        let path = format!("/stok={}/ds", stok);
        let body = payload.to_string();
        let resp = http_request(
            gateway_ip,
            "POST",
            &path,
            Some("application/json"),
            None,
            Some(&body),
        )?;
        serde_json::from_str(&resp.body).map_err(|_| "TP-Link 接口响应解析失败".to_string())
    }
}

impl RouterAdapter for TpLinkAdapter {
    fn id(&self) -> &'static str {
        "tplink"
    }

    fn display_name(&self) -> &'static str {
        "TP-Link 路由器"
    }

    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            supports_clients: true,
            supports_wifi_config: true,
            supports_block_mac: true,
            supports_guest_wifi: true,
            supports_reboot: true,
            default_username: Some("admin".to_string()),
            note: "已接入 TP-Link 适配器（支持经典 Stok JSON 接口）；部分较新 RSA 加密固件将自动提示在网页后台操作。".to_string(),
        }
    }

    fn matches(&self, info: &RouterProbeInfo) -> bool {
        let combined = format!(
            "{} {} {}",
            info.http_status_and_headers,
            info.http_body,
            info.page_title.unwrap_or("")
        )
        .to_lowercase();
        info.mac_vendor == Some("TP-Link")
            || combined.contains("tp-link")
            || combined.contains("tplogin")
            || combined.contains("tplink")
    }

    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String> {
        let user = username
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("admin");

        let login_payload = serde_json::json!({
            "method": "do",
            "login": { "password": password }
        })
        .to_string();
        let resp = http_request(
            gateway_ip,
            "POST",
            "/",
            Some("application/json"),
            None,
            Some(&login_payload),
        )?;
        if let Ok(json) = serde_json::from_str::<Value>(&resp.body) {
            if json.get("error_code").and_then(|v| v.as_i64()) == Some(0) {
                if let Some(stok) = json.get("stok").and_then(|v| v.as_str()) {
                    return Ok(RouterSession {
                        gateway_ip: gateway_ip.to_string(),
                        adapter_id: "tplink".to_string(),
                        auth_token: stok.to_string(),
                        cookie: None,
                        username: user.to_string(),
                        mode: "tplink_stok".to_string(),
                    });
                }
            }
            if json.pointer("/data/code").is_some() || json.get("error_code").and_then(|v| v.as_i64()) == Some(-40401) {
                return Err("TP-Link 登录未通过：请核对密码；若为新加密固件请点击「网页后台」操作。".to_string());
            }
        }
        Err("TP-Link 路由器登录失败，请核对管理密码或点击「网页后台」手动管理".to_string())
    }

    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String> {
        let payload = serde_json::json!({
            "hosts_info": { "table": "online_host" },
            "method": "get"
        });
        let json = Self::ds_call(&session.gateway_ip, &session.auth_token, &payload)?;
        let list = parse_tplink_hosts_json(&json);
        if !list.is_empty() {
            return Ok(list);
        }
        Err("未从 TP-Link 读到终端列表".to_string())
    }

    fn update_wifi_config(
        &self,
        session: &RouterSession,
        ssid: &str,
        password: &str,
    ) -> Result<String, String> {
        if password.len() < 8 {
            return Err("Wi-Fi 密码至少需要 8 位字符".to_string());
        }
        let payload = serde_json::json!({
            "wireless": {
                "wlan_host_2g": { "ssid": ssid, "key": password },
                "wlan_host_5g": { "ssid": ssid, "key": password }
            },
            "method": "set"
        });
        let json = Self::ds_call(&session.gateway_ip, &session.auth_token, &payload)?;
        if json.get("error_code").and_then(|v| v.as_i64()) == Some(0) {
            return Ok("已更新 TP-Link Wi-Fi 名称与密码，请使用新密码重连。".to_string());
        }
        Err("TP-Link 改密未成功，请在网页后台确认".to_string())
    }

    fn block_mac(
        &self,
        session: &RouterSession,
        mac: &str,
        block: bool,
    ) -> Result<String, String> {
        let norm = normalize_mac(mac).ok_or_else(|| "无效的 MAC 地址".to_string())?;
        let upper = norm.to_uppercase();
        let payload = if block {
            serde_json::json!({
                "hosts_info": {
                    "table": "online_host",
                    "name": upper,
                    "blocked": "1"
                },
                "method": "set"
            })
        } else {
            serde_json::json!({
                "hosts_info": {
                    "table": "blocked_host",
                    "name": upper,
                    "blocked": "0"
                },
                "method": "set"
            })
        };
        let json = Self::ds_call(&session.gateway_ip, &session.auth_token, &payload)?;
        if json.get("error_code").and_then(|v| v.as_i64()) == Some(0) {
            return Ok(if block {
                format!("已在 TP-Link 拉黑 {}", upper)
            } else {
                format!("已在 TP-Link 解除拉黑 {}", upper)
            });
        }
        Err("TP-Link 设备黑名单修改未成功".to_string())
    }

    fn set_guest_wifi(
        &self,
        session: &RouterSession,
        enabled: bool,
        ssid: Option<&str>,
        password: Option<&str>,
    ) -> Result<String, String> {
        let mut guest = serde_json::json!({
            "enable": if enabled { "1" } else { "0" }
        });
        if let Some(s) = ssid.filter(|v| !v.trim().is_empty()) {
            guest["ssid"] = Value::String(s.trim().to_string());
        }
        if let Some(pw) = password.filter(|v| v.len() >= 8) {
            guest["key"] = Value::String(pw.to_string());
        }
        let payload = serde_json::json!({
            "wireless": { "wlan_guest_2g": guest },
            "method": "set"
        });
        let json = Self::ds_call(&session.gateway_ip, &session.auth_token, &payload)?;
        if json.get("error_code").and_then(|v| v.as_i64()) == Some(0) {
            return Ok(if enabled {
                "已开启 TP-Link 访客 Wi-Fi".to_string()
            } else {
                "已关闭 TP-Link 访客 Wi-Fi".to_string()
            });
        }
        Err("TP-Link 访客网络设置未成功".to_string())
    }

    fn reboot(&self, session: &RouterSession) -> Result<String, String> {
        let payload = serde_json::json!({
            "system": { "reboot": Value::Null },
            "method": "do"
        });
        let json = Self::ds_call(&session.gateway_ip, &session.auth_token, &payload)?;
        if json.get("error_code").and_then(|v| v.as_i64()) == Some(0) {
            return Ok("已向 TP-Link 发送重启指令".to_string());
        }
        Err("TP-Link 重启指令未被接受".to_string())
    }
}

pub fn parse_tplink_hosts_json(json: &Value) -> Vec<RouterClientInfo> {
    let Some(arr) = json
        .pointer("/hosts_info/online_host")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in arr {
        let obj = entry
            .as_object()
            .and_then(|m| m.values().next())
            .unwrap_or(entry);
        let mac_raw = obj.get("mac").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(mac) = normalize_mac(mac_raw) {
            let ip = obj.get("ip").and_then(|v| v.as_str()).map(String::from);
            let name = obj
                .get("hostname")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            let htype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let band = if htype.contains("5") {
                Some("5G".to_string())
            } else if htype.contains("2") || htype.contains("1") {
                Some("2.4G".to_string())
            } else {
                Some("有线/Wi-Fi".to_string())
            };
            out.push(RouterClientInfo {
                ip,
                mac,
                name,
                band,
                signal_dbm: None,
            });
        }
    }
    out
}

// ============================================================================
// 华为 / 荣耀适配器（HiLink API 识别与基础对接）
// ============================================================================

impl RouterAdapter for HuaweiAdapter {
    fn id(&self) -> &'static str {
        "huawei"
    }

    fn display_name(&self) -> &'static str {
        "华为 / 荣耀路由器 (HiLink)"
    }

    fn capabilities(&self) -> RouterCapabilities {
        RouterCapabilities {
            supports_clients: true,
            supports_wifi_config: false,
            supports_block_mac: false,
            supports_guest_wifi: false,
            supports_reboot: true,
            default_username: Some("admin".to_string()),
            note: "已识别华为 / 荣耀 HiLink 路由器：支持经典型号终端查看与重启；新款鸿蒙智联路由器采用 SCRAM 双向校验，改密建议使用「智慧生活 App」或点击打开网页后台。".to_string(),
        }
    }

    fn matches(&self, info: &RouterProbeInfo) -> bool {
        let combined = format!(
            "{} {} {}",
            info.http_status_and_headers,
            info.http_body,
            info.page_title.unwrap_or("")
        )
        .to_lowercase();
        info.mac_vendor == Some("华为 Huawei")
            || combined.contains("huawei")
            || combined.contains("hilink")
            || combined.contains("honor")
            || combined.contains("华为")
    }

    fn authenticate(
        &self,
        gateway_ip: &str,
        username: Option<&str>,
        password: &str,
    ) -> Result<RouterSession, String> {
        let user = username
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("admin");

        let info_resp = http_request(gateway_ip, "GET", "/api/system/deviceinfo", None, None, None)?;
        if info_resp.status_code == 200 && info_resp.body.contains("MAC") {
            return Ok(RouterSession {
                gateway_ip: gateway_ip.to_string(),
                adapter_id: "huawei".to_string(),
                auth_token: password.to_string(),
                cookie: info_resp.set_cookies.first().cloned(),
                username: user.to_string(),
                mode: "hilink".to_string(),
            });
        }

        Err("当前华为路由器采用增强挑战认证，请点击「打开网页后台」或使用「智慧生活 App」进行管理。".to_string())
    }

    fn list_clients(&self, session: &RouterSession) -> Result<Vec<RouterClientInfo>, String> {
        let resp = http_request(
            &session.gateway_ip,
            "GET",
            "/api/system/HostInfo",
            None,
            session.cookie.as_deref(),
            None,
        )?;
        if resp.status_code == 200 {
            if let Ok(json) = serde_json::from_str::<Value>(&resp.body) {
                let list = parse_huawei_hosts_json(&json);
                if !list.is_empty() {
                    return Ok(list);
                }
            }
        }
        Err("未从当前华为固件读取到终端列表，请以「谁在用我的网」扫描结果为准。".to_string())
    }

    fn update_wifi_config(
        &self,
        _session: &RouterSession,
        _ssid: &str,
        _password: &str,
    ) -> Result<String, String> {
        Err("华为新款路由器改密需使用 SCRAM 会话校验，请点击「打开网页后台」或用「智慧生活 App」修改。".to_string())
    }

    fn block_mac(
        &self,
        _session: &RouterSession,
        _mac: &str,
        _block: bool,
    ) -> Result<String, String> {
        Err("请点击「打开网页后台」在华为路由器终端管理中禁用设备。".to_string())
    }

    fn reboot(&self, session: &RouterSession) -> Result<String, String> {
        let resp = http_request(
            &session.gateway_ip,
            "POST",
            "/api/service/reboot.cgi",
            Some("application/json"),
            session.cookie.as_deref(),
            Some("{}"),
        )?;
        if resp.status_code == 200 {
            return Ok("已向华为路由器发送重启请求".to_string());
        }
        Err("当前华为型号限制了远程重启接口，请前往网页后台操作。".to_string())
    }
}

pub fn parse_huawei_hosts_json(json: &Value) -> Vec<RouterClientInfo> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let mac_raw = item.get("MACAddress").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(mac) = normalize_mac(mac_raw) {
            let ip = item.get("IPAddress").and_then(|v| v.as_str()).map(String::from);
            let name = item
                .get("HostName")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            let layer = item.get("InterfaceType").and_then(|v| v.as_str()).unwrap_or("");
            let band = if layer.contains("5G") {
                Some("5G".to_string())
            } else if layer.contains("2.4G") {
                Some("2.4G".to_string())
            } else {
                Some("有线/Wi-Fi".to_string())
            };
            out.push(RouterClientInfo {
                ip,
                mac,
                name,
                band,
                signal_dbm: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hashes_against_rfc_vectors() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_adapter_matching() {
        let openwrt_info = RouterProbeInfo {
            gateway_ip: "192.168.1.1",
            mac_vendor: None,
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>LuCI - OpenWrt</title>",
            page_title: Some("LuCI - OpenWrt"),
        };
        assert_eq!(match_router_adapter(&openwrt_info).unwrap().id(), "openwrt");

        let xiaomi_info = RouterProbeInfo {
            gateway_ip: "192.168.31.1",
            mac_vendor: Some("小米 Xiaomi"),
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>小米路由器</title>",
            page_title: Some("小米路由器"),
        };
        assert_eq!(match_router_adapter(&xiaomi_info).unwrap().id(), "xiaomi");

        let tplink_info = RouterProbeInfo {
            gateway_ip: "192.168.1.1",
            mac_vendor: Some("TP-Link"),
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>TP-LINK</title>",
            page_title: Some("TP-LINK"),
        };
        assert_eq!(match_router_adapter(&tplink_info).unwrap().id(), "tplink");

        let huawei_info = RouterProbeInfo {
            gateway_ip: "192.168.3.1",
            mac_vendor: Some("华为 Huawei"),
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>HUAWEI HiLink</title>",
            page_title: Some("HUAWEI HiLink"),
        };
        assert_eq!(match_router_adapter(&huawei_info).unwrap().id(), "huawei");

        let zte_info = RouterProbeInfo {
            gateway_ip: "192.168.5.1",
            mac_vendor: Some("中兴 ZTE"),
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>ZXHN F7607P</title>",
            page_title: Some("ZXHN F7607P"),
        };
        assert_eq!(match_router_adapter(&zte_info).unwrap().id(), "zte");
    }

    #[test]
    fn test_parse_openwrt_status_json() {
        let sample = serde_json::json!({
            "leases": [
                { "macaddr": "AA:BB:CC:11:22:33", "ipaddr": "192.168.1.105", "hostname": "iPhone" }
            ],
            "wifinets": [
                {
                    "name": "radio1",
                    "networks": [
                        {
                            "frequency": 5.18,
                            "assoclist": {
                                "AA:BB:CC:11:22:33": { "signal": -52 }
                            }
                        }
                    ]
                }
            ]
        });
        let clients = parse_openwrt_status_json(&sample);
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].mac, "aa:bb:cc:11:22:33");
        assert_eq!(clients[0].band.as_deref(), Some("5G"));
        assert_eq!(clients[0].signal_dbm, Some(-52));
    }

    #[test]
    fn test_parse_zte_gch_clients() {
        let html = r#"<script>Transfer_meaning('MACAddress0','C4:92:D9:11:22:33');Transfer_meaning('MACAddress1','A4:83:E7:44:55:66');</script>"#;
        let clients = parse_zte_gch_clients(html);
        assert_eq!(clients.len(), 2);
        assert_eq!(clients[0].mac, "c4:92:d9:11:22:33");
    }

    #[test]
    fn test_parse_xiaomi_tplink_huawei_clients() {
        let xm = serde_json::json!({
            "code": 0,
            "list": [
                { "mac": "AA:BB:CC:00:11:22", "name": "iPad", "type": 2, "ip": [{"ip": "192.168.31.12"}] }
            ]
        });
        let xm_res = parse_xiaomi_devicelist_json(&xm);
        assert_eq!(xm_res.len(), 1);
        assert_eq!(xm_res[0].band.as_deref(), Some("5G"));

        let tp = serde_json::json!({
            "hosts_info": {
                "online_host": [
                    { "h1": { "mac": "11-22-33-44-55-66", "ip": "192.168.1.10", "hostname": "MacBook", "type": "5G" } }
                ]
            }
        });
        let tp_res = parse_tplink_hosts_json(&tp);
        assert_eq!(tp_res.len(), 1);
        assert_eq!(tp_res[0].mac, "11:22:33:44:55:66");

        let hw = serde_json::json!([
            { "MACAddress": "22:33:44:55:66:77", "IPAddress": "192.168.3.8", "HostName": "MatePad", "InterfaceType": "5G" }
        ]);
        let hw_res = parse_huawei_hosts_json(&hw);
        assert_eq!(hw_res.len(), 1);
        assert_eq!(hw_res[0].name.as_deref(), Some("MatePad"));
    }
}
