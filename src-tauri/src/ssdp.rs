use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SsdpInfo {
    pub friendly_name: Option<String>,
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub device_type: Option<String>,
}

impl SsdpInfo {
    /// 根据 UPnP deviceType / friendlyName / modelName 推断设备类型
    pub fn device_type_hint(&self) -> Option<&'static str> {
        let combined = format!(
            "{} {} {}",
            self.device_type.as_deref().unwrap_or(""),
            self.friendly_name.as_deref().unwrap_or(""),
            self.model_name.as_deref().unwrap_or("")
        )
        .to_lowercase();

        if combined.contains("internetgatewaydevice")
            || combined.contains("wandevice")
            || combined.contains("router")
        {
            return Some("router");
        }
        if combined.contains("mediarenderer")
            || combined.contains("mediaserver")
            || combined.contains("printer")
            || combined.contains("scanner")
            || combined.contains("tv")
            || combined.contains("speaker")
            || combined.contains("xiaomi")
            || combined.contains("sonos")
            || combined.contains("nas")
            || combined.contains("synology")
            || combined.contains("camera")
        {
            return Some("iot");
        }
        if self.friendly_name.is_some() || self.model_name.is_some() {
            return Some("iot");
        }
        None
    }
}

/// 发送 SSDP M-SEARCH 多播探测局域网智能设备（电视、音箱、打印机、NAS、路由器等）
pub fn discover_ssdp_devices() -> HashMap<String, SsdpInfo> {
    let mut locations: HashMap<String, String> = HashMap::new();

    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return HashMap::new();
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(250)));

    let req = concat!(
        "M-SEARCH * HTTP/1.1\r\n",
        "HOST: 239.255.255.250:1900\r\n",
        "MAN: \"ssdp:discover\"\r\n",
        "MX: 1\r\n",
        "ST: ssdp:all\r\n\r\n"
    );
    let _ = sock.send_to(req.as_bytes(), "239.255.255.250:1900");
    thread::sleep(Duration::from_millis(50));
    let _ = sock.send_to(req.as_bytes(), "239.255.255.250:1900");

    let deadline = Instant::now() + Duration::from_millis(950);
    let mut buf = [0u8; 2048];
    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, addr)) => {
                let ip = addr.ip().to_string();
                let resp = String::from_utf8_lossy(&buf[..n]);
                for line in resp.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("location") {
                            let url = v.trim().to_string();
                            if url.starts_with("http://") {
                                locations.entry(ip.clone()).or_insert(url);
                            }
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }

    let queue = Arc::new(Mutex::new(locations.into_iter().collect::<Vec<_>>()));
    let results = Arc::new(Mutex::new(HashMap::<String, SsdpInfo>::new()));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let q = Arc::clone(&queue);
        let res = Arc::clone(&results);
        handles.push(thread::spawn(move || loop {
            let next = {
                let mut g = q.lock().unwrap();
                g.pop()
            };
            let Some((ip, url)) = next else { break };
            if let Some(info) = fetch_upnp_xml(&url) {
                res.lock().unwrap().insert(ip, info);
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    results.lock().unwrap().clone()
}

fn fetch_upnp_xml(url: &str) -> Option<SsdpInfo> {
    let rest = url.strip_prefix("http://")?;
    let (host_port, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    let addr_str = if host_port.contains(':') {
        host_port.to_string()
    } else {
        format!("{}:80", host_port)
    };
    let addr: SocketAddr = addr_str.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(550)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(450)));

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: wifi-buddy/0.2\r\n\r\n",
        path, host_port
    );
    stream.write_all(req.as_bytes()).ok()?;

    let mut buf = vec![0u8; 24576];
    let mut total = 0;
    while total < buf.len() {
        match stream.read(&mut buf[total..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
    let text = String::from_utf8_lossy(&buf[..total]);
    let friendly_name = extract_xml_tag(&text, "friendlyName");
    let manufacturer = extract_xml_tag(&text, "manufacturer");
    let model_name = extract_xml_tag(&text, "modelName");
    let device_type = extract_xml_tag(&text, "deviceType");

    if friendly_name.is_none() && manufacturer.is_none() && model_name.is_none() {
        return None;
    }

    Some(SsdpInfo {
        friendly_name,
        manufacturer,
        model_name,
        device_type,
    })
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

pub fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let lower = xml.to_lowercase();
    let open = format!("<{}>", tag.to_lowercase());
    let close = format!("</{}>", tag.to_lowercase());
    let start_idx = lower.find(&open)? + open.len();
    let rel_end = lower[start_idx..].find(&close)?;
    let val = decode_xml_entities(xml[start_idx..start_idx + rel_end].trim());
    let val = val.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_xml_tag_and_hint() {
        let sample = "<Device><DeviceType>urn:schemas-upnp-org:device:MediaRenderer:1</DeviceType><FriendlyName>客厅小米电视 &amp; 音响</FriendlyName><manufacturer>Xiaomi</manufacturer><modelName>L65M7</modelName></Device>";
        assert_eq!(
            extract_xml_tag(sample, "friendlyName").as_deref(),
            Some("客厅小米电视 & 音响")
        );
        assert_eq!(extract_xml_tag(sample, "manufacturer").as_deref(), Some("Xiaomi"));
        assert_eq!(extract_xml_tag(sample, "modelName").as_deref(), Some("L65M7"));

        let info = SsdpInfo {
            friendly_name: extract_xml_tag(sample, "friendlyName"),
            manufacturer: extract_xml_tag(sample, "manufacturer"),
            model_name: extract_xml_tag(sample, "modelName"),
            device_type: extract_xml_tag(sample, "deviceType"),
        };
        assert_eq!(info.device_type_hint(), Some("iot"));
    }
}
