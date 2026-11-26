use crate::hardware_probe::{
    find_matching_installed_printer, install_system_printer_os, list_installed_printers,
    open_printer_maintenance_os, open_system_printer_settings_os, probe_active_devices_with_hints,
    probe_web_fingerprint, scan_common_ports, InstalledPrinter, ProbeSummary,
};
use crate::ssdp::SsdpInfo;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityAction {
    pub id: String,
    pub label: String,
    pub description: String,
    pub style: String,
    pub url: Option<String>,
    pub disabled: bool,
    pub badge: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfile {
    pub adapter_id: String,
    pub kind: String,
    pub category_label: String,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub status_text: String,
    pub services: Vec<String>,
    pub admin_url: Option<String>,
    pub system_installed: Option<bool>,
    pub system_printer_name: Option<String>,
    pub actions: Vec<CapabilityAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareActionResult {
    pub success: bool,
    pub message: String,
    pub detail: Option<String>,
    pub refresh_needed: bool,
}

pub struct HardwareEvidence<'a> {
    pub ip: &'a str,
    #[allow(dead_code)]
    pub mac: &'a str,
    pub vendor: &'a str,
    pub hostname: Option<&'a str>,
    pub is_gateway: bool,
    pub router_brand: Option<&'a str>,
    pub router_admin_url: Option<&'a str>,
    pub router_adapter_supported: bool,
    pub ping_alive: bool,
    pub latency_ms: Option<u32>,
    pub ssdp: Option<&'a SsdpInfo>,
    pub probe: Option<&'a ProbeSummary>,
    pub installed_printers: &'a [InstalledPrinter],
}

pub trait HardwareAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn probe(&self, ev: &HardwareEvidence<'_>) -> Option<HardwareProfile>;
}

pub struct RouterHardwareAdapter;
pub struct PrinterHardwareAdapter;
pub struct SmartHomeHardwareAdapter;

impl HardwareAdapter for RouterHardwareAdapter {
    fn id(&self) -> &'static str {
        "router_gateway"
    }

    fn probe(&self, ev: &HardwareEvidence<'_>) -> Option<HardwareProfile> {
        if !ev.is_gateway {
            return None;
        }
        let brand = ev.router_brand.unwrap_or("主路由器").to_string();
        let admin_url = ev
            .router_admin_url
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("http://{}", ev.ip));

        let mut actions = vec![CapabilityAction {
            id: "open_router_tab".to_string(),
            label: if ev.router_adapter_supported {
                "路由器控制与守护".to_string()
            } else {
                "查看改密引导".to_string()
            },
            description: "前往「路由器与守护」绑定账号、查看终端或开启访客网络".to_string(),
            style: "primary".to_string(),
            url: None,
            disabled: false,
            badge: if ev.router_adapter_supported {
                Some("已支持专属适配".to_string())
            } else {
                None
            },
        }];

        actions.push(CapabilityAction {
            id: "open_admin_url".to_string(),
            label: "打开网页后台".to_string(),
            description: format!("在浏览器中打开 {}", admin_url),
            style: "secondary".to_string(),
            url: Some(admin_url.clone()),
            disabled: false,
            badge: None,
        });

        Some(HardwareProfile {
            adapter_id: self.id().to_string(),
            kind: "router".to_string(),
            category_label: "主路由器".to_string(),
            model: Some(brand.clone()),
            manufacturer: Some(brand),
            status_text: ev
                .latency_ms
                .map(|ms| format!("网关在线 · {} ms", ms))
                .unwrap_or_else(|| "网关在线".to_string()),
            services: vec!["Web 管理后台".to_string()],
            admin_url: Some(admin_url),
            system_installed: None,
            system_printer_name: None,
            actions,
        })
    }
}

fn infer_printer_brand(ev: &HardwareEvidence<'_>, title: Option<&str>, server: Option<&str>) -> String {
    let combined = format!(
        "{} {} {} {} {}",
        ev.vendor,
        title.unwrap_or(""),
        server.unwrap_or(""),
        ev.hostname.unwrap_or(""),
        ev.ssdp
            .and_then(|s| s.manufacturer.as_deref())
            .unwrap_or("")
    )
    .to_lowercase();

    if combined.contains("epson") || combined.contains("爱普生") {
        "爱普生 Epson".to_string()
    } else if combined.contains("canon") || combined.contains("佳能") {
        "佳能 Canon".to_string()
    } else if combined.contains("brother") || combined.contains("兄弟") {
        "兄弟 Brother".to_string()
    } else if combined.contains("hp") || combined.contains("hewlett") || combined.contains("惠普") {
        "惠普 HP".to_string()
    } else if !ev.vendor.contains("未知") && !ev.vendor.contains("固定") {
        ev.vendor.to_string()
    } else {
        "网络打印机".to_string()
    }
}

fn build_driver_guide_url(brand: &str, model: Option<&str>) -> String {
    let bl = brand.to_lowercase();
    let m = model.unwrap_or("").trim();
    if bl.contains("epson") || bl.contains("爱普生") {
        if m.is_empty() {
            "https://www.epson.com.cn/Apps/tech_support/index.aspx".to_string()
        } else {
            format!(
                "https://www.epson.com.cn/Apps/tech_support/Search.aspx?q={}",
                m.replace(' ', "%20")
            )
        }
    } else if bl.contains("hp") || bl.contains("惠普") {
        "https://support.hp.com/cn-zh/drivers/printers".to_string()
    } else if bl.contains("canon") || bl.contains("佳能") {
        "https://www.canon.com.cn/supports/download/sims/search/index".to_string()
    } else if bl.contains("brother") || bl.contains("兄弟") {
        "https://www.brother.cn/support/download".to_string()
    } else {
        format!("https://www.bing.com/search?q={}%20driver", m.replace(' ', "%20"))
    }
}

fn build_printer_actions(
    ip: &str,
    brand: &str,
    model: Option<&str>,
    admin_url: Option<&str>,
    installed_name: Option<&str>,
) -> Vec<CapabilityAction> {
    let is_installed = installed_name.is_some();
    let mut actions = vec![CapabilityAction {
        id: "install_system_printer".to_string(),
        label: if is_installed {
            "已连接到本机电脑".to_string()
        } else {
            "一键添加到本机打印".to_string()
        },
        description: if let Some(pname) = installed_name {
            format!("已在系统打印机中就绪（{}），可直接发起打印", pname)
        } else {
            format!("自动通过局域网 IP ({}) 将该打印机添加到系统列表", ip)
        },
        style: "primary".to_string(),
        url: None,
        disabled: is_installed,
        badge: if is_installed {
            Some("已就绪".to_string())
        } else {
            Some("推荐".to_string())
        },
    }];

    actions.push(CapabilityAction {
        id: "open_printer_maintenance".to_string(),
        label: "喷头清洗 / 打印维护".to_string(),
        description: if is_installed {
            "直达系统打印首选项窗口，在官方驱动「维护」中执行喷头清洗或深度清洗".to_string()
        } else {
            "打开打印首选项中的驱动「维护」选项卡执行清洗（需先添加到本机或安装官方驱动）"
                .to_string()
        },
        style: "secondary".to_string(),
        url: None,
        disabled: false,
        badge: Some("维护入口".to_string()),
    });

    actions.push(CapabilityAction {
        id: "open_system_printer_settings".to_string(),
        label: "打开系统打印机设置".to_string(),
        description: "打开系统「打印机和扫描仪」面板查看打印队列或手动添加".to_string(),
        style: "secondary".to_string(),
        url: None,
        disabled: false,
        badge: None,
    });

    if let Some(url) = admin_url {
        actions.push(CapabilityAction {
            id: "open_admin_url".to_string(),
            label: "打开打印机网页后台".to_string(),
            description: format!("在浏览器访问 {} 查看网络、连接状态与固件信息", url),
            style: "secondary".to_string(),
            url: Some(url.to_string()),
            disabled: false,
            badge: None,
        });
    }

    let driver_url = build_driver_guide_url(brand, model);
    actions.push(CapabilityAction {
        id: "open_driver_guide".to_string(),
        label: "下载官方完整驱动（含扫描/清洗）".to_string(),
        description: "系统通用驱动支持无线打印；如需深度清洗、喷嘴检查或无线扫描，请安装官方驱动".to_string(),
        style: "subtle".to_string(),
        url: Some(driver_url),
        disabled: false,
        badge: None,
    });

    actions
}

fn format_printer_status_text(ping_alive: bool, is_installed: bool, latency_ms: Option<u32>) -> String {
    match (ping_alive, is_installed, latency_ms) {
        (true, true, Some(ms)) => format!("在线 · {} ms · 本机已连接", ms),
        (true, false, Some(ms)) => format!("在线 · {} ms · 本机未添加", ms),
        (true, true, None) => "在线 · 本机已连接".to_string(),
        (true, false, None) => "在线 · 本机未添加".to_string(),
        (false, true, _) => "休眠或离线 · 本机已连接".to_string(),
        (false, false, _) => "休眠或离线 · 本机未添加".to_string(),
    }
}

/// 根据历史绑定记录与系统已安装打印机状态重建打印机硬件画像（避免重启或休眠时消失）
pub fn build_restored_printer_profile(
    ip: &str,
    vendor: &str,
    model: Option<&str>,
    admin_url: Option<&str>,
    ping_alive: bool,
    latency_ms: Option<u32>,
    installed_printers: &[InstalledPrinter],
) -> HardwareProfile {
    let installed = find_matching_installed_printer(installed_printers, ip, model);
    let is_installed = installed.is_some();
    let installed_name = installed.as_ref().map(|p| p.name.clone());
    let brand = if !vendor.trim().is_empty() && !vendor.contains("未知") && !vendor.contains("固定") {
        vendor.to_string()
    } else {
        "网络打印机".to_string()
    };
    let status_text = format_printer_status_text(ping_alive, is_installed, latency_ms);
    let actions = build_printer_actions(
        ip,
        &brand,
        model,
        admin_url,
        installed_name.as_deref(),
    );

    HardwareProfile {
        adapter_id: "network_printer".to_string(),
        kind: "printer".to_string(),
        category_label: "Wi-Fi 打印机".to_string(),
        model: model.map(String::from),
        manufacturer: Some(brand),
        status_text,
        services: if is_installed {
            vec!["已关联本机打印队列".to_string()]
        } else {
            vec!["已记录网络打印机".to_string()]
        },
        admin_url: admin_url.map(String::from),
        system_installed: Some(is_installed),
        system_printer_name: installed_name,
        actions,
    }
}

impl HardwareAdapter for PrinterHardwareAdapter {
    fn id(&self) -> &'static str {
        "network_printer"
    }

    fn probe(&self, ev: &HardwareEvidence<'_>) -> Option<HardwareProfile> {
        if ev.is_gateway {
            return None;
        }
        let open_ports = ev.probe.map(|p| p.open_ports.as_slice()).unwrap_or(&[]);
        let mdns_svcs = ev.probe.map(|p| p.mdns_services.as_slice()).unwrap_or(&[]);

        let web_title = ev.probe.and_then(|p| p.web.title.as_deref());
        let web_server = ev.probe.and_then(|p| p.web.server.as_deref());
        let model = web_title
            .map(|t| t.to_string())
            .or_else(|| ev.ssdp.and_then(|s| s.model_name.clone()))
            .or_else(|| ev.ssdp.and_then(|s| s.friendly_name.clone()));

        let installed = find_matching_installed_printer(
            ev.installed_printers,
            ev.ip,
            model.as_deref().or(ev.hostname),
        );

        let has_printer_port = open_ports.contains(&631) || open_ports.contains(&9100);
        let has_printer_mdns = mdns_svcs.iter().any(|s| s.contains("_ipp"));
        let has_printer_vendor = ev.vendor.contains("Epson")
            || ev.vendor.contains("HP")
            || ev.vendor.contains("Canon")
            || ev.vendor.contains("Brother");
        let ssdp_printer = ev
            .ssdp
            .and_then(|s| s.device_type.as_deref())
            .map(|t| t.to_lowercase().contains("printer"))
            .unwrap_or(false);

        if !has_printer_port
            && !has_printer_mdns
            && !has_printer_vendor
            && !ssdp_printer
            && installed.is_none()
        {
            return None;
        }

        let brand = infer_printer_brand(ev, web_title, web_server);
        let final_model = model.or_else(|| installed.as_ref().map(|p| p.name.clone()));

        let mut services = Vec::new();
        if open_ports.contains(&631) {
            services.push("IPP/IPPS 无线打印 (631)".to_string());
        }
        if open_ports.contains(&9100) {
            services.push("RAW 高速打印 (9100)".to_string());
        }
        if open_ports.contains(&443) || open_ports.contains(&80) {
            services.push("Web 状态管理页".to_string());
        }
        if services.is_empty() {
            services.push("局域网打印服务".to_string());
        }

        let admin_url = ev.probe.and_then(|p| p.web.admin_url.clone()).or_else(|| {
            if open_ports.contains(&443) {
                Some(format!("https://{}/", ev.ip))
            } else if open_ports.contains(&80) {
                Some(format!("http://{}/", ev.ip))
            } else {
                None
            }
        });

        let is_installed = installed.is_some();
        let installed_name = installed.as_ref().map(|p| p.name.clone());
        let status_text = format_printer_status_text(ev.ping_alive, is_installed, ev.latency_ms);
        let actions = build_printer_actions(
            ev.ip,
            &brand,
            final_model.as_deref(),
            admin_url.as_deref(),
            installed_name.as_deref(),
        );

        Some(HardwareProfile {
            adapter_id: self.id().to_string(),
            kind: "printer".to_string(),
            category_label: "Wi-Fi 打印机".to_string(),
            model: final_model,
            manufacturer: Some(brand),
            status_text,
            services,
            admin_url,
            system_installed: Some(is_installed),
            system_printer_name: installed_name,
            actions,
        })
    }
}

impl HardwareAdapter for SmartHomeHardwareAdapter {
    fn id(&self) -> &'static str {
        "smarthome_iot"
    }

    fn probe(&self, ev: &HardwareEvidence<'_>) -> Option<HardwareProfile> {
        if ev.is_gateway {
            return None;
        }
        let open_ports = ev.probe.map(|p| p.open_ports.as_slice()).unwrap_or(&[]);
        let mdns_svcs = ev.probe.map(|p| p.mdns_services.as_slice()).unwrap_or(&[]);

        let is_ha = open_ports.contains(&8123)
            || mdns_svcs.iter().any(|s| s.contains("home-assistant"));
        let is_camera = open_ports.contains(&554) || ev.vendor.contains("摄像头") || ev.vendor.contains("大华");
        let is_media = mdns_svcs.iter().any(|s| s.contains("airplay") || s.contains("googlecast"))
            || ev
                .ssdp
                .and_then(|s| s.device_type_hint())
                .map(|h| h == "iot")
                .unwrap_or(false);
        let is_esp = ev.vendor.contains("Espressif");

        if !is_ha && !is_camera && !is_media && !is_esp {
            return None;
        }

        let (kind, category_label, model, admin_url, services) = if is_ha {
            (
                "smarthome".to_string(),
                "智能家居中枢".to_string(),
                Some("Home Assistant".to_string()),
                Some(format!("http://{}:8123", ev.ip)),
                vec!["Home Assistant (8123)".to_string()],
            )
        } else if is_camera {
            let url = if open_ports.contains(&80) {
                Some(format!("http://{}/", ev.ip))
            } else {
                None
            };
            (
                "camera".to_string(),
                "安防摄像头".to_string(),
                ev.ssdp.and_then(|s| s.model_name.clone()),
                url,
                if open_ports.contains(&554) {
                    vec!["RTSP 视频流 (554)".to_string()]
                } else {
                    vec!["安防设备".to_string()]
                },
            )
        } else if is_media {
            let title = ev
                .ssdp
                .and_then(|s| s.friendly_name.clone().or_else(|| s.model_name.clone()));
            (
                "media".to_string(),
                "影音 / 智能设备".to_string(),
                title,
                None,
                vec!["局域网媒体发现".to_string()],
            )
        } else {
            let url = if open_ports.contains(&80) {
                Some(format!("http://{}/", ev.ip))
            } else {
                None
            };
            (
                "smarthome".to_string(),
                "智能家居终端".to_string(),
                None,
                url,
                vec!["Wi-Fi 智能节点".to_string()],
            )
        };

        let mut actions = Vec::new();
        if let Some(ref url) = admin_url {
            actions.push(CapabilityAction {
                id: "open_admin_url".to_string(),
                label: if is_ha {
                    "打开 Home Assistant 面板".to_string()
                } else {
                    "打开设备管理页".to_string()
                },
                description: format!("在浏览器访问 {}", url),
                style: "primary".to_string(),
                url: Some(url.clone()),
                disabled: false,
                badge: None,
            });
        }

        Some(HardwareProfile {
            adapter_id: self.id().to_string(),
            kind,
            category_label,
            model,
            manufacturer: ev
                .ssdp
                .and_then(|s| s.manufacturer.clone())
                .or_else(|| Some(ev.vendor.to_string())),
            status_text: ev
                .latency_ms
                .map(|ms| format!("在线 · {} ms", ms))
                .unwrap_or_else(|| "在线".to_string()),
            services,
            admin_url,
            system_installed: None,
            system_printer_name: None,
            actions,
        })
    }
}

fn match_hardware_profile(ev: &HardwareEvidence<'_>) -> Option<HardwareProfile> {
    let adapters: Vec<Box<dyn HardwareAdapter>> = vec![
        Box::new(RouterHardwareAdapter),
        Box::new(PrinterHardwareAdapter),
        Box::new(SmartHomeHardwareAdapter),
    ];
    for adapter in adapters {
        if let Some(profile) = adapter.probe(ev) {
            return Some(profile);
        }
    }
    None
}

/// 在一次局域网扫描中批量并发生成可管理硬件画像
pub fn build_lan_hardware_profiles(
    ips: &[String],
    gateway_ip: &str,
    local_ip: &str,
    priority_ips: &HashSet<String>,
) -> (HashMap<String, ProbeSummary>, Vec<InstalledPrinter>) {
    let probe_map = probe_active_devices_with_hints(ips, gateway_ip, local_ip, priority_ips);
    let installed_printers = list_installed_printers();
    (probe_map, installed_printers)
}

pub fn resolve_device_hardware_profile(ev: &HardwareEvidence<'_>) -> Option<HardwareProfile> {
    match_hardware_profile(ev)
}

/// 对单台硬件进行实时深度探测（供抽屉页刷新使用）
pub fn probe_single_hardware_profile(
    ip: &str,
    mac: &str,
    vendor: &str,
    hostname: Option<&str>,
    is_gateway: bool,
) -> Option<HardwareProfile> {
    let open_ports = if is_gateway {
        vec![80]
    } else {
        scan_common_ports(ip)
    };
    let web = if !is_gateway && open_ports.iter().any(|p| matches!(p, 631 | 9100 | 8123 | 554 | 80 | 443)) {
        probe_web_fingerprint(ip, &open_ports)
    } else {
        Default::default()
    };
    let summary = ProbeSummary {
        open_ports,
        web,
        mdns_services: Vec::new(),
    };
    let installed = list_installed_printers();
    let gw_admin_url = format!("http://{}", ip);
    let ev = HardwareEvidence {
        ip,
        mac,
        vendor,
        hostname,
        is_gateway,
        router_brand: if is_gateway { Some(vendor) } else { None },
        router_admin_url: if is_gateway {
            Some(gw_admin_url.as_str())
        } else {
            None
        },
        router_adapter_supported: false,
        ping_alive: true,
        latency_ms: None,
        ssdp: None,
        probe: Some(&summary),
        installed_printers: &installed,
    };
    match_hardware_profile(&ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_probe::WebFingerprint;

    #[test]
    fn test_printer_adapter_matches_epson() {
        let probe = ProbeSummary {
            open_ports: vec![80, 443, 631, 9100],
            web: WebFingerprint {
                title: Some("L3250 Series".to_string()),
                server: Some("EPSON_Linux UPnP/1.0".to_string()),
                admin_url: Some("https://192.168.5.67/".to_string()),
            },
            mdns_services: vec![],
        };
        let ev = HardwareEvidence {
            ip: "192.168.5.67",
            mac: "a4:d7:3c:53:f6:35",
            vendor: "爱普生 Epson",
            hostname: None,
            is_gateway: false,
            router_brand: None,
            router_admin_url: None,
            router_adapter_supported: false,
            ping_alive: true,
            latency_ms: Some(5),
            ssdp: None,
            probe: Some(&probe),
            installed_printers: &[],
        };
        let profile = resolve_device_hardware_profile(&ev).expect("should match printer");
        assert_eq!(profile.kind, "printer");
        assert_eq!(profile.model.as_deref(), Some("L3250 Series"));
        assert_eq!(profile.manufacturer.as_deref(), Some("爱普生 Epson"));
        assert_eq!(profile.system_installed, Some(false));
        assert!(profile.actions.iter().any(|a| a.id == "install_system_printer"));
    }

    #[test]
    fn test_printer_adapter_matches_installed_when_ports_timed_out() {
        let installed = vec![InstalledPrinter {
            name: "Canon G3810".to_string(),
            port_name: "IP_192.168.5.77".to_string(),
            host_address: "192.168.5.77".to_string(),
        }];
        let ev = HardwareEvidence {
            ip: "192.168.5.77",
            mac: "11:22:33:44:55:66",
            vendor: "未知厂商",
            hostname: None,
            is_gateway: false,
            router_brand: None,
            router_admin_url: None,
            router_adapter_supported: false,
            ping_alive: false,
            latency_ms: None,
            ssdp: None,
            probe: None,
            installed_printers: &installed,
        };
        let profile = resolve_device_hardware_profile(&ev).expect("installed printer should match");
        assert_eq!(profile.kind, "printer");
        assert_eq!(profile.system_installed, Some(true));
        assert_eq!(profile.system_printer_name.as_deref(), Some("Canon G3810"));
        assert!(profile
            .actions
            .iter()
            .any(|a| a.id == "open_printer_maintenance"));
    }

    #[test]
    fn test_build_restored_printer_profile_offline_states() {
        let installed = vec![InstalledPrinter {
            name: "EPSON L3250 Series".to_string(),
            port_name: "IP_192.168.5.67".to_string(),
            host_address: "192.168.5.67".to_string(),
        }];
        let connected = build_restored_printer_profile(
            "192.168.5.67",
            "爱普生 Epson",
            Some("L3250 Series"),
            Some("https://192.168.5.67/"),
            false,
            None,
            &installed,
        );
        assert_eq!(connected.system_installed, Some(true));
        assert!(connected.status_text.contains("休眠或离线"));
        assert!(connected.status_text.contains("本机已连接"));
        assert!(connected
            .actions
            .iter()
            .any(|a| a.id == "open_printer_maintenance"));

        let disconnected = build_restored_printer_profile(
            "192.168.5.88",
            "惠普 HP",
            Some("LaserJet"),
            None,
            false,
            None,
            &installed,
        );
        assert_eq!(disconnected.system_installed, Some(false));
        assert!(disconnected.status_text.contains("本机未添加"));
    }
}

/// 统一执行硬件动作（如一键添加打印机、打开系统打印机设置、喷头清洗维护入口等）
pub fn execute_hardware_action_impl(
    ip: &str,
    _mac: &str,
    action_id: &str,
    params: Option<&Value>,
) -> HardwareActionResult {
    match action_id {
        "install_system_printer" => {
            let display_name = params
                .and_then(|v| v.get("display_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Wi-Fi 打印机");
            let open_ports = scan_common_ports(ip);
            let has_631 = open_ports.contains(&631);
            let has_9100 = open_ports.contains(&9100);
            match install_system_printer_os(ip, display_name, has_631, has_9100) {
                Ok(msg) => HardwareActionResult {
                    success: true,
                    message: msg,
                    detail: Some("如需使用无线扫描或深度清洗功能，可点击下方「下载官方完整驱动」。".to_string()),
                    refresh_needed: true,
                },
                Err(err) => HardwareActionResult {
                    success: false,
                    message: err,
                    detail: None,
                    refresh_needed: false,
                },
            }
        }
        "open_printer_maintenance" => {
            let printer_name = params
                .and_then(|v| v.get("system_printer_name"))
                .and_then(|v| v.as_str());
            let display_name = params
                .and_then(|v| v.get("display_name"))
                .and_then(|v| v.as_str());
            match open_printer_maintenance_os(ip, printer_name, display_name) {
                Ok(msg) => HardwareActionResult {
                    success: true,
                    message: msg,
                    detail: Some("若本机已装厂商完整驱动，可在弹出的「维护」选项卡执行喷头清洗/深度清洗；若仅用系统通用驱动，需安装官方驱动或长按打印机机身维护键。".to_string()),
                    refresh_needed: false,
                },
                Err(err) => HardwareActionResult {
                    success: false,
                    message: err,
                    detail: Some("你也可以先安装官方完整驱动，或在打印机网页后台/物理面板操作。".to_string()),
                    refresh_needed: false,
                },
            }
        }
        "open_system_printer_settings" => match open_system_printer_settings_os() {
            Ok(msg) => HardwareActionResult {
                success: true,
                message: msg,
                detail: Some(format!("如果在列表中未看到打印机，点「添加设备 → 手动添加」并输入 IP：{}", ip)),
                refresh_needed: false,
            },
            Err(err) => HardwareActionResult {
                success: false,
                message: err,
                detail: None,
                refresh_needed: false,
            },
        },
        _ => HardwareActionResult {
            success: false,
            message: format!("暂不支持动作: {}", action_id),
            detail: None,
            refresh_needed: false,
        },
    }
}
