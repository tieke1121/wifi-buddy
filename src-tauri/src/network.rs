use crate::oui::{classify_device, is_randomized_mac, lookup_vendor, normalize_mac};
use crate::router::{match_router_adapter, RouterCapabilities, RouterProbeInfo};
use crate::ssdp::discover_ssdp_devices;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct RouterFingerprint {
    pub gateway_ip: String,
    pub gateway_mac: Option<String>,
    pub brand: String,
    pub admin_url: String,
    pub page_title: Option<String>,
    pub adapter_id: Option<String>,
    pub adapter_supported: bool,
    pub capabilities: Option<RouterCapabilities>,
    pub guide_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkOverview {
    pub interface_name: String,
    pub local_ip: String,
    pub subnet_cidr: String,
    pub ssid: Option<String>,
    pub ssid_redacted: bool,
    pub preferred_ssids: Vec<String>,
    pub signal_dbm: Option<i32>,
    pub noise_dbm: Option<i32>,
    pub channel: Option<String>,
    pub phy_mode: Option<String>,
    pub tx_rate_mbps: Option<u32>,
    pub router: RouterFingerprint,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawLanDevice {
    pub ip: String,
    pub mac: String,
    pub vendor: String,
    pub hostname: Option<String>,
    pub device_type: String,
    pub default_name: String,
    pub is_self: bool,
    pub is_gateway: bool,
    pub is_randomized_mac: bool,
    pub ping_alive: bool,
    pub latency_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthStep {
    pub id: String,
    pub title: String,
    pub status: String, // "good" | "warning" | "error"
    pub badge: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub overall_status: String, // "good" | "warning" | "error"
    pub summary_title: String,
    pub summary_advice: String,
    pub gateway_latency_ms: Option<u32>,
    pub internet_latency_ms: Option<u32>,
    pub signal_dbm: Option<i32>,
    pub steps: Vec<HealthStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WifiPasswordResult {
    pub ssid: String,
    pub password: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeedTestResult {
    pub download_mbps: Option<f64>,
    pub quality_label: String,
    pub advice: String,
    pub source: String,
    pub error: Option<String>,
}

fn run_cmd(prog: &str, args: &[&str]) -> Option<String> {
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            if s.trim().is_empty() {
                s = String::from_utf8_lossy(&o.stderr).to_string();
            }
            s
        })
}

/// 获取活跃网卡、本机 IP 与默认网关
fn detect_interface_and_gateway() -> (String, String, String) {
    #[cfg(target_os = "macos")]
    {
        let mut iface = "en0".to_string();
        let mut gateway = String::new();
        if let Some(out) = run_cmd("route", &["-n", "get", "default"]) {
            for line in out.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("interface:") {
                    iface = rest.trim().to_string();
                } else if let Some(rest) = t.strip_prefix("gateway:") {
                    gateway = rest.trim().to_string();
                }
            }
        }
        let mut local_ip = String::new();
        if let Some(out) = run_cmd("ipconfig", &["getifaddr", &iface]) {
            let trimmed = out.trim();
            if trimmed.parse::<Ipv4Addr>().is_ok() {
                local_ip = trimmed.to_string();
            }
        }
        if local_ip.is_empty() {
            local_ip = detect_local_ip_udp().unwrap_or_else(|| "192.168.1.100".to_string());
        }
        if gateway.is_empty() {
            gateway = guess_gateway_from_ip(&local_ip);
        }
        return (iface, local_ip, gateway);
    }

    #[cfg(target_os = "windows")]
    {
        let local_ip = detect_local_ip_udp().unwrap_or_else(|| "192.168.1.100".to_string());
        let mut gateway = String::new();
        if let Some(out) = run_cmd("route", &["print", "0.0.0.0"]) {
            for line in out.lines() {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() >= 4 && cols[0] == "0.0.0.0" && cols[1] == "0.0.0.0" {
                    if cols[2].parse::<Ipv4Addr>().is_ok() {
                        gateway = cols[2].to_string();
                        break;
                    }
                }
            }
        }
        if gateway.is_empty() {
            gateway = guess_gateway_from_ip(&local_ip);
        }
        return ("Wi-Fi".to_string(), local_ip, gateway);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let local_ip = detect_local_ip_udp().unwrap_or_else(|| "192.168.1.100".to_string());
        let gateway = guess_gateway_from_ip(&local_ip);
        ("wlan0".to_string(), local_ip, gateway)
    }
}

fn detect_local_ip_udp() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("223.5.5.5:80").ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(addr) => Some(addr.ip().to_string()),
        _ => None,
    }
}

fn guess_gateway_from_ip(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        format!("{}.{}.{}.1", parts[0], parts[1], parts[2])
    } else {
        "192.168.1.1".to_string()
    }
}

fn subnet_prefix_from_ip(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        format!("{}.{}.{}", parts[0], parts[1], parts[2])
    } else {
        "192.168.1".to_string()
    }
}

#[cfg(target_os = "macos")]
fn get_self_mac_macos(iface: &str) -> Option<String> {
    let out = run_cmd("ifconfig", &[iface])?;
    for line in out.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("ether ") {
            return normalize_mac(rest.trim());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn read_macos_wifi_details(iface: &str) -> (Option<String>, bool, Vec<String>, Option<i32>, Option<i32>, Option<String>, Option<String>, Option<u32>) {
    let mut ssid: Option<String> = None;
    let mut ssid_redacted = false;
    if let Some(summary) = run_cmd("ipconfig", &["getsummary", iface]) {
        for line in summary.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("SSID :") {
                let val = rest.trim();
                if val == "<redacted>" {
                    ssid_redacted = true;
                } else if !val.is_empty() {
                    ssid = Some(val.to_string());
                }
            }
        }
    }

    let mut preferred_ssids = Vec::new();
    if let Some(pref) = run_cmd("networksetup", &["-listpreferredwirelessnetworks", iface]) {
        for line in pref.lines().skip(1) {
            let name = line.trim();
            if !name.is_empty() && preferred_ssids.len() < 12 {
                preferred_ssids.push(name.to_string());
            }
        }
    }

    let mut signal_dbm = None;
    let mut noise_dbm = None;
    let mut channel = None;
    let mut phy_mode = None;
    let mut tx_rate_mbps = None;

    if let Some(sp) = run_cmd("system_profiler", &["SPAirPortDataType"]) {
        let mut in_current = false;
        for line in sp.lines() {
            let t = line.trim();
            if t.starts_with("Current Network Information:") {
                in_current = true;
                continue;
            }
            if in_current {
                if t.starts_with("awdl0:") || t.starts_with("Other Local Wi-Fi Networks:") {
                    break;
                }
                if ssid.is_none() && t.ends_with(':') && !t.contains(' ') && t != "<redacted>:" {
                    ssid = Some(t.trim_end_matches(':').to_string());
                } else if let Some(v) = t.strip_prefix("PHY Mode:") {
                    phy_mode = Some(v.trim().to_string());
                } else if let Some(v) = t.strip_prefix("Channel:") {
                    channel = Some(v.trim().to_string());
                } else if let Some(v) = t.strip_prefix("Transmit Rate:") {
                    tx_rate_mbps = v.trim().parse::<u32>().ok();
                } else if let Some(v) = t.strip_prefix("Signal / Noise:") {
                    // 格式: "-53 dBm / -92 dBm"
                    let parts: Vec<&str> = v.split('/').collect();
                    if let Some(sig) = parts.first() {
                        signal_dbm = sig.trim().split_whitespace().next().and_then(|n| n.parse::<i32>().ok());
                    }
                    if let Some(noi) = parts.get(1) {
                        noise_dbm = noi.trim().split_whitespace().next().and_then(|n| n.parse::<i32>().ok());
                    }
                }
            }
        }
    }

    (ssid, ssid_redacted, preferred_ssids, signal_dbm, noise_dbm, channel, phy_mode, tx_rate_mbps)
}

#[cfg(target_os = "windows")]
fn read_windows_wifi_details() -> (Option<String>, bool, Vec<String>, Option<i32>, Option<i32>, Option<String>, Option<String>, Option<u32>) {
    let mut ssid = None;
    let mut signal_dbm = None;
    let mut channel = None;
    let mut phy_mode = None;
    let mut tx_rate_mbps = None;

    if let Some(out) = run_cmd("netsh", &["wlan", "show", "interfaces"]) {
        for line in out.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_lowercase();
                let val = v.trim().to_string();
                if key == "ssid" && !val.is_empty() {
                    ssid = Some(val);
                } else if key == "radio type" || key.contains("无线电类型") {
                    phy_mode = Some(val);
                } else if key == "channel" || key.contains("信道") {
                    channel = Some(format!("信道 {}", val));
                } else if key.contains("receive rate") || key.contains("接收速率") {
                    tx_rate_mbps = val.split_whitespace().next().and_then(|n| n.parse::<f32>().ok()).map(|f| f as u32);
                } else if key == "signal" || key.contains("信号") {
                    if let Some(pct) = val.trim_end_matches('%').trim().parse::<i32>().ok() {
                        // 将 Windows 百分比信号粗略换算为 dBm (-100 ~ -30)
                        signal_dbm = Some(-100 + (pct * 70 / 100));
                    }
                }
            }
        }
    }

    let mut preferred_ssids = Vec::new();
    if let Some(out) = run_cmd("netsh", &["wlan", "show", "profiles"]) {
        for line in out.lines() {
            if let Some((_, v)) = line.split_once(':') {
                let name = v.trim();
                if !name.is_empty() && !name.contains("User profiles") && preferred_ssids.len() < 12 {
                    preferred_ssids.push(name.to_string());
                }
            }
        }
    }

    (ssid, false, preferred_ssids, signal_dbm, None, channel, phy_mode, tx_rate_mbps)
}

fn probe_gateway_fingerprint(gateway_ip: &str, gateway_mac: Option<&str>) -> RouterFingerprint {
    let mac_vendor = gateway_mac.and_then(lookup_vendor);
    let mut page_title: Option<String> = None;
    let mut http_raw = String::new();
    let mut http_hint = String::new();

    if let Ok(addr) = format!("{}:80", gateway_ip).parse::<SocketAddr>() {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(900)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(900)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(900)));
            let req = format!(
                "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: wifi-buddy/0.2\r\n\r\n",
                gateway_ip
            );
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut buf = vec![0u8; 8192];
                let mut total = 0;
                while total < buf.len() {
                    match stream.read(&mut buf[total..]) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => total += n,
                    }
                }
                http_raw = String::from_utf8_lossy(&buf[..total]).to_string();
                http_hint = http_raw.to_lowercase();
                if let Some(start) = http_hint.find("<title>") {
                    if let Some(end) = http_hint[start + 7..].find("</title>") {
                        let t = http_raw[start + 7..start + 7 + end].trim().to_string();
                        if !t.is_empty() {
                            page_title = Some(t);
                        }
                    }
                }
            }
        }
    }

    let (http_headers, http_body) = http_raw
        .split_once("\r\n\r\n")
        .unwrap_or((http_raw.as_str(), ""));

    let probe_info = RouterProbeInfo {
        gateway_ip,
        mac_vendor,
        http_status_and_headers: http_headers,
        http_body,
        page_title: page_title.as_deref(),
    };
    let matched_adapter = match_router_adapter(&probe_info);

    let brand = if http_hint.contains("zte") || mac_vendor == Some("中兴 ZTE") {
        "中兴 (ZTE)".to_string()
    } else if http_hint.contains("miwifi") || http_hint.contains("xiaomi") || mac_vendor == Some("小米 Xiaomi") {
        "小米路由器".to_string()
    } else if http_hint.contains("tp-link") || http_hint.contains("tplogin") || mac_vendor == Some("TP-Link") {
        "TP-Link".to_string()
    } else if http_hint.contains("huawei") || http_hint.contains("hilink") || mac_vendor == Some("华为 Huawei") {
        "华为路由器 / 光猫".to_string()
    } else if http_hint.contains("asus") || mac_vendor == Some("华硕 ASUS") {
        "华硕 ASUS".to_string()
    } else if http_hint.contains("luci") || http_hint.contains("openwrt") {
        "OpenWrt".to_string()
    } else if let Some(v) = mac_vendor {
        v.to_string()
    } else {
        "通用家用路由器 / 光猫".to_string()
    };

    let guide_steps = if brand.contains("中兴") {
        vec![
            format!("点击下方按钮打开中兴后台（http://{}）", gateway_ip),
            "查看光猫/路由器背面贴纸上的默认管理账号和密码并登录".to_string(),
            "进入「网络 / WLAN 设置」即可修改 Wi-Fi 名称与密码".to_string(),
        ]
    } else if brand.contains("OpenWrt") {
        vec![
            format!("可在「路由器与守护」输入 root 密码直接绑定 OpenWrt（http://{}）", gateway_ip),
            "如需网页设置，点击打开 LuCI 后台进入「网络 → 无线」".to_string(),
            "建议安装 luci-mod-rpc 以开启一键改密与拉黑功能".to_string(),
        ]
    } else {
        vec![
            format!("点击打开路由器管理页面（http://{}）", gateway_ip),
            "输入管理员密码（如忘记，通常印在路由器底部贴纸上）".to_string(),
            "找到「无线设置 / Wi-Fi 设置」修改密码并保存".to_string(),
        ]
    };

    let (adapter_id, adapter_supported, capabilities) = match matched_adapter {
        Some(adapter) => (
            Some(adapter.id().to_string()),
            true,
            Some(adapter.capabilities()),
        ),
        None => (None, false, None),
    };

    RouterFingerprint {
        gateway_ip: gateway_ip.to_string(),
        gateway_mac: gateway_mac.map(|s| s.to_string()),
        brand,
        admin_url: format!("http://{}", gateway_ip),
        page_title,
        adapter_id,
        adapter_supported,
        capabilities,
        guide_steps,
    }
}

/// 读取当前 ARP 表 (ip -> mac)
fn read_arp_table(iface_filter: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    #[cfg(target_os = "macos")]
    {
        if let Some(out) = run_cmd("arp", &["-na"]) {
            for line in out.lines() {
                if let Some(iface) = iface_filter {
                    if !line.contains(&format!(" on {} ", iface)) {
                        continue;
                    }
                }
                if line.contains("(incomplete)") || line.contains("224.0.0.") || line.contains("239.255.") || line.contains("169.254.") {
                    continue;
                }
                if let (Some(lpar), Some(rpar)) = (line.find('('), line.find(')')) {
                    let ip = line[lpar + 1..rpar].trim();
                    if ip.ends_with(".255") || ip.parse::<Ipv4Addr>().is_err() {
                        continue;
                    }
                    if let Some(at_idx) = line[rpar..].find(" at ") {
                        let after = &line[rpar + at_idx + 4..];
                        if let Some(mac_raw) = after.split_whitespace().next() {
                            if let Some(mac) = normalize_mac(mac_raw) {
                                map.insert(ip.to_string(), mac);
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = iface_filter;
        if let Some(out) = run_cmd("arp", &["-a"]) {
            for line in out.lines() {
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() >= 2 && cols[0].parse::<Ipv4Addr>().is_ok() {
                    let ip = cols[0];
                    if ip.starts_with("224.") || ip.starts_with("239.") || ip.starts_with("169.254.") || ip.ends_with(".255") {
                        continue;
                    }
                    if let Some(mac) = normalize_mac(cols[1]) {
                        if mac != "ff:ff:ff:ff:ff:ff" {
                            map.insert(ip.to_string(), mac);
                        }
                    }
                }
            }
        }
    }

    map
}

fn ping_host(ip: &str, timeout_ms: u32) -> Option<u32> {
    let start = Instant::now();
    #[cfg(target_os = "macos")]
    let status = Command::new("ping")
        .args(["-c", "1", "-W", &timeout_ms.to_string(), ip])
        .output()
        .ok()?;
    #[cfg(target_os = "windows")]
    let status = Command::new("ping")
        .args(["-n", "1", "-w", &timeout_ms.to_string(), ip])
        .output()
        .ok()?;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = Command::new("ping")
        .args(["-c", "1", "-W", "1", ip])
        .output()
        .ok()?;

    if status.status.success() {
        Some(start.elapsed().as_millis().max(1) as u32)
    } else {
        None
    }
}

/// 纯 Rust UDP 发送 mDNS PTR 反向解析 (`d.c.b.a.in-addr.arpa`)，跨平台无需依赖 `dig`
fn mdns_reverse_lookup(ip: &str) -> Option<String> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let query_name = format!("{}.{}.{}.{}.in-addr.arpa", parts[3], parts[2], parts[1], parts[0]);

    let mut pkt = Vec::with_capacity(64);
    // Header: ID=0x1234, Flags=0, QDCOUNT=1, AN/NS/AR=0
    pkt.extend_from_slice(&[0x12, 0x34, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    for label in query_name.split('.') {
        pkt.push(label.len() as u8);
        pkt.extend_from_slice(label.as_bytes());
    }
    pkt.push(0x00);
    // QTYPE = PTR (12), QCLASS = IN (1)
    pkt.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]);

    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(350))).ok()?;
    sock.send_to(&pkt, "224.0.0.251:5353").ok()?;

    let mut buf = [0u8; 512];
    let (n, _) = sock.recv_from(&mut buf).ok()?;
    parse_dns_ptr_response(&buf[..n])
}

fn read_dns_name(buf: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut next_pos = pos;
    let mut depth = 0;

    while pos < buf.len() && depth < 16 {
        let len = buf[pos];
        if len == 0 {
            if !jumped {
                next_pos = pos + 1;
            }
            return Some((labels.join("."), next_pos));
        }
        if (len & 0xC0) == 0xC0 {
            if pos + 1 >= buf.len() {
                return None;
            }
            let offset = (((len & 0x3F) as usize) << 8) | (buf[pos + 1] as usize);
            if !jumped {
                next_pos = pos + 2;
                jumped = true;
            }
            pos = offset;
            depth += 1;
            continue;
        }
        let l = len as usize;
        if pos + 1 + l > buf.len() {
            return None;
        }
        labels.push(String::from_utf8_lossy(&buf[pos + 1..pos + 1 + l]).to_string());
        pos += 1 + l;
        if !jumped {
            next_pos = pos;
        }
    }
    None
}

fn parse_dns_ptr_response(buf: &[u8]) -> Option<String> {
    if buf.len() < 12 {
        return None;
    }
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    if ancount == 0 {
        return None;
    }
    let mut pos = 12;
    for _ in 0..qdcount {
        let (_, np) = read_dns_name(buf, pos)?;
        pos = np + 4;
    }
    for _ in 0..ancount {
        let (_, np) = read_dns_name(buf, pos)?;
        pos = np;
        if pos + 10 > buf.len() {
            return None;
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let rdlength = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;
        if rtype == 12 && pos + rdlength <= buf.len() {
            if let Some((target, _)) = read_dns_name(buf, pos) {
                return Some(target);
            }
        }
        pos += rdlength;
    }
    None
}

pub fn get_network_overview() -> NetworkOverview {
    let (iface, local_ip, gateway_ip) = detect_interface_and_gateway();
    let prefix = subnet_prefix_from_ip(&local_ip);
    let subnet_cidr = format!("{}.0/24", prefix);

    #[cfg(target_os = "macos")]
    let (ssid, ssid_redacted, preferred_ssids, signal_dbm, noise_dbm, channel, phy_mode, tx_rate_mbps) =
        read_macos_wifi_details(&iface);
    #[cfg(target_os = "windows")]
    let (ssid, ssid_redacted, preferred_ssids, signal_dbm, noise_dbm, channel, phy_mode, tx_rate_mbps) =
        read_windows_wifi_details();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let (ssid, ssid_redacted, preferred_ssids, signal_dbm, noise_dbm, channel, phy_mode, tx_rate_mbps) =
        (None, false, vec![], None, None, None, None, None);

    let arp = read_arp_table(Some(&iface));
    let gateway_mac = arp.get(&gateway_ip).cloned();
    let router = probe_gateway_fingerprint(&gateway_ip, gateway_mac.as_deref());

    NetworkOverview {
        interface_name: iface,
        local_ip,
        subnet_cidr,
        ssid,
        ssid_redacted,
        preferred_ssids,
        signal_dbm,
        noise_dbm,
        channel,
        phy_mode,
        tx_rate_mbps,
        router,
    }
}

pub fn scan_lan_devices() -> Vec<RawLanDevice> {
    let (iface, local_ip, gateway_ip) = detect_interface_and_gateway();
    let prefix = subnet_prefix_from_ip(&local_ip);

    // 与 Ping 扫描并行发起 SSDP 智能设备发现，不额外增加等待时间
    let ssdp_handle = thread::spawn(discover_ssdp_devices);

    // 1. 并发探测 1..254，唤醒 ARP 并记录 Ping 延迟
    let ips: Vec<String> = (1..=254).map(|i| format!("{}.{}", prefix, i)).collect();
    let queue = Arc::new(Mutex::new(ips));
    let ping_results = Arc::new(Mutex::new(HashMap::<String, u32>::new()));

    let mut handles = Vec::new();
    for _ in 0..85 {
        let q = Arc::clone(&queue);
        let res = Arc::clone(&ping_results);
        handles.push(thread::spawn(move || loop {
            let next_ip = {
                let mut guard = q.lock().unwrap();
                guard.pop()
            };
            let Some(ip) = next_ip else { break };
            if let Some(ms) = ping_host(&ip, 200) {
                res.lock().unwrap().insert(ip, ms);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    let ping_map = ping_results.lock().unwrap().clone();
    let ssdp_map = ssdp_handle.join().unwrap_or_default();
    let mut arp_map = read_arp_table(Some(&iface));

    // 确保本机自己始终在列表中
    #[cfg(target_os = "macos")]
    if !arp_map.contains_key(&local_ip) {
        if let Some(self_mac) = get_self_mac_macos(&iface) {
            arp_map.insert(local_ip.clone(), self_mac);
        }
    }

    let mut all_ips: HashSet<String> = arp_map.keys().cloned().collect();
    all_ips.insert(local_ip.clone());

    let ip_list: Vec<String> = all_ips.into_iter().collect();
    let mdns_queue = Arc::new(Mutex::new(ip_list.clone()));
    let mdns_results = Arc::new(Mutex::new(HashMap::<String, String>::new()));

    let mut mdns_handles = Vec::new();
    for _ in 0..24 {
        let q = Arc::clone(&mdns_queue);
        let res = Arc::clone(&mdns_results);
        mdns_handles.push(thread::spawn(move || loop {
            let next_ip = {
                let mut guard = q.lock().unwrap();
                guard.pop()
            };
            let Some(ip) = next_ip else { break };
            if let Some(name) = mdns_reverse_lookup(&ip) {
                res.lock().unwrap().insert(ip, name);
            }
        }));
    }
    for h in mdns_handles {
        let _ = h.join();
    }
    let mdns_map = mdns_results.lock().unwrap().clone();

    let mut devices = Vec::new();
    for ip in ip_list {
        let Some(mac) = arp_map.get(&ip).cloned() else {
            continue;
        };
        let is_self = ip == local_ip;
        let is_gateway = ip == gateway_ip;
        let hostname = mdns_map.get(&ip).cloned();
        let latency_ms = ping_map.get(&ip).copied();
        let ping_alive = latency_ms.is_some() || is_self;
        let (mut dtype, mut default_name, mut vendor) =
            classify_device(&ip, &mac, hostname.as_deref(), is_self, is_gateway);

        if !is_self && !is_gateway {
            if let Some(ssdp) = ssdp_map.get(&ip) {
                if let Some(ref fname) = ssdp.friendly_name {
                    default_name = fname.clone();
                } else if let Some(ref model) = ssdp.model_name {
                    default_name = model.clone();
                }
                if let Some(hint) = ssdp.device_type_hint() {
                    if dtype == "unknown" || (dtype == "phone" && vendor.contains("未知")) || hint == "iot" {
                        dtype = hint;
                    }
                }
                if let Some(ref mfr) = ssdp.manufacturer {
                    if vendor.contains("未知") || vendor.contains("隐私") || vendor.contains("通用") {
                        vendor = mfr.clone();
                    }
                }
            }
        }

        devices.push(RawLanDevice {
            ip,
            mac: mac.clone(),
            vendor,
            hostname,
            device_type: dtype.to_string(),
            default_name,
            is_self,
            is_gateway,
            is_randomized_mac: is_randomized_mac(&mac),
            ping_alive,
            latency_ms,
        });
    }

    devices.sort_by_key(|d| {
        let last_octet = d
            .ip
            .split('.')
            .next_back()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(999);
        (
            if d.is_self { 0 } else if d.is_gateway { 1 } else { 2 },
            last_octet,
        )
    });

    devices
}

pub fn get_saved_wifi_password(ssid: &str) -> WifiPasswordResult {
    let trimmed = ssid.trim();
    if trimmed.is_empty() {
        return WifiPasswordResult {
            ssid: String::new(),
            password: None,
            error: Some("请先选择或输入当前连接的 Wi-Fi 名称".to_string()),
        };
    }

    #[cfg(target_os = "macos")]
    {
        let out = Command::new("security")
            .args(["find-generic-password", "-wa", trimmed])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let pw = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let (password, error) = if pw.is_empty() {
                    (None, Some("未在钥匙串中找到该 Wi-Fi 的密码记录".to_string()))
                } else {
                    (Some(pw), None)
                };
                WifiPasswordResult {
                    ssid: trimmed.to_string(),
                    password,
                    error,
                }
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                let msg = if err.contains("User interaction is not allowed") || err.contains("canceled") || err.contains("denied") {
                    "你取消了 macOS 钥匙串授权，无法读取保存的密码（也可直接在下方手动填写）".to_string()
                } else {
                    format!("未读取到「{}」的密码，请确认 Wi-Fi 名称是否正确", trimmed)
                };
                WifiPasswordResult {
                    ssid: trimmed.to_string(),
                    password: None,
                    error: Some(msg),
                }
            }
            Err(e) => WifiPasswordResult {
                ssid: trimmed.to_string(),
                password: None,
                error: Some(format!("调用系统钥匙串失败: {}", e)),
            },
        }
    }

    #[cfg(target_os = "windows")]
    {
        let arg = format!("name={}", trimmed);
        let out = Command::new("netsh")
            .args(["wlan", "show", "profile", &arg, "key=clear"])
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        let kl = k.to_lowercase();
                        if kl.contains("key content") || kl.contains("关键内容") {
                            let pw = v.trim().to_string();
                            return WifiPasswordResult {
                                ssid: trimmed.to_string(),
                                password: Some(pw),
                                error: None,
                            };
                        }
                    }
                }
                WifiPasswordResult {
                    ssid: trimmed.to_string(),
                    password: None,
                    error: Some(format!("未在系统记录中找到「{}」的密码", trimmed)),
                }
            }
            _ => WifiPasswordResult {
                ssid: trimmed.to_string(),
                password: None,
                error: Some("读取 Windows Wi-Fi 配置失败".to_string()),
            },
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    WifiPasswordResult {
        ssid: trimmed.to_string(),
        password: None,
        error: Some("当前系统暂不支持自动读取 Wi-Fi 密码".to_string()),
    }
}

pub fn run_health_check() -> HealthReport {
    let overview = get_network_overview();
    let gw_ip = overview.router.gateway_ip.clone();
    let signal_dbm = overview.signal_dbm;

    // 1. 测试到网关 3 次 Ping
    let mut gw_samples = Vec::new();
    for _ in 0..3 {
        if let Some(ms) = ping_host(&gw_ip, 800) {
            gw_samples.push(ms);
        }
    }
    let gateway_latency_ms = if gw_samples.is_empty() {
        None
    } else {
        Some((gw_samples.iter().sum::<u32>()) / (gw_samples.len() as u32))
    };

    // 2. 测试到外网常见 DNS/服务端 TCP 连通性
    let targets = ["223.5.5.5:53", "119.29.29.29:53", "1.1.1.1:53"];
    let mut ext_samples = Vec::new();
    for target in targets {
        if let Ok(addr) = target.parse::<SocketAddr>() {
            let start = Instant::now();
            if TcpStream::connect_timeout(&addr, Duration::from_millis(1200)).is_ok() {
                ext_samples.push(start.elapsed().as_millis().max(1) as u32);
            }
        }
    }
    let internet_latency_ms = if ext_samples.is_empty() {
        None
    } else {
        Some((ext_samples.iter().sum::<u32>()) / (ext_samples.len() as u32))
    };

    let wifi_bad = gateway_latency_ms.is_none()
        || gateway_latency_ms.unwrap_or(0) > 45
        || signal_dbm.map(|s| s < -75).unwrap_or(false);
    let inet_bad = internet_latency_ms.is_none();
    let inet_slow = internet_latency_ms.unwrap_or(0) > 120;

    let step1 = if gateway_latency_ms.is_none() {
        HealthStep {
            id: "wifi".to_string(),
            title: "1. 本机到路由器".to_string(),
            status: "error".to_string(),
            badge: "连接异常".to_string(),
            detail: format!("无法稳定连接路由器 {}，请确认已连上正确的 Wi-Fi。", gw_ip),
        }
    } else if wifi_bad {
        let sig_txt = signal_dbm.map(|s| format!("信号 {} dBm，", s)).unwrap_or_default();
        HealthStep {
            id: "wifi".to_string(),
            title: "1. 本机到路由器".to_string(),
            status: "warning".to_string(),
            badge: format!("偏弱 ({} ms)", gateway_latency_ms.unwrap()),
            detail: format!("{}到路由器延迟偏高，可能隔墙较多或距离路由器较远。", sig_txt),
        }
    } else {
        let sig_txt = signal_dbm.map(|s| format!("（信号 {} dBm）", s)).unwrap_or_default();
        HealthStep {
            id: "wifi".to_string(),
            title: "1. 本机到路由器".to_string(),
            status: "good".to_string(),
            badge: format!("极佳 ({} ms)", gateway_latency_ms.unwrap()),
            detail: format!("本机与路由器无线连接稳定{}，内网传输顺畅。", sig_txt),
        }
    };

    let step2 = if inet_bad {
        HealthStep {
            id: "internet".to_string(),
            title: "2. 路由器到外网宽带".to_string(),
            status: "error".to_string(),
            badge: "外网中断".to_string(),
            detail: "路由器本身能连上，但无法连通外部互联网（可能是光猫掉线或宽带故障）。".to_string(),
        }
    } else if inet_slow {
        HealthStep {
            id: "internet".to_string(),
            title: "2. 路由器到外网宽带".to_string(),
            status: "warning".to_string(),
            badge: format!("偏慢 ({} ms)", internet_latency_ms.unwrap()),
            detail: "外网可以访问但响应较慢，可能是运营商线路拥堵或后台有人大量下载。".to_string(),
        }
    } else {
        HealthStep {
            id: "internet".to_string(),
            title: "2. 路由器到外网宽带".to_string(),
            status: "good".to_string(),
            badge: format!("畅通 ({} ms)", internet_latency_ms.unwrap()),
            detail: "宽带出口与公共 DNS 解析均正常，网页和视频访问无阻。".to_string(),
        }
    };

    let step3 = if inet_bad || gateway_latency_ms.is_none() {
        HealthStep {
            id: "experience".to_string(),
            title: "3. 综合上网体验".to_string(),
            status: "error".to_string(),
            badge: "不可用".to_string(),
            detail: "外网暂不可达，建议优先按下方指引排查光猫或路由器。".to_string(),
        }
    } else if wifi_bad || inet_slow {
        HealthStep {
            id: "experience".to_string(),
            title: "3. 综合上网体验".to_string(),
            status: "warning".to_string(),
            badge: "偶有卡顿".to_string(),
            detail: "刷网页通常没问题，但视频会议或高清直播可能会卡顿。".to_string(),
        }
    } else {
        let rate_txt = overview
            .tx_rate_mbps
            .map(|r| format!("当前无线协商速率约 {} Mbps，", r))
            .unwrap_or_default();
        HealthStep {
            id: "experience".to_string(),
            title: "3. 综合上网体验".to_string(),
            status: "good".to_string(),
            badge: "流畅".to_string(),
            detail: format!("{}看 4K 视频、在线会议和日常使用都没有压力。", rate_txt),
        }
    };

    let (overall_status, summary_title, summary_advice) = if gateway_latency_ms.is_none() || inet_bad {
        (
            "error".to_string(),
            "家里 Wi-Fi 连着，但外网宽带暂时断了".to_string(),
            "建议排查顺序：1) 看光猫是否亮红灯；2) 拔插路由器/光猫电源等 30 秒再试；3) 确认宽带是否欠费并联系运营商。".to_string(),
        )
    } else if wifi_bad {
        (
            "warning".to_string(),
            "宽带本身没坏，主要是你当前位置 Wi-Fi 信号偏弱".to_string(),
            "建议：靠近路由器试试，或者在卧室/书房增加 Mesh 子路由；如果是墙体较多，连 2.4G Wi-Fi 会比 5G 更穿墙。".to_string(),
        )
    } else if inet_slow {
        (
            "warning".to_string(),
            "Wi-Fi 正常，但外网响应偏慢".to_string(),
            "建议先到「谁在用我的网」看看是不是有陌生设备或某台电脑正在满速下载。".to_string(),
        )
    } else {
        (
            "good".to_string(),
            "当前网络非常健康！".to_string(),
            "如果依然觉得某一个特定网站或 App 打开慢，通常是对方服务器或代理设置问题，与家里 Wi-Fi 无关。".to_string(),
        )
    };

    HealthReport {
        overall_status,
        summary_title,
        summary_advice,
        gateway_latency_ms,
        internet_latency_ms,
        signal_dbm,
        steps: vec![step1, step2, step3],
    }
}

pub fn classify_speed_mbps(mbps: f64) -> (String, String) {
    if mbps >= 100.0 {
        (
            "超清流畅".to_string(),
            "看 4K 视频、开高清视频会议、下载大文件都非常轻松。".to_string(),
        )
    } else if mbps >= 30.0 {
        (
            "日常流畅".to_string(),
            "刷短视频、看高清剧集和远程办公都很顺畅。".to_string(),
        )
    } else if mbps >= 10.0 {
        (
            "基本够用".to_string(),
            "聊微信、看网页没问题；多人同时看高清视频可能会略卡。".to_string(),
        )
    } else {
        (
            "网速偏慢".to_string(),
            "当前带宽较低，建议检查是否有设备正在满速下载，或靠近路由器试试。".to_string(),
        )
    }
}

/// 轻量真实下载测速（优先调用系统内置 curl 测量多节点下行吞吐，5 秒封顶）
pub fn run_speed_test() -> SpeedTestResult {
    let endpoints: [(&str, &str); 3] = [
        (
            "Cloudflare CDN",
            "https://speed.cloudflare.com/__down?bytes=8388608",
        ),
        (
            "阿里云开源镜像",
            "https://mirrors.aliyun.com/ubuntu-releases/24.04.1/ubuntu-24.04.1-live-server-amd64.iso",
        ),
        (
            "清华开源镜像",
            "https://mirrors.tuna.tsinghua.edu.cn/ubuntu-releases/24.04.1/ubuntu-24.04.1-live-server-amd64.iso",
        ),
    ];

    let sink = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut last_err = String::from("未能连接到测速节点，请检查外网连通性");

    for (source_name, url) in endpoints {
        let out = Command::new("curl")
            .args([
                "-sSL",
                "--connect-timeout",
                "3",
                "--max-time",
                "5",
                "-r",
                "0-8388607",
                "-o",
                sink,
                "-w",
                "%{speed_download} %{size_download} %{http_code}",
                url,
            ])
            .output();

        if let Ok(o) = out {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let cols: Vec<&str> = stdout.split_whitespace().collect();
            if cols.len() >= 3 {
                let speed_bps = cols[0].parse::<f64>().unwrap_or(0.0);
                let size_bytes = cols[1].parse::<f64>().unwrap_or(0.0);
                let http_code = cols[2].parse::<u16>().unwrap_or(0);

                if (200..400).contains(&http_code) && size_bytes >= 131_072.0 && speed_bps > 0.0 {
                    let mbps = ((speed_bps * 8.0 / 1_000_000.0) * 10.0).round() / 10.0;
                    let (quality_label, advice) = classify_speed_mbps(mbps);
                    return SpeedTestResult {
                        download_mbps: Some(mbps),
                        quality_label,
                        advice,
                        source: source_name.to_string(),
                        error: None,
                    };
                }
            }
            let err_txt = String::from_utf8_lossy(&o.stderr).trim().to_string();
            if !err_txt.is_empty() {
                last_err = err_txt;
            }
        }
    }

    SpeedTestResult {
        download_mbps: None,
        quality_label: "测速失败".to_string(),
        advice: "请先运行上方「一键网络体检」确认外网宽带是否正常连通。".to_string(),
        source: "多节点测速".to_string(),
        error: Some(last_err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_speed_mbps() {
        assert_eq!(classify_speed_mbps(150.5).0, "超清流畅");
        assert_eq!(classify_speed_mbps(48.0).0, "日常流畅");
        assert_eq!(classify_speed_mbps(15.0).0, "基本够用");
        assert_eq!(classify_speed_mbps(3.2).0, "网速偏慢");
    }

    #[test]
    #[ignore]
    fn live_network_smoke_test() {
        let ov = get_network_overview();
        println!("OVERVIEW: iface={} ip={} gw={} brand={} sig={:?}", ov.interface_name, ov.local_ip, ov.router.gateway_ip, ov.router.brand, ov.signal_dbm);
        assert!(!ov.local_ip.is_empty());
        assert!(!ov.router.gateway_ip.is_empty());

        let devs = scan_lan_devices();
        println!("DEVICES FOUND: {}", devs.len());
        for d in devs.iter().take(10) {
            println!("  {} | {} | {} | {} | host={:?}", d.ip, d.mac, d.default_name, d.vendor, d.hostname);
        }
        assert!(!devs.is_empty());

        let health = run_health_check();
        println!("HEALTH: {} - {}", health.overall_status, health.summary_title);
    }
}
