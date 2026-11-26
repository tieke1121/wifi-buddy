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
    fn reboot(&self, session: &RouterSession) -> Result<String, String>;
}

pub struct OpenWrtAdapter;
pub struct ZteAdapter;

pub fn get_adapter_by_id(id: &str) -> Option<Box<dyn RouterAdapter>> {
    match id {
        "openwrt" => Some(Box::new(OpenWrtAdapter)),
        "zte" => Some(Box::new(ZteAdapter)),
        _ => None,
    }
}

pub fn match_router_adapter(info: &RouterProbeInfo) -> Option<Box<dyn RouterAdapter>> {
    let adapters: Vec<Box<dyn RouterAdapter>> =
        vec![Box::new(OpenWrtAdapter), Box::new(ZteAdapter)];
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
            supports_reboot: true,
            default_username: Some("root".to_string()),
            note: "已接入 OpenWrt LuCI 适配器：支持查看无线与 DHCP 终端、修改 Wi-Fi 密码、拉黑设备及重启（写操作需路由器已安装 luci-mod-rpc）。".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_matching() {
        let openwrt_info = RouterProbeInfo {
            gateway_ip: "192.168.1.1",
            mac_vendor: None,
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>LuCI - OpenWrt</title>",
            page_title: Some("LuCI - OpenWrt"),
        };
        let matched = match_router_adapter(&openwrt_info).unwrap();
        assert_eq!(matched.id(), "openwrt");

        let zte_info = RouterProbeInfo {
            gateway_ip: "192.168.5.1",
            mac_vendor: Some("中兴 ZTE"),
            http_status_and_headers: "HTTP/1.1 200 OK",
            http_body: "<title>ZXHN F7607P</title>",
            page_title: Some("ZXHN F7607P"),
        };
        let matched_zte = match_router_adapter(&zte_info).unwrap();
        assert_eq!(matched_zte.id(), "zte");
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
}
