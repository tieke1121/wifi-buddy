export interface RouterCapabilities {
  supports_clients: boolean;
  supports_wifi_config: boolean;
  supports_block_mac: boolean;
  supports_guest_wifi: boolean;
  supports_reboot: boolean;
  default_username: string | null;
  note: string;
}

export interface RouterFingerprint {
  gateway_ip: string;
  gateway_mac: string | null;
  brand: string;
  admin_url: string;
  page_title: string | null;
  adapter_id: string | null;
  adapter_supported: boolean;
  capabilities: RouterCapabilities | null;
  guide_steps: string[];
}

export interface RouterBinding {
  gateway_ip: string;
  adapter_id: string;
  username: string | null;
  bound_at: number;
}

export interface RouterBindResult {
  adapter_id: string;
  adapter_name: string;
  gateway_ip: string;
  message: string;
}

export interface RouterClientInfo {
  ip: string | null;
  mac: string;
  name: string | null;
  band: string | null;
  signal_dbm: number | null;
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
  device_type: "laptop" | "phone" | "router" | "iot" | string;
  is_self: boolean;
  is_gateway: boolean;
  is_randomized_mac: boolean;
  ping_alive: boolean;
  latency_ms: number | null;
  trusted: boolean;
  blocked: boolean;
  is_new: boolean;
}

export interface NetworkEvent {
  timestamp: number;
  kind: string;
  title: string;
  detail: string;
}

export interface AppSettings {
  auto_scan_enabled: boolean;
  scan_interval_secs: number;
  notify_new_device: boolean;
  minimize_to_tray: boolean;
}

export interface ParentalRule {
  id: string;
  mac: string;
  device_name: string;
  start_hhmm: string;
  end_hhmm: string;
  enabled: boolean;
  currently_enforced?: boolean;
  last_notified_day?: string | null;
}

export interface GuestWifiState {
  enabled: boolean;
  ssid: string;
  password: string;
  auto_off_at: number | null;
}

export interface AppPreferences {
  onboarding_done: boolean;
  settings: AppSettings;
  events: NetworkEvent[];
  router_binding: RouterBinding | null;
  has_saved_router_password: boolean;
  router_connected: boolean;
  parental_rules: ParentalRule[];
  guest_wifi: GuestWifiState;
  autostart_enabled: boolean;
}

export interface ScanResponse {
  scanned_at: number;
  total_clients: number;
  new_count: number;
  ping_alive_count: number;
  devices: DeviceView[];
  brand_new_devices: DeviceView[];
  events: NetworkEvent[];
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

export interface SpeedTestResult {
  download_mbps: number | null;
  quality_label: string;
  advice: string;
  source: string;
  error: string | null;
}

export interface WifiPasswordResult {
  ssid: string;
  password: string | null;
  error: string | null;
}
