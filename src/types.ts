export interface RouterFingerprint {
  gateway_ip: string;
  gateway_mac: string | null;
  brand: string;
  admin_url: string;
  page_title: string | null;
  adapter_supported: boolean;
  guide_steps: string[];
}

export interface NetworkOverview {
  interface_name: string;
  local_ip: string;
  subnet_cidr: string;
  ssid: string | null;
  ssid_redacted: boolean;
  preferred_ssids: string[];
  signal_dbm: number | null;
  noise_dbm: number | null;
  channel: string | null;
  phy_mode: string | null;
  tx_rate_mbps: number | null;
  router: RouterFingerprint;
}

export interface DeviceView {
  ip: string;
  mac: string;
  display_name: string;
  default_name: string;
  alias: string | null;
  vendor: string;
  hostname: string | null;
  device_type: "laptop" | "phone" | "router" | "iot" | "unknown" | string;
  is_self: boolean;
  is_gateway: boolean;
  is_randomized_mac: boolean;
  ping_alive: boolean;
  latency_ms: number | null;
  trusted: boolean;
  blocked: boolean;
  is_new: boolean;
}

export interface ScanResponse {
  scanned_at: number;
  total_clients: number;
  new_count: number;
  ping_alive_count: number;
  devices: DeviceView[];
}

export interface HealthStep {
  id: string;
  title: string;
  status: "good" | "warning" | "error" | string;
  badge: string;
  detail: string;
}

export interface HealthReport {
  overall_status: "good" | "warning" | "error" | string;
  summary_title: string;
  summary_advice: string;
  gateway_latency_ms: number | null;
  internet_latency_ms: number | null;
  signal_dbm: number | null;
  steps: HealthStep[];
}

export interface WifiPasswordResult {
  ssid: string;
  password: string | null;
  error: string | null;
}
