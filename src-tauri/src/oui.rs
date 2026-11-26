/// 规范化 MAC 地址为小写两位冒号分隔格式（如 `c4:09:d9:69:65:7d`）
pub fn normalize_mac(raw: &str) -> Option<String> {
    let sep = if raw.contains(':') {
        ':'
    } else if raw.contains('-') {
        '-'
    } else {
        return None;
    };
    let parts: Vec<&str> = raw.trim().split(sep).collect();
    if parts.len() != 6 {
        return None;
    }
    let mut normalized = Vec::with_capacity(6);
    for part in parts {
        let val = u8::from_str_radix(part, 16).ok()?;
        normalized.push(format!("{:02x}", val));
    }
    Some(normalized.join(":"))
}

/// 检查是否为本地管理位（随机/隐私 MAC 地址）
pub fn is_randomized_mac(mac: &str) -> bool {
    mac.split(':')
        .next()
        .and_then(|first| u8::from_str_radix(first, 16).ok())
        .map(|b| (b & 0x02) != 0)
        .unwrap_or(false)
}

/// 根据 MAC 前三段 OUI 查找常见设备/网卡厂商
pub fn lookup_vendor(mac: &str) -> Option<&'static str> {
    let prefix = mac.get(0..8)?;
    match prefix {
        // 中兴 ZTE
        "c4:92:d9" | "00:19:c6" | "00:1e:73" | "00:22:93" | "34:4b:50" | "70:9f:2d"
        | "98:f5:37" | "c8:64:c7" | "d4:37:d7" | "ec:1d:7f" => Some("中兴 ZTE"),
        // 苹果 Apple
        "00:03:93" | "00:1b:63" | "00:25:00" | "14:5a:fc" | "28:cf:e9" | "3c:06:30"
        | "5c:8a:ae" | "60:f8:1d" | "70:56:81" | "84:2f:57" | "a4:83:e7" | "ac:bc:32"
        | "bc:52:b7" | "c8:ff:28" | "d0:81:7a" | "f0:9e:4a" | "f4:5c:89" | "f8:ff:c2" => {
            Some("Apple")
        }
        // 小米 Xiaomi
        "04:cf:8c" | "14:f6:5a" | "28:6c:07" | "34:ce:00" | "50:64:2b" | "64:09:80"
        | "74:23:44" | "78:11:dc" | "8c:be:be" | "98:fa:e3" | "b4:b5:b6" | "c4:0b:cb"
        | "d4:97:0b" | "f0:b4:29" | "f8:a4:5f" => Some("小米 Xiaomi"),
        // 华为 Huawei / 荣耀
        "00:e0:fc" | "04:f9:38" | "10:47:80" | "24:09:95" | "28:31:52" | "34:6b:d3"
        | "48:46:fb" | "54:25:ea" | "70:72:3c" | "88:28:b3" | "ac:e2:15" | "cc:a2:23"
        | "e0:24:7f" | "f4:e3:fb" => Some("华为 Huawei"),
        // TP-Link
        "14:cc:20" | "18:d6:c7" | "30:b4:9e" | "50:c7:bf" | "60:e3:27" | "74:da:88"
        | "84:16:f9" | "98:da:c4" | "b0:4e:26" | "c0:25:e9" | "d8:07:b6" | "e8:de:27"
        | "f4:f2:6d" => Some("TP-Link"),
        // 华硕 ASUS
        "04:92:26" | "10:bf:48" | "1c:87:2c" | "2c:fd:a1" | "38:2c:4a" | "50:46:5d"
        | "70:8b:cd" | "ac:9e:17" | "bc:ee:7b" | "d8:50:e6" | "f0:79:59" => Some("华硕 ASUS"),
        // Intel / PC 无线网卡
        "00:15:00" | "24:41:8c" | "34:13:e8" | "48:51:b7" | "5c:e0:c5" | "74:4c:a1"
        | "80:86:f2" | "8c:8d:28" | "a4:34:d9" | "b4:b6:76" | "dc:53:60" | "f8:a2:d6" => {
            Some("Intel 网卡")
        }
        // Realtek / 瑞昱（PC/电视盒子常见）
        "00:e0:4c" | "24:b2:b9" | "48:02:2a" | "50:2b:73" | "50:c2:e8" | "80:1f:02"
        | "c8:94:02" => Some("Realtek"),
        // 乐鑫 Espressif（智能家居插座/灯具/传感器）
        "18:fe:34" | "24:0a:c4" | "30:ae:a4" | "3c:71:bf" | "4c:11:ae" | "5c:cf:7f"
        | "84:0d:8e" | "a4:cf:12" | "bc:dd:c2" | "cc:50:e3" | "ec:fa:bc" => {
            Some("智能家居 (Espressif)")
        }
        // 海康威视 / 大华（摄像头）
        "28:57:be" | "44:19:b6" | "54:c4:15" | "a4:14:37" | "c0:56:e3" => {
            Some("海康威视摄像头")
        }
        "3c:ef:8c" | "4c:11:bf" | "90:02:a9" => Some("大华安防"),
        // 三星 Samsung
        "00:12:fb" | "14:49:e0" | "28:27:bf" | "50:cc:f8" | "84:25:db" | "bc:14:85"
        | "cc:07:ab" | "f4:b5:20" => Some("三星 Samsung"),
        // 联想 / Dell / HP
        "2c:5d:34" | "3c:97:0e" | "54:ee:75" | "70:5a:0f" | "b0:60:88" | "c0:28:0b" => {
            Some("通用硬件")
        }
        // 爱普生 Epson（无线打印机 / 投影仪）
        "00:00:48" | "00:26:ab" | "44:d2:44" | "64:eb:8c" | "a4:d7:3c" | "b0:e8:92"
        | "f8:d0:27" => Some("爱普生 Epson"),
        // 惠普 HP 打印机与办公外设
        "00:11:0a" | "00:17:a4" | "10:1f:74" | "3c:d9:2b" | "6c:c2:17" | "94:57:a5"
        | "b4:b5:2f" | "d4:c9:ef" => Some("惠普 HP"),
        // 佳能 Canon（打印机 / 扫描仪）
        "00:00:85" | "00:1e:8f" | "18:0c:ac" | "34:9f:7b" | "88:87:17" | "f4:81:39" => {
            Some("佳能 Canon")
        }
        // 兄弟 Brother（打印机 / 一体机）
        "00:1b:a9" | "00:80:77" | "30:05:5c" | "3c:2a:f4" | "b4:22:00" => {
            Some("兄弟 Brother")
        }
        _ => None,
    }
}

/// 综合主机名、厂商、随机 MAC 推断设备类型与对小白友好的默认名称
pub fn classify_device(
    ip: &str,
    mac: &str,
    hostname: Option<&str>,
    is_self: bool,
    is_gateway: bool,
) -> (&'static str, String, String) {
    let randomized = is_randomized_mac(mac);
    let vendor = lookup_vendor(mac);
    let hn_lower = hostname.unwrap_or("").to_lowercase();

    if is_gateway {
        let label = vendor.unwrap_or("主路由器");
        return ("router", format!("路由器 ({})", label), label.to_string());
    }
    if is_self {
        let label = hostname
            .map(|s| s.trim_end_matches(".local.").trim_end_matches(".local").to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "当前这台电脑".to_string());
        return ("laptop", label, vendor.unwrap_or("本机").to_string());
    }

    if hn_lower.contains("macbook") || hn_lower.contains("imac") || hn_lower.contains("macmini") {
        let clean = hostname
            .unwrap_or("Mac 电脑")
            .trim_end_matches(".local.")
            .trim_end_matches(".local");
        return ("laptop", clean.to_string(), "Apple".to_string());
    }
    if hn_lower.contains("iphone") || hn_lower.contains("ipad") {
        let clean = hostname
            .unwrap_or("iPhone / iPad")
            .trim_end_matches(".local.")
            .trim_end_matches(".local");
        return ("phone", clean.to_string(), "Apple".to_string());
    }
    if hn_lower.contains("android") {
        let clean = hostname
            .unwrap_or("安卓手机")
            .trim_end_matches(".local.")
            .trim_end_matches(".local");
        return (
            "phone",
            format!("安卓设备 ({})", clean),
            if randomized {
                "Android（随机MAC）".to_string()
            } else {
                vendor.unwrap_or("Android").to_string()
            },
        );
    }

    if let Some(h) = hostname.filter(|s| !s.is_empty()) {
        let clean = h.trim_end_matches(".local.").trim_end_matches(".local");
        let dtype = if hn_lower.contains("tv") || hn_lower.contains("printer") {
            "iot"
        } else {
            "laptop"
        };
        let v = vendor.unwrap_or(if randomized { "随机 MAC 终端" } else { "局域网设备" });
        return (dtype, clean.to_string(), v.to_string());
    }

    if let Some(v) = vendor {
        let dtype = if v.contains("Epson")
            || v.contains("Canon")
            || v.contains("Brother")
            || v.contains("HP")
        {
            "printer"
        } else if v.contains("智能家居") || v.contains("摄像头") || v.contains("Realtek") {
            "iot"
        } else if v.contains("Apple") || v.contains("小米") || v.contains("华为") || v.contains("三星") {
            "phone"
        } else {
            "laptop"
        };
        let suffix = ip.split('.').next_back().unwrap_or("");
        return (dtype, format!("{} 设备 (#{})", v, suffix), v.to_string());
    }

    let suffix = ip.split('.').next_back().unwrap_or("");
    if randomized {
        (
            "phone",
            format!("手机或笔记本 (#{})", suffix),
            "隐私随机 MAC".to_string(),
        )
    } else {
        (
            "iot",
            format!("固定接入设备 (#{})", suffix),
            "未知固定网卡".to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_and_vendor() {
        assert_eq!(normalize_mac("c4:9:d9:69:65:7d").as_deref(), Some("c4:09:d9:69:65:7d"));
        assert_eq!(normalize_mac("C4-92-D9-69-65-7D").as_deref(), Some("c4:92:d9:69:65:7d"));
        assert_eq!(lookup_vendor("c4:92:d9:69:65:7d"), Some("中兴 ZTE"));
        assert!(is_randomized_mac("c6:53:6c:8b:35:23"));
        assert!(!is_randomized_mac("c4:92:d9:69:65:7d"));
    }
}
