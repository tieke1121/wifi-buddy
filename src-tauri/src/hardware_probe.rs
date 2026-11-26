use crate::cmd_util::hidden_command;
#[cfg(target_os = "windows")]
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct WebFingerprint {
    pub title: Option<String>,
    pub server: Option<String>,
    pub admin_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledPrinter {
    pub name: String,
    pub port_name: String,
    pub host_address: String,
}

#[derive(Debug, Clone, Default)]
pub struct ProbeSummary {
    pub open_ports: Vec<u16>,
    pub web: WebFingerprint,
    pub mdns_services: Vec<String>,
}

/// 按指定超时并发探测单个 IP 的常用硬件服务端口
pub fn scan_common_ports_with_timeout(ip: &str, timeout_ms: u64) -> Vec<u16> {
    let candidates = [80u16, 443, 631, 9100, 8123, 554];
    let mut handles = Vec::with_capacity(candidates.len());
    for port in candidates {
        let addr_str = format!("{}:{}", ip, port);
        handles.push(thread::spawn(move || {
            let addr: SocketAddr = addr_str.parse().ok()?;
            if TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok() {
                Some(port)
            } else {
                None
            }
        }));
    }
    let mut open = Vec::new();
    for h in handles {
        if let Ok(Some(port)) = h.join() {
            open.push(port);
        }
    }
    open.sort_unstable();
    open
}

/// 并发探测单个 IP 的常用硬件服务端口（默认超时）
pub fn scan_common_ports(ip: &str) -> Vec<u16> {
    scan_common_ports_with_timeout(ip, 450)
}

/// 通过打印常用端口轻触唤醒可能处于 Wi-Fi 休眠状态的设备并刷新 ARP
pub fn wake_host_tcp(ip: &str, timeout_ms: u64) -> bool {
    let ports = [631u16, 9100, 80, 443];
    let mut handles = Vec::with_capacity(ports.len());
    for port in ports {
        let addr_str = format!("{}:{}", ip, port);
        handles.push(thread::spawn(move || {
            let Ok(addr) = addr_str.parse::<SocketAddr>() else {
                return false;
            };
            TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok()
        }));
    }
    let mut any_ok = false;
    for h in handles {
        if let Ok(true) = h.join() {
            any_ok = true;
        }
    }
    any_ok
}

fn extract_tag_content(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = lower.find(&open)? + open.len();
    let rel_end = lower[start..].find(&close)?;
    let raw = html[start..start + rel_end].trim();
    let cleaned: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn extract_meta_refresh_paths(html: &str) -> Vec<String> {
    let lower = html.to_lowercase();
    let mut paths = Vec::new();
    for (idx, _) in lower.match_indices("url=") {
        let rest = html[idx + 4..]
            .trim_start()
            .trim_start_matches(['\'', '"'])
            .trim_start();
        if let Some(end) = rest.find(['\'', '"', '>', ' ', ';']) {
            let path = rest[..end].trim();
            if !path.is_empty() && !path.starts_with("http") {
                let clean = path.trim_start_matches('.').to_string();
                if !paths.contains(&clean) {
                    paths.push(clean);
                }
            }
        }
    }
    // 更长的路径通常为根目录完整跳转地址（如 /PRESENTATION/HTML/TOP/INDEX.HTML）
    paths.sort_by_key(|p| std::cmp::Reverse(p.len()));
    paths
}

fn fetch_with_curl(url: &str) -> Option<(Option<String>, String)> {
    let out = hidden_command("curl")
        .args([
            "-skL",
            "--connect-timeout",
            "2",
            "--max-time",
            "3",
            "-D",
            "-",
            url,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut server = None;
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("server") {
                server = Some(v.trim().to_string());
            }
        }
    }
    Some((server, text))
}

/// 读取硬件 Web 首页的标题、Server 标识与可访问的管理地址
pub fn probe_web_fingerprint(ip: &str, open_ports: &[u16]) -> WebFingerprint {
    let has_https = open_ports.contains(&443);
    let has_http = open_ports.contains(&80);
    let has_ha = open_ports.contains(&8123);

    if has_ha && !has_http && !has_https {
        return WebFingerprint {
            title: Some("Home Assistant".to_string()),
            server: None,
            admin_url: Some(format!("http://{}:8123", ip)),
        };
    }

    if !has_http && !has_https {
        return WebFingerprint::default();
    }

    let primary_url = if has_https {
        format!("https://{}/", ip)
    } else {
        format!("http://{}/", ip)
    };

    let mut server = None;
    let mut title = None;

    if let Some((srv, body)) = fetch_with_curl(&primary_url) {
        server = srv;
        title = extract_tag_content(&body, "title");
        if title.is_none() {
            for rel in extract_meta_refresh_paths(&body).into_iter().take(2) {
                let next_url = format!(
                    "{}{}",
                    primary_url.trim_end_matches('/'),
                    if rel.starts_with('/') {
                        rel
                    } else {
                        format!("/{}", rel)
                    }
                );
                if let Some((srv2, body2)) = fetch_with_curl(&next_url) {
                    if server.is_none() {
                        server = srv2;
                    }
                    title = extract_tag_content(&body2, "title");
                    if title.is_some() {
                        break;
                    }
                }
            }
        }
    } else if has_http {
        if let Ok(addr) = format!("{}:80", ip).parse::<SocketAddr>() {
            if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(450)) {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let req = format!(
                    "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                    ip
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
                    let text = String::from_utf8_lossy(&buf[..total]).to_string();
                    title = extract_tag_content(&text, "title");
                }
            }
        }
    }

    WebFingerprint {
        title,
        server,
        admin_url: Some(primary_url),
    }
}

/// 并发浏览局域网 DNS-SD 服务（打印机与智能家居中枢）
pub fn discover_dnssd_services() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return map;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(220)));

    let queries = [
        "_ipp._tcp.local",
        "_ipps._tcp.local",
        "_home-assistant._tcp.local",
        "_airplay._tcp.local",
        "_googlecast._tcp.local",
    ];

    for qname in queries {
        let mut pkt = Vec::with_capacity(64);
        pkt.extend_from_slice(&[0x22, 0x33, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        for label in qname.split('.') {
            pkt.push(label.len() as u8);
            pkt.extend_from_slice(label.as_bytes());
        }
        pkt.push(0x00);
        pkt.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]);
        let _ = sock.send_to(&pkt, "224.0.0.251:5353");
    }

    let deadline = Instant::now() + Duration::from_millis(650);
    let mut buf = [0u8; 2048];
    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, addr)) => {
                let ip = addr.ip().to_string();
                let ascii = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                let entry = map.entry(ip).or_default();
                for tag in ["_ipp._tcp", "_ipps._tcp", "_home-assistant", "_airplay", "_googlecast"] {
                    if ascii.contains(tag) && !entry.iter().any(|x| x == tag) {
                        entry.push(tag.to_string());
                    }
                }
            }
            Err(_) => break,
        }
    }
    map
}

/// 并发探测活跃局域网设备的端口与 Web 指纹（对重点已知设备使用更长超时）
pub fn probe_active_devices_with_hints(
    ips: &[String],
    gateway_ip: &str,
    local_ip: &str,
    priority_ips: &HashSet<String>,
) -> HashMap<String, ProbeSummary> {
    let dnssd_handle = thread::spawn(discover_dnssd_services);
    let targets: Vec<String> = ips
        .iter()
        .filter(|ip| ip.as_str() != gateway_ip && ip.as_str() != local_ip)
        .cloned()
        .collect();

    let queue = Arc::new(Mutex::new(targets));
    let results = Arc::new(Mutex::new(HashMap::<String, ProbeSummary>::new()));
    let priority_set = Arc::new(priority_ips.clone());

    let mut handles = Vec::new();
    for _ in 0..16 {
        let q = Arc::clone(&queue);
        let res = Arc::clone(&results);
        let prio = Arc::clone(&priority_set);
        handles.push(thread::spawn(move || loop {
            let next_ip = {
                let mut g = q.lock().unwrap();
                g.pop()
            };
            let Some(ip) = next_ip else { break };
            let is_priority = prio.contains(&ip);
            let timeout_ms = if is_priority { 1200 } else { 450 };
            let open_ports = scan_common_ports_with_timeout(&ip, timeout_ms);
            let should_probe_web = open_ports.iter().any(|p| matches!(p, 631 | 9100 | 8123 | 554))
                || (is_priority && (open_ports.contains(&80) || open_ports.contains(&443)));
            let web = if should_probe_web {
                probe_web_fingerprint(&ip, &open_ports)
            } else {
                WebFingerprint::default()
            };
            res.lock().unwrap().insert(
                ip,
                ProbeSummary {
                    open_ports,
                    web,
                    mdns_services: Vec::new(),
                },
            );
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    let dnssd_map = dnssd_handle.join().unwrap_or_default();
    let mut final_map = results.lock().unwrap().clone();
    for (ip, svcs) in dnssd_map {
        final_map.entry(ip).or_default().mdns_services = svcs;
    }
    final_map
}

/// 并发探测活跃局域网设备的端口与 Web 指纹
#[allow(dead_code)]
pub fn probe_active_devices(
    ips: &[String],
    gateway_ip: &str,
    local_ip: &str,
) -> HashMap<String, ProbeSummary> {
    probe_active_devices_with_hints(ips, gateway_ip, local_ip, &HashSet::new())
}

#[cfg(target_os = "windows")]
#[derive(Debug, Deserialize)]
struct WinPrinterRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "PortName")]
    port_name: String,
    #[serde(rename = "Host")]
    host: Option<String>,
}

/// 查询当前操作系统已经安装的打印机列表
pub fn list_installed_printers() -> Vec<InstalledPrinter> {
    #[cfg(target_os = "windows")]
    {
        let script = concat!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; ",
            "$list = @(Get-Printer -ErrorAction SilentlyContinue | ForEach-Object { ",
            "$p = $_; $h = ''; ",
            "try { $pt = Get-PrinterPort -Name $p.PortName -ErrorAction SilentlyContinue; if ($pt -and $pt.PrinterHostAddress) { $h = [string]$pt.PrinterHostAddress } } catch {}; ",
            "[PSCustomObject]@{ Name = [string]$p.Name; PortName = [string]$p.PortName; Host = $h } ",
            "}); ",
            "$list | ConvertTo-Json -Compress"
        );
        let Ok(out) = hidden_command("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
            .output()
        else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() || text == "null" {
            return Vec::new();
        }
        let rows: Vec<WinPrinterRow> = if text.starts_with('[') {
            serde_json::from_str(&text).unwrap_or_default()
        } else if text.starts_with('{') {
            serde_json::from_str::<WinPrinterRow>(&text)
                .map(|r| vec![r])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        return rows
            .into_iter()
            .map(|r| InstalledPrinter {
                name: r.name,
                port_name: r.port_name,
                host_address: r.host.unwrap_or_default(),
            })
            .collect();
    }

    #[cfg(target_os = "macos")]
    {
        let Ok(out) = hidden_command("lpstat").args(["-v"]).output() else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let mut list = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(rest) = line.strip_prefix("device for ") {
                if let Some((name, uri)) = rest.split_once(':') {
                    list.push(InstalledPrinter {
                        name: name.trim().to_string(),
                        port_name: uri.trim().to_string(),
                        host_address: uri.trim().to_string(),
                    });
                }
            }
        }
        return list;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Vec::new()
    }
}

pub fn find_matching_installed_printer(
    installed: &[InstalledPrinter],
    ip: &str,
    model_hint: Option<&str>,
) -> Option<InstalledPrinter> {
    let mut model_tokens: HashSet<String> = HashSet::new();
    if let Some(m) = model_hint {
        for tok in m.split_whitespace() {
            let clean = tok.trim().to_lowercase();
            if clean.len() >= 4 && clean != "series" && clean != "printer" {
                model_tokens.insert(clean);
            }
        }
    }

    for p in installed {
        if p.host_address == ip || p.port_name.contains(ip) {
            return Some(p.clone());
        }
        let name_lower = p.name.to_lowercase();
        if !model_tokens.is_empty() && model_tokens.iter().any(|t| name_lower.contains(t)) {
            return Some(p.clone());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn encode_utf16le_base64(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::with_capacity(input.len() * 2);
    for unit in input.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(target_os = "windows")]
fn friendly_win_printer_error(raw_err: &str, ip: &str, uac_canceled: bool) -> String {
    if uac_canceled {
        return format!(
            "你取消了 Windows 管理员权限弹窗，暂时无法自动安装。重新点击上方按钮并在弹窗中点「是」，或点击「打开系统打印机设置」使用 IP（{}）手动添加。",
            ip
        );
    }
    let lower = raw_err.to_lowercase();
    if lower.contains("0x80070005")
        || lower.contains("permissiondenied")
        || lower.contains("access")
        || raw_err.contains("拒绝")
    {
        return format!(
            "系统限制了当前操作权限，无法直接自动写入打印队列。请点击下方「打开系统打印机设置」，选择「添加设备 → 手动添加」并输入 IP（{}），或直接下载厂商官方驱动包一键安装。",
            ip
        );
    }
    if lower.contains("426")
        || lower.contains("ssl")
        || lower.contains("tls")
        || lower.contains("certificate")
        || lower.contains("ipp")
        || lower.contains("0x80070490")
    {
        return format!(
            "该打印机启用了自签名加密连接，Windows 通用驱动暂无法直接自动绑定。建议点击「下载官方完整驱动（含扫描）」直接安装，或在「系统打印机设置」中让 Windows 自动搜索添加（IP：{}）。",
            ip
        );
    }
    format!(
        "未能通过自动向导完成添加。建议点击下方「打开系统打印机设置」搜索添加（IP：{}），或直接「下载官方完整驱动（含扫描）」一键安装。",
        ip
    )
}

/// 调用操作系统官方打印机安装工具将局域网打印机添加到本机
pub fn install_system_printer_os(
    ip: &str,
    display_name: &str,
    has_631: bool,
    has_9100: bool,
) -> Result<String, String> {
    let existing = list_installed_printers();
    if let Some(found) = find_matching_installed_printer(&existing, ip, Some(display_name)) {
        return Ok(format!("当前电脑已安装「{}」，可以直接打印。", found.name));
    }

    #[cfg(target_os = "windows")]
    {
        let _ = has_9100;
        let safe_name = display_name
            .replace('\'', "")
            .replace('"', "")
            .trim()
            .to_string();
        let printer_name = if safe_name.is_empty() {
            format!("Wi-Fi 打印机 ({})", ip)
        } else {
            safe_name
        };

        let mut ipp_candidates = Vec::new();
        if has_631 {
            ipp_candidates.push(format!("http://{}:631/ipp/print", ip));
            ipp_candidates.push(format!("https://{}:631/ipp/print", ip));
        } else {
            ipp_candidates.push(format!("http://{}:631/ipp/print", ip));
        }

        let mut last_err = String::new();
        let mut need_elevation = false;
        for ipp_url in &ipp_candidates {
            let cmd = format!(
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Add-Printer -Name '{}' -IppURL '{}' -ErrorAction Stop",
                printer_name, ipp_url
            );
            if let Ok(out) = hidden_command("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &cmd])
                .output()
            {
                if out.status.success() {
                    let after = list_installed_printers();
                    if find_matching_installed_printer(&after, ip, Some(&printer_name)).is_some() {
                        return Ok(format!("已成功将「{}」添加到 Windows 打印机列表。", printer_name));
                    }
                }
                let err_txt = String::from_utf8_lossy(&out.stderr).trim().to_string();
                if err_txt.contains("0x80070005")
                    || err_txt.to_lowercase().contains("permissiondenied")
                    || err_txt.contains("拒绝")
                {
                    need_elevation = true;
                }
                if !err_txt.is_empty() {
                    last_err = err_txt;
                }
            }
        }

        if need_elevation {
            let err_log = std::env::temp_dir().join("wifi_buddy_printer_install.log");
            let err_log_str = err_log.to_string_lossy().replace('\'', "''");
            let _ = std::fs::remove_file(&err_log);

            let mut elevated_lines = vec![
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8".to_string(),
                "$installed = $false".to_string(),
                "$lastErr = ''".to_string(),
            ];
            for ipp_url in &ipp_candidates {
                elevated_lines.push(format!(
                    "if (-not $installed) {{ try {{ Add-Printer -Name '{}' -IppURL '{}' -ErrorAction Stop; $installed = $true }} catch {{ $lastErr = $_.Exception.Message }} }}",
                    printer_name, ipp_url
                ));
            }
            elevated_lines.push(format!(
                "if (-not $installed) {{ Set-Content -Path '{}' -Value $lastErr -Encoding UTF8; exit 1 }}",
                err_log_str
            ));
            let inner_script = elevated_lines.join("; ");
            let encoded = encode_utf16le_base64(&inner_script);

            let uac_wrapper = format!(
                "$p = Start-Process powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand','{}'; exit $p.ExitCode",
                encoded
            );
            let uac_res = hidden_command("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &uac_wrapper])
                .output();

            let uac_canceled = match &uac_res {
                Ok(o) if o.status.success() => {
                    let after = list_installed_printers();
                    if find_matching_installed_printer(&after, ip, Some(&printer_name)).is_some() {
                        let _ = std::fs::remove_file(&err_log);
                        return Ok(format!("已成功添加「{}」，现在可以直接打印了。", printer_name));
                    }
                    false
                }
                Ok(o) => {
                    let stderr_str = String::from_utf8_lossy(&o.stderr).to_lowercase();
                    stderr_str.contains("canceled") || stderr_str.contains("取消") || !err_log.exists()
                }
                Err(_) => true,
            };

            let elevated_err = std::fs::read_to_string(&err_log).unwrap_or_default();
            let _ = std::fs::remove_file(&err_log);
            let combined_err = if elevated_err.trim().is_empty() {
                last_err.as_str()
            } else {
                elevated_err.trim()
            };
            return Err(friendly_win_printer_error(combined_err, ip, uac_canceled));
        }

        return Err(friendly_win_printer_error(&last_err, ip, false));
    }

    #[cfg(target_os = "macos")]
    {
        let queue_name: String = display_name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let queue_name = if queue_name.trim_matches('_').is_empty() {
            format!("Printer_{}", ip.replace('.', "_"))
        } else {
            queue_name
        };
        let uri = if has_631 {
            format!("ipp://{}/ipp/print", ip)
        } else if has_9100 {
            format!("socket://{}:9100", ip)
        } else {
            format!("ipp://{}/ipp/print", ip)
        };
        let out = hidden_command("lpadmin")
            .args(["-p", &queue_name, "-E", "-v", &uri, "-m", "everywhere"])
            .output()
            .map_err(|e| format!("调用 macOS 打印配置工具失败: {}", e))?;
        if out.status.success() {
            return Ok(format!("已将「{}」添加到 macOS 打印机列表。", display_name));
        }
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("添加打印机失败：{}，请尝试在系统设置中手动添加 IP {}", err, ip));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (ip, display_name, has_631, has_9100);
        Err("当前系统暂不支持自动添加打印机".to_string())
    }
}

fn parse_valid_lan_ipv4(candidate: &str) -> Option<String> {
    let addr: Ipv4Addr = candidate.trim().parse().ok()?;
    if addr.is_loopback() || addr.is_unspecified() || addr.is_link_local() || addr.is_multicast() {
        return None;
    }
    Some(addr.to_string())
}

fn extract_ipv4_from_text(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let slice = text[start..i].trim_matches('.');
            if let Some(ip) = parse_valid_lan_ipv4(slice) {
                return Some(ip);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// 从已安装的打印机记录中提取可识别的局域网 IPv4 地址
pub fn extract_printer_ipv4(printer: &InstalledPrinter) -> Option<String> {
    if let Some(ip) = parse_valid_lan_ipv4(&printer.host_address) {
        return Some(ip);
    }
    extract_ipv4_from_text(&printer.port_name)
}

/// 从操作系统已安装的打印机列表中收集去重后的局域网 IPv4 地址
pub fn extract_network_printer_ips(installed: &[InstalledPrinter]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ips = Vec::new();
    for p in installed {
        if let Some(ip) = extract_printer_ipv4(p) {
            if seen.insert(ip.clone()) {
                ips.push(ip);
            }
        }
    }
    ips
}

/// 打开操作系统原生打印机设置页面
pub fn open_system_printer_settings_os() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        hidden_command("cmd")
            .args(["/C", "start", "", "ms-settings:printers"])
            .spawn()
            .map_err(|e| format!("打开系统设置失败: {}", e))?;
        return Ok("已为你打开 Windows「打印机和扫描仪」设置页。".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        hidden_command("open")
            .args(["x-apple.systempreferences:com.apple.Print-Scan-Settings.extension"])
            .spawn()
            .map_err(|e| format!("打开系统设置失败: {}", e))?;
        return Ok("已为你打开 macOS「打印机与扫描仪」设置页。".to_string());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err("当前系统暂不支持快捷跳转打印机设置".to_string())
    }
}

/// 打开系统打印机首选项/维护面板（供进入官方驱动的喷头清洗/深度清洗入口）
pub fn open_printer_maintenance_os(
    ip: &str,
    printer_name: Option<&str>,
    display_name: Option<&str>,
) -> Result<String, String> {
    let target_name = printer_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            let installed = list_installed_printers();
            find_matching_installed_printer(&installed, ip, display_name).map(|p| p.name)
        });

    #[cfg(target_os = "windows")]
    {
        let Some(name) = target_name else {
            return Err(format!(
                "当前电脑尚未关联到该打印机的打印队列（IP：{}）。请先点击「一键添加到本机打印」或安装官方驱动后再打开维护面板。",
                ip
            ));
        };
        let arg = format!("printui.dll,PrintUIEntry");
        hidden_command("rundll32")
            .args([&arg, "/e", "/n", &name])
            .spawn()
            .map_err(|e| format!("打开打印首选项失败: {}", e))?;
        return Ok(format!("已为你打开「{}」的打印首选项窗口。", name));
    }

    #[cfg(target_os = "macos")]
    {
        let _ = target_name;
        let _ = open_system_printer_settings_os()?;
        return Ok("已打开 macOS「打印机与扫描仪」，选择目标打印机后点击「选项与耗材 → 实用工具」即可执行清洗。".to_string());
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = target_name;
        Err("当前系统暂不支持快捷打开打印维护面板".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_and_meta_refresh() {
        let redirect_html = "<script>if (-1 < Ret) { document.write(\"<meta http-equiv = 'refresh' content='0; URL= ./HTML/TOP/INDEX.HTML'>\"); } else { document.write(\"<meta http-equiv='refresh' content='0; URL= ./PRESENTATION/HTML/TOP/INDEX.HTML'>\"); }</script>";
        let paths = extract_meta_refresh_paths(redirect_html);
        assert_eq!(
            paths.first().map(String::as_str),
            Some("/PRESENTATION/HTML/TOP/INDEX.HTML")
        );
        let page = "<html><head><title>  L3250   Series </title></head></html>";
        assert_eq!(extract_tag_content(page, "title").as_deref(), Some("L3250 Series"));
    }

    #[test]
    fn test_find_matching_installed_printer() {
        let list = vec![InstalledPrinter {
            name: "EPSON L3250 Series".to_string(),
            port_name: "IP_192.168.5.67".to_string(),
            host_address: "192.168.5.67".to_string(),
        }];
        assert!(find_matching_installed_printer(&list, "192.168.5.67", None).is_some());
        assert!(find_matching_installed_printer(&list, "192.168.5.99", Some("L3250 Series")).is_some());
        assert!(find_matching_installed_printer(&list, "192.168.5.99", Some("LaserJet M1005")).is_none());
    }

    #[test]
    fn test_extract_network_printer_ips() {
        let list = vec![
            InstalledPrinter {
                name: "Microsoft Print to PDF".to_string(),
                port_name: "PORTPROMPT:".to_string(),
                host_address: "".to_string(),
            },
            InstalledPrinter {
                name: "EPSON L3250 Series".to_string(),
                port_name: "IP_192.168.5.67".to_string(),
                host_address: "".to_string(),
            },
            InstalledPrinter {
                name: "HP LaserJet".to_string(),
                port_name: "http://192.168.5.88:631/ipp/print".to_string(),
                host_address: "192.168.5.88".to_string(),
            },
            InstalledPrinter {
                name: "Local USB".to_string(),
                port_name: "USB001".to_string(),
                host_address: "127.0.0.1".to_string(),
            },
        ];
        let ips = extract_network_printer_ips(&list);
        assert_eq!(ips, vec!["192.168.5.67".to_string(), "192.168.5.88".to_string()]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_friendly_win_printer_error_and_base64() {
        let msg = friendly_win_printer_error("PermissionDenied: HRESULT 0x80070005", "192.168.5.67", false);
        assert!(msg.contains("192.168.5.67"));
        assert!(!msg.contains("HRESULT"));
        assert!(!msg.contains("MSFT_Printer"));

        let canceled = friendly_win_printer_error("", "192.168.5.67", true);
        assert!(canceled.contains("取消"));
        assert_eq!(encode_utf16le_base64("hi"), "aABpAA==");
    }
}
