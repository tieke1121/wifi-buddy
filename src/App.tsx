import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";
import {
  Activity,
  AlertTriangle,
  ArrowUpRight,
  Ban,
  Check,
  CheckCircle2,
  Copy,
  Eye,
  EyeOff,
  Gauge,
  KeyRound,
  Laptop,
  Lock,
  Pencil,
  Radar,
  RefreshCw,
  Router,
  Search,
  Settings2,
  Smartphone,
  Sparkles,
  Tv,
  Wand2,
  Wifi,
  X,
} from "lucide-react";
import { OnboardingModal } from "./components/OnboardingModal";
import { RouterGuardianTab } from "./components/RouterGuardianTab";
import { HardwareCardsSection, HardwareKindIcon } from "./components/HardwareCardsSection";
import { HardwareDrawer } from "./components/HardwareDrawer";
import { evaluatePasswordStrength, generateMemorablePassword } from "./utils/password";
import type {
  AppPreferences,
  AppSettings,
  CapabilityAction,
  DeviceView,
  HardwareActionResult,
  HardwareProfile,
  HealthReport,
  NetworkOverview,
  ScanResponse,
  SpeedTestResult,
  WifiPasswordResult,
} from "./types";

type TabId = "devices" | "share" | "health" | "router";
type DeviceFilter = "all" | "hardware" | "new" | "personal" | "iot" | "blocked";

function getSignalMeta(dbm: number | null) {
  if (dbm === null) return { label: "信号检测中", bars: 2, color: "text-slate-400" };
  if (dbm >= -55) return { label: "信号极佳", bars: 4, color: "text-emerald-500" };
  if (dbm >= -67) return { label: "信号良好", bars: 3, color: "text-emerald-500" };
  if (dbm >= -77) return { label: "信号一般", bars: 2, color: "text-amber-500" };
  return { label: "信号偏弱", bars: 1, color: "text-rose-500" };
}

function SignalBars({ bars }: { bars: number }) {
  return (
    <div className="flex items-end gap-0.5 h-3.5" aria-hidden="true">
      {[1, 2, 3, 4].map((lvl) => (
        <span
          key={lvl}
          className={`w-1 rounded-full transition-colors ${
            lvl <= bars ? "bg-emerald-500" : "bg-slate-200 dark:bg-zinc-700"
          }`}
          style={{ height: `${lvl * 25}%` }}
        />
      ))}
    </div>
  );
}

function DeviceIcon({ type }: { type: string }) {
  switch (type) {
    case "laptop":
      return <Laptop className="w-4 h-4" />;
    case "phone":
      return <Smartphone className="w-4 h-4" />;
    case "router":
      return <Router className="w-4 h-4" />;
    default:
      return <Tv className="w-4 h-4" />;
  }
}

export function App() {
  const [activeTab, setActiveTab] = useState<TabId>("devices");
  const [overview, setOverview] = useState<NetworkOverview | null>(null);
  const [preferences, setPreferences] = useState<AppPreferences | null>(null);
  const [scan, setScan] = useState<ScanResponse | null>(null);
  const [scanning, setScanning] = useState(false);

  const [filter, setFilter] = useState<DeviceFilter>("all");
  const [search, setSearch] = useState("");
  const [showTech, setShowTech] = useState(false);

  const [renamingDevice, setRenamingDevice] = useState<DeviceView | null>(null);
  const [renameInput, setRenameInput] = useState("");

  // 家庭硬件管理抽屉状态
  const [activeHardwareMac, setActiveHardwareMac] = useState<string | null>(null);
  const [runningHardwareKey, setRunningHardwareKey] = useState<string | null>(null);
  const [refreshingHardware, setRefreshingHardware] = useState(false);
  const [hardwareActionResult, setHardwareActionResult] =
    useState<HardwareActionResult | null>(null);

  // 分享与改密
  const [selectedSsid, setSelectedSsid] = useState("");
  const [wifiPassword, setWifiPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [readingPw, setReadingPw] = useState(false);
  const [copied, setCopied] = useState(false);
  const qrCanvasRef = useRef<HTMLCanvasElement | null>(null);

  // 路由器直接改密表单
  const [newWifiSsid, setNewWifiSsid] = useState("");
  const [newWifiPassword, setNewWifiPassword] = useState("");
  const [showNewWifiPw, setShowNewWifiPw] = useState(false);
  const [confirmingWifiSave, setConfirmingWifiSave] = useState(false);
  const [updatingWifi, setUpdatingWifi] = useState(false);

  // 访客 Wi-Fi
  const [guestSsid, setGuestSsid] = useState("MyHome_Guest");
  const [guestPassword, setGuestPassword] = useState("");
  const [guestDurationMins, setGuestDurationMins] = useState(240);
  const [updatingGuest, setUpdatingGuest] = useState(false);

  // 体检与测速
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);
  const [speedResult, setSpeedResult] = useState<SpeedTestResult | null>(null);
  const [testingSpeed, setTestingSpeed] = useState(false);

  // Toast
  const [toast, setToast] = useState<string | null>(null);
  const notify = (msg: string) => setToast(msg);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3800);
    return () => clearTimeout(t);
  }, [toast]);

  async function refreshPreferences() {
    try {
      const prefs = await invoke<AppPreferences>("get_app_preferences");
      setPreferences(prefs);
    } catch {
      // ignore
    }
  }

  async function loadOverviewAndScan() {
    setScanning(true);
    try {
      const [ov, prefs] = await Promise.all([
        invoke<NetworkOverview>("get_network_overview"),
        invoke<AppPreferences>("get_app_preferences"),
      ]);
      setOverview(ov);
      setPreferences(prefs);
      if (prefs.guest_wifi) {
        setGuestSsid(prefs.guest_wifi.ssid || "MyHome_Guest");
        setGuestPassword(prefs.guest_wifi.password || "");
      }
      const initialSsid = ov.ssid || ov.preferred_ssids[0] || "";
      setSelectedSsid((prev) => prev || initialSsid);
      setNewWifiSsid((prev) => prev || initialSsid);

      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      setPreferences((prev) => (prev ? { ...prev, events: res.events } : prev));
    } catch (e) {
      notify(`读取网络信息失败：${String(e)}`);
    } finally {
      setScanning(false);
    }
  }

  useEffect(() => {
    loadOverviewAndScan();

    let unlisten: (() => void) | undefined;
    listen<ScanResponse>("lan-scan-completed", (event) => {
      const res = event.payload;
      setScan(res);
      setPreferences((prev) => (prev ? { ...prev, events: res.events } : prev));
      if (res.brand_new_devices.length > 0) {
        notify(`后台巡检发现 ${res.brand_new_devices.length} 台新设备上线`);
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    const canvas = qrCanvasRef.current;
    if (!canvas || !selectedSsid.trim()) return;
    const escapeField = (v: string) => v.replace(/([\\;,:"])/g, "\\$1");
    const payload = wifiPassword
      ? `WIFI:T:WPA;S:${escapeField(selectedSsid)};P:${escapeField(wifiPassword)};;`
      : `WIFI:T:nopass;S:${escapeField(selectedSsid)};;`;
    QRCode.toCanvas(canvas, payload, {
      width: 176,
      margin: 1,
      color: { dark: "#0f172a", light: "#ffffff" },
    }).catch(() => {});
  }, [selectedSsid, wifiPassword, activeTab]);

  async function handleRescan() {
    setScanning(true);
    try {
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      setPreferences((prev) => (prev ? { ...prev, events: res.events } : prev));
      notify(`扫描完成，当前共有 ${res.total_clients} 台终端在线`);
    } catch (e) {
      notify(`扫描出错：${String(e)}`);
    } finally {
      setScanning(false);
    }
  }

  async function saveAliasByMac(mac: string, alias: string) {
    await invoke("save_device_meta", {
      mac,
      alias,
      trusted: true,
      blocked: null,
    });
    const res = await invoke<ScanResponse>("scan_devices");
    setScan(res);
  }

  async function handleSaveAlias() {
    if (!renamingDevice) return;
    try {
      await saveAliasByMac(renamingDevice.mac, renameInput);
      setRenamingDevice(null);
      notify(`已将设备备注为「${renameInput.trim() || renamingDevice.default_name}」`);
    } catch (e) {
      notify(`保存失败：${String(e)}`);
    }
  }

  async function handleTrustDevice(d: DeviceView) {
    try {
      await invoke("save_device_meta", {
        mac: d.mac,
        alias: null,
        trusted: true,
        blocked: false,
      });
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      notify(`已将「${d.display_name}」标为熟人设备`);
    } catch (e) {
      notify(`操作失败：${String(e)}`);
    }
  }

  async function handleToggleHidden(d: DeviceView, blocked: boolean) {
    try {
      await invoke("save_device_meta", {
        mac: d.mac,
        alias: null,
        trusted: null,
        blocked,
      });
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      notify(blocked ? `已隐藏「${d.display_name}」` : `已恢复显示「${d.display_name}」`);
    } catch (e) {
      notify(`操作失败：${String(e)}`);
    }
  }

  async function handleRouterBlock(d: DeviceView, block: boolean) {
    try {
      const msg = await invoke<string>("router_block_device", {
        mac: d.mac,
        block,
      });
      await invoke("save_device_meta", {
        mac: d.mac,
        alias: null,
        trusted: null,
        blocked: block,
      });
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      await refreshPreferences();
      notify(msg);
    } catch (e) {
      notify(String(e));
    }
  }

  async function handleReadSavedPassword() {
    if (!selectedSsid.trim()) {
      notify("请先选择或输入当前的 Wi-Fi 名称");
      return;
    }
    setReadingPw(true);
    try {
      const res = await invoke<WifiPasswordResult>("get_saved_wifi_password", {
        ssid: selectedSsid,
      });
      if (res.password) {
        setWifiPassword(res.password);
        setShowPassword(true);
        notify(`已读取「${res.ssid}」的保存密码`);
      } else {
        notify(res.error || "未能读取到密码，请手动输入");
      }
    } catch (e) {
      notify(`读取密码失败：${String(e)}`);
    } finally {
      setReadingPw(false);
    }
  }

  async function handleApplyRouterWifi() {
    if (!newWifiSsid.trim()) {
      notify("请填写新的 Wi-Fi 名称");
      return;
    }
    if (newWifiPassword.length < 8) {
      notify("Wi-Fi 密码至少需要 8 位");
      return;
    }
    setUpdatingWifi(true);
    try {
      const msg = await invoke<string>("update_router_wifi", {
        ssid: newWifiSsid.trim(),
        password: newWifiPassword,
      });
      setSelectedSsid(newWifiSsid.trim());
      setWifiPassword(newWifiPassword);
      setConfirmingWifiSave(false);
      await refreshPreferences();
      notify(msg);
    } catch (e) {
      notify(String(e));
    } finally {
      setUpdatingWifi(false);
    }
  }

  async function handleRunHealth() {
    setCheckingHealth(true);
    try {
      const report = await invoke<HealthReport>("run_health_check");
      setHealth(report);
    } catch (e) {
      notify(`体检失败：${String(e)}`);
    } finally {
      setCheckingHealth(false);
    }
  }

  async function handleRunSpeedTest() {
    setTestingSpeed(true);
    try {
      const res = await invoke<SpeedTestResult>("run_speed_test");
      setSpeedResult(res);
      if (res.error) {
        notify(res.error);
      } else {
        notify(`测速完成：当前下行约 ${res.download_mbps} Mbps`);
      }
    } catch (e) {
      notify(`测速失败：${String(e)}`);
    } finally {
      setTestingSpeed(false);
    }
  }

  async function handleOpenRouterAdmin() {
    if (!overview?.router.admin_url) return;
    try {
      await openUrl(overview.router.admin_url);
    } catch (e) {
      notify(`打开路由器后台失败：${String(e)}`);
    }
  }

  function handleOpenHardwareDrawer(device: DeviceView) {
    setHardwareActionResult(null);
    setActiveHardwareMac(device.mac);
  }

  async function handleRefreshSingleHardware(device: DeviceView) {
    setRefreshingHardware(true);
    try {
      const updated = await invoke<HardwareProfile | null>("probe_single_hardware", {
        ip: device.ip,
        mac: device.mac,
        vendor: device.vendor,
        hostname: device.hostname,
        isGateway: device.is_gateway,
      });
      setScan((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          devices: prev.devices.map((item) =>
            item.mac === device.mac ? { ...item, hardware: updated } : item
          ),
        };
      });
      notify("已刷新硬件实时状态");
    } catch (e) {
      notify(`刷新硬件状态失败：${String(e)}`);
    } finally {
      setRefreshingHardware(false);
    }
  }

  async function handleRunHardwareAction(
    device: DeviceView,
    action: CapabilityAction
  ) {
    if (action.id === "open_router_tab") {
      setActiveHardwareMac(null);
      setActiveTab("router");
      return;
    }
    if (action.url) {
      try {
        await openUrl(action.url);
      } catch (e) {
        notify(`打开链接失败：${String(e)}`);
      }
      return;
    }

    const key = `${device.mac}:${action.id}`;
    setRunningHardwareKey(key);
    setHardwareActionResult(null);
    try {
      const res = await invoke<HardwareActionResult>("execute_hardware_action", {
        ip: device.ip,
        mac: device.mac,
        actionId: action.id,
        params: {
          display_name: device.display_name,
          system_printer_name: device.hardware?.system_printer_name ?? null,
        },
      });
      setHardwareActionResult(res);
      if (res.success || !activeHardwareMac) {
        notify(res.message);
      } else {
        notify("未能自动完成添加，请按抽屉内提示操作");
      }
      if (res.refresh_needed) {
        await handleRefreshSingleHardware(device);
        await refreshPreferences();
      }
    } catch (e) {
      const errRes: HardwareActionResult = {
        success: false,
        message: String(e),
        detail: null,
        refresh_needed: false,
      };
      setHardwareActionResult(errRes);
      notify(String(e));
    } finally {
      setRunningHardwareKey(null);
    }
  }

  async function handleUpdateSettings(next: AppSettings) {
    try {
      const saved = await invoke<AppSettings>("update_app_settings", { settings: next });
      setPreferences((prev) => (prev ? { ...prev, settings: saved } : prev));
      notify("守护设置已保存");
    } catch (e) {
      notify(`保存设置失败：${String(e)}`);
    }
  }

  async function handleClearEvents() {
    try {
      await invoke("clear_network_events");
      setPreferences((prev) => (prev ? { ...prev, events: [] } : prev));
      notify("已清空网络动态记录");
    } catch (e) {
      notify(`清空失败：${String(e)}`);
    }
  }

  async function handleFinishOnboarding() {
    try {
      await invoke("mark_onboarding_done");
    } catch {
      // ignore
    }
    setPreferences((prev) => (prev ? { ...prev, onboarding_done: true } : prev));
  }

  async function handleToggleGuestWifi(enable: boolean) {
    setUpdatingGuest(true);
    try {
      const state = await invoke("set_guest_wifi", {
        enabled: enable,
        ssid: guestSsid.trim() || null,
        password: guestPassword.trim() || null,
        durationMins: enable ? guestDurationMins : null,
      });
      await refreshPreferences();
      if (enable) {
        setSelectedSsid(guestSsid.trim() || "MyHome_Guest");
        setWifiPassword(guestPassword.trim());
        notify("已开启访客 Wi-Fi，右侧二维码已切换为访客网络");
      } else {
        notify("已关闭访客 Wi-Fi");
      }
      void state;
    } catch (e) {
      notify(String(e));
    } finally {
      setUpdatingGuest(false);
    }
  }

  function handleCopyHealthReport() {
    if (!overview) return;
    const lines = [
      `【Wi-Fi 小管家 · 网络诊断报告】`,
      `生成时间：${new Date().toLocaleString("zh-CN")}`,
      `路由器：${overview.router.brand} (${overview.router.gateway_ip})`,
      `当前网络：${overview.ssid || "系统隐藏 SSID"} | ${overview.channel ?? "未知信道"} | 信号 ${overview.signal_dbm ?? "--"} dBm`,
      health
        ? `体检结论：${health.summary_title}（网关延迟 ${health.gateway_latency_ms ?? "--"} ms / 外网延迟 ${health.internet_latency_ms ?? "--"} ms）`
        : "体检结论：尚未运行延迟体检",
      speedResult?.download_mbps
        ? `下行测速：${speedResult.download_mbps} Mbps（${speedResult.quality_label}，节点：${speedResult.source}）`
        : "下行测速：尚未测速",
      health ? `排查建议：${health.summary_advice}` : "",
    ].filter(Boolean);

    navigator.clipboard.writeText(lines.join("\n")).then(
      () => notify("已复制完整诊断报告，可直接粘贴发给宽带客服或朋友"),
      () => notify("复制失败，请手动查看页面结果")
    );
  }

  function handleCopyWifiCard() {
    if (!selectedSsid.trim()) {
      notify("请先填写 Wi-Fi 名称");
      return;
    }
    const text = wifiPassword
      ? `Wi-Fi 名称：${selectedSsid}\nWi-Fi 密码：${wifiPassword}`
      : `Wi-Fi 名称：${selectedSsid}（无密码）`;
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
        notify("已复制到剪贴板，去微信直接粘贴即可");
      },
      () => notify("复制失败，请手动复制")
    );
  }

  const filteredDevices = useMemo(() => {
    const list = scan?.devices ?? [];
    const q = search.trim().toLowerCase();
    return list.filter((d) => {
      if (filter === "blocked") return d.blocked;
      if (d.blocked) return false;
      if (filter === "hardware" && !d.hardware) return false;
      if (filter === "new" && !d.is_new) return false;
      if (filter === "personal" && d.device_type !== "phone" && d.device_type !== "laptop") {
        return false;
      }
      if (filter === "iot" && d.device_type !== "iot" && d.device_type !== "router") {
        return false;
      }
      if (q) {
        const hay = `${d.display_name} ${d.default_name} ${d.vendor} ${d.ip} ${d.mac} ${d.hostname ?? ""}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [scan, filter, search]);

  const personalCount = useMemo(
    () =>
      (scan?.devices ?? []).filter(
        (d) => !d.is_gateway && !d.blocked && (d.device_type === "phone" || d.device_type === "laptop")
      ).length,
    [scan]
  );

  const iotCount = useMemo(
    () =>
      (scan?.devices ?? []).filter(
        (d) => !d.is_gateway && !d.blocked && d.device_type !== "phone" && d.device_type !== "laptop"
      ).length,
    [scan]
  );

  const hardwareCount = useMemo(
    () => (scan?.devices ?? []).filter((d) => !d.blocked && Boolean(d.hardware)).length,
    [scan]
  );

  const activeHardwareDevice = useMemo(
    () =>
      activeHardwareMac
        ? (scan?.devices ?? []).find((d) => d.mac === activeHardwareMac) ?? null
        : null,
    [scan, activeHardwareMac]
  );

  const sigMeta = getSignalMeta(overview?.signal_dbm ?? null);
  const newPwStrength = evaluatePasswordStrength(newWifiPassword);
  const isRouterBound = Boolean(
    preferences?.router_connected || preferences?.has_saved_router_password
  );
  const canRouterBlock = Boolean(
    isRouterBound && overview?.router.capabilities?.supports_block_mac
  );
  const canRouterWifi = Boolean(
    overview?.router.adapter_supported && overview?.router.capabilities?.supports_wifi_config
  );

  const tabMeta: Record<TabId, { title: string; subtitle: string }> = {
    devices: {
      title: "家庭硬件与雷达",
      subtitle: overview
        ? `子网 ${overview.subnet_cidr} · 自动识别路由器、Wi-Fi 打印机与智能设备`
        : "正在扫描局域网活跃硬件与终端...",
    },
    share: {
      title: "扫码连网与改密码",
      subtitle: "读取本机保存的密码生成亲友二维码，或通过路由器适配器一键修改密码",
    },
    health: {
      title: "一键网络体检",
      subtitle: "三段式排查卡顿源头 + 多节点真实下行测速（Mbps）",
    },
    router: {
      title: "路由器与守护",
      subtitle: "中兴 / OpenWrt 适配器绑定、后台定时巡检与最近网络动态",
    },
  };

  return (
    <div className="h-screen w-screen flex bg-[#f4f5f7] text-slate-900 dark:bg-[#0d0f14] dark:text-zinc-100 select-none overflow-hidden">
      {/* 左侧边栏 */}
      <aside className="w-60 shrink-0 border-r border-slate-200/80 dark:border-zinc-800/80 bg-white/80 dark:bg-zinc-900/70 backdrop-blur-xl flex flex-col justify-between p-4">
        <div className="space-y-6">
          {/* Brand */}
          <div className="flex items-center gap-3 px-1">
            <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 text-white flex items-center justify-center shadow-sm shadow-blue-500/25 shrink-0">
              <Wifi className="w-5 h-5" />
            </div>
            <div className="min-w-0">
              <div className="text-sm font-bold tracking-tight truncate">
                Wi-Fi 小管家
              </div>
              <div className="text-[11px] text-slate-500 dark:text-zinc-400 truncate">
                {overview?.router.brand ?? "正在识别网络..."}
              </div>
            </div>
          </div>

          {/* Nav */}
          <nav className="space-y-1">
            {(
              [
                {
                  id: "devices",
                  label: "家庭硬件与雷达",
                  icon: Radar,
                  badge: (scan?.new_count ?? 0) > 0 ? `${scan?.new_count}` : null,
                },
                { id: "share", label: "扫码连与改密码", icon: KeyRound, badge: null },
                { id: "health", label: "一键网络体检", icon: Activity, badge: null },
                { id: "router", label: "路由器与守护", icon: Settings2, badge: null },
              ] as const
            ).map((item) => {
              const Icon = item.icon;
              const active = activeTab === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setActiveTab(item.id)}
                  className={`w-full px-3 py-2.5 rounded-xl text-[13px] font-medium flex items-center justify-between transition-all cursor-pointer ${
                    active
                      ? "bg-blue-600 text-white shadow-sm shadow-blue-600/20"
                      : "text-slate-600 dark:text-zinc-300 hover:bg-slate-100/90 dark:hover:bg-zinc-800/70"
                  }`}
                >
                  <span className="flex items-center gap-2.5">
                    <Icon className="w-4 h-4 shrink-0" />
                    <span>{item.label}</span>
                  </span>
                  {item.badge && (
                    <span
                      className={`text-[11px] font-semibold px-1.5 py-0.5 rounded-full leading-none ${
                        active
                          ? "bg-white/20 text-white"
                          : "bg-amber-100 text-amber-700 dark:bg-amber-500/20 dark:text-amber-300"
                      }`}
                    >
                      {item.badge}
                    </span>
                  )}
                </button>
              );
            })}
          </nav>
        </div>

        {/* Bottom Live Wi-Fi Card */}
        <div className="rounded-2xl bg-slate-50 dark:bg-zinc-950/70 border border-slate-200/80 dark:border-zinc-800 p-3.5 space-y-2.5">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium text-slate-500 dark:text-zinc-400">
              当前无线状态
            </span>
            <SignalBars bars={sigMeta.bars} />
          </div>
          <div>
            <div className="text-xs font-semibold truncate">
              {overview?.ssid ||
                (overview?.ssid_redacted
                  ? overview.preferred_ssids[0] || "已连接 Wi-Fi"
                  : "未识别 SSID")}
            </div>
            <div className="text-[11px] text-slate-500 dark:text-zinc-400 mt-0.5 flex items-center justify-between">
              <span>{sigMeta.label}</span>
              {overview?.tx_rate_mbps && (
                <span className="font-mono">{overview.tx_rate_mbps} Mbps</span>
              )}
            </div>
          </div>
          <div className="pt-2 border-t border-slate-200/70 dark:border-zinc-800 flex items-center justify-between text-[11px] text-slate-500 dark:text-zinc-400">
            <span>{preferences?.settings.auto_scan_enabled ? "后台守护中" : "网关"}</span>
            <span className="font-mono text-slate-700 dark:text-zinc-300">
              {overview?.router.gateway_ip ?? "--"}
            </span>
          </div>
        </div>
      </aside>

      {/* 右侧主内容区 */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <header className="h-16 shrink-0 px-6 border-b border-slate-200/80 dark:border-zinc-800/80 bg-white/70 dark:bg-zinc-900/50 backdrop-blur-md flex items-center justify-between gap-4">
          <div className="min-w-0">
            <h1 className="text-base font-bold tracking-tight truncate">
              {tabMeta[activeTab].title}
            </h1>
            <p className="text-xs text-slate-500 dark:text-zinc-400 truncate mt-0.5">
              {tabMeta[activeTab].subtitle}
            </p>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            {activeTab === "devices" && (
              <button
                onClick={handleRescan}
                disabled={scanning}
                className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 shadow-xs shadow-blue-600/20 transition cursor-pointer"
              >
                <RefreshCw className={`w-3.5 h-3.5 ${scanning ? "animate-spin" : ""}`} />
                {scanning ? "扫描中..." : "刷新设备"}
              </button>
            )}
            {activeTab === "health" && (
              <>
                {(health || speedResult) && (
                  <button
                    onClick={handleCopyHealthReport}
                    className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 hover:bg-slate-50 dark:hover:bg-zinc-800 transition cursor-pointer"
                  >
                    <Copy className="w-3.5 h-3.5" />
                    复制诊断报告
                  </button>
                )}
                <button
                  onClick={handleRunSpeedTest}
                  disabled={testingSpeed}
                  className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 hover:bg-slate-50 dark:hover:bg-zinc-800 disabled:opacity-60 transition cursor-pointer"
                >
                  <Gauge className={`w-3.5 h-3.5 ${testingSpeed ? "animate-spin" : ""}`} />
                  {testingSpeed ? "测速中..." : "测真实网速"}
                </button>
                <button
                  onClick={handleRunHealth}
                  disabled={checkingHealth}
                  className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 shadow-xs shadow-blue-600/20 transition cursor-pointer"
                >
                  <Activity className={`w-3.5 h-3.5 ${checkingHealth ? "animate-pulse" : ""}`} />
                  {checkingHealth ? "正在体检..." : "开始体检"}
                </button>
              </>
            )}
          </div>
        </header>

        <main className="flex-1 overflow-y-auto p-6 space-y-5 select-text">
          {/* TAB 1: 设备雷达 */}
          {activeTab === "devices" && (
            <>
              {(scan?.new_count ?? 0) > 0 && (
                <div className="rounded-2xl bg-amber-50/90 dark:bg-amber-500/10 border border-amber-200/80 dark:border-amber-500/25 px-4 py-3 flex items-center justify-between gap-3">
                  <div className="flex items-center gap-2.5 text-xs text-amber-900 dark:text-amber-200">
                    <AlertTriangle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0" />
                    <span>
                      发现 <strong>{scan?.new_count}</strong> 台未备注的陌生设备。若是家人手机开启了“私有无线局域网地址”，点「备注」或「标为熟人」即可不再提醒。
                    </span>
                  </div>
                  <button
                    onClick={() => setFilter("new")}
                    className="text-xs font-medium px-2.5 py-1 rounded-lg bg-amber-200/70 hover:bg-amber-200 text-amber-900 dark:bg-amber-500/25 dark:text-amber-200 shrink-0 cursor-pointer"
                  >
                    只看新设备
                  </button>
                </div>
              )}

              {/* 3 张统计 Bento 卡片 */}
              <div className="grid grid-cols-3 gap-4">
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-4 shadow-2xs">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-500 dark:text-zinc-400">当前在线终端</span>
                    <span className="w-7 h-7 rounded-lg bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400 flex items-center justify-center">
                      <Radar className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-2xl font-bold mt-2 tabular-nums tracking-tight">
                    {scan ? `${scan.total_clients}` : "--"}
                    <span className="text-xs font-normal text-slate-400 ml-1">台</span>
                  </div>
                  <div className="text-[11px] text-slate-500 dark:text-zinc-400 mt-1.5">
                    可管理硬件 {hardwareCount} · 手机电脑 {personalCount} · 固定 {iotCount}
                  </div>
                </div>

                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-4 shadow-2xs">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-500 dark:text-zinc-400">待确认新设备</span>
                    <span className="w-7 h-7 rounded-lg bg-amber-50 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400 flex items-center justify-center">
                      <AlertTriangle className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-2xl font-bold mt-2 tabular-nums tracking-tight text-amber-600 dark:text-amber-400">
                    {scan ? `${scan.new_count}` : "--"}
                    <span className="text-xs font-normal text-slate-400 ml-1">台</span>
                  </div>
                  <div className="text-[11px] text-slate-500 dark:text-zinc-400 mt-1.5">
                    未备注过的陌生设备
                  </div>
                </div>

                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-4 shadow-2xs">
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-500 dark:text-zinc-400">无线规格与信道</span>
                    <span className="w-7 h-7 rounded-lg bg-emerald-50 text-emerald-600 dark:bg-emerald-500/15 dark:text-emerald-400 flex items-center justify-center">
                      <Wifi className="w-4 h-4" />
                    </span>
                  </div>
                  <div className="text-lg font-bold mt-2 truncate">
                    {overview?.phy_mode ?? "Wi-Fi"}
                  </div>
                  <div className="text-[11px] text-slate-500 dark:text-zinc-400 mt-1.5 truncate">
                    {overview?.channel ?? sigMeta.label}
                  </div>
                </div>
              </div>

              {/* 可管理家庭硬件卡片区 */}
              <HardwareCardsSection
                devices={scan?.devices ?? []}
                runningKey={runningHardwareKey}
                onOpenDrawer={handleOpenHardwareDrawer}
                onRunAction={handleRunHardwareAction}
              />

              {/* 列表卡片 */}
              <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl shadow-2xs overflow-hidden">
                <div className="px-4 py-3 border-b border-slate-100 dark:border-zinc-800 flex flex-wrap items-center justify-between gap-3">
                  <div className="inline-flex items-center p-1 rounded-xl bg-slate-100 dark:bg-zinc-950 gap-0.5">
                    {(
                      [
                        ["all", "全部"],
                        ["hardware", "可管理硬件"],
                        ["new", "陌生设备"],
                        ["personal", "手机/电脑"],
                        ["iot", "固定/家居"],
                        ["blocked", "已隐藏/拉黑"],
                      ] as [DeviceFilter, string][]
                    ).map(([id, label]) => (
                      <button
                        key={id}
                        onClick={() => setFilter(id)}
                        className={`text-xs px-2.5 py-1.5 rounded-lg font-medium transition cursor-pointer ${
                          filter === id
                            ? "bg-white dark:bg-zinc-800 text-slate-900 dark:text-white shadow-2xs"
                            : "text-slate-500 dark:text-zinc-400 hover:text-slate-800 dark:hover:text-zinc-200"
                        }`}
                      >
                        {label}
                      </button>
                    ))}
                  </div>

                  <div className="flex items-center gap-3">
                    <div className="relative">
                      <Search className="w-3.5 h-3.5 text-slate-400 absolute left-2.5 top-1/2 -translate-y-1/2" />
                      <input
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                        placeholder="搜索设备、品牌或 IP"
                        className="text-xs pl-8 pr-3 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500 w-44"
                      />
                    </div>

                    <button
                      type="button"
                      onClick={() => setShowTech((v) => !v)}
                      className="flex items-center gap-2 text-xs text-slate-500 dark:text-zinc-400 cursor-pointer"
                    >
                      <span>IP / MAC</span>
                      <span
                        className={`w-8 h-4.5 rounded-full p-0.5 transition-colors flex items-center ${
                          showTech ? "bg-blue-600 justify-end" : "bg-slate-200 dark:bg-zinc-700 justify-start"
                        }`}
                      >
                        <span className="w-3.5 h-3.5 rounded-full bg-white shadow-xs" />
                      </span>
                    </button>
                  </div>
                </div>

                <div className="divide-y divide-slate-100 dark:divide-zinc-800/80">
                  {scanning && !scan ? (
                    <div className="py-14 flex flex-col items-center justify-center gap-2 text-xs text-slate-500">
                      <RefreshCw className="w-5 h-5 animate-spin text-blue-500" />
                      <span>正在并发探测局域网设备与 SSDP 智能家居...</span>
                    </div>
                  ) : filteredDevices.length === 0 ? (
                    <div className="py-14 text-center text-xs text-slate-500">
                      当前筛选条件下没有设备
                    </div>
                  ) : (
                    filteredDevices.map((d) => (
                      <div
                        key={`${d.ip}-${d.mac}`}
                        className="px-4 py-3 flex items-center justify-between gap-4 hover:bg-slate-50/70 dark:hover:bg-zinc-800/30 transition-colors"
                      >
                        <div className="flex items-center gap-3 min-w-0">
                          <div
                            className={`w-9 h-9 rounded-xl flex items-center justify-center shrink-0 ${
                              d.is_self
                                ? "bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400"
                                : d.is_gateway
                                  ? "bg-indigo-50 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-400"
                                  : d.is_new
                                    ? "bg-amber-50 text-amber-600 dark:bg-amber-500/15 dark:text-amber-400"
                                    : "bg-slate-100 text-slate-600 dark:bg-zinc-800 dark:text-zinc-300"
                            }`}
                          >
                            {d.hardware ? (
                              <HardwareKindIcon kind={d.hardware.kind} />
                            ) : (
                              <DeviceIcon type={d.device_type} />
                            )}
                          </div>

                          <div className="min-w-0">
                            <div className="flex items-center gap-2 flex-wrap">
                              <span className="text-[13px] font-semibold truncate">
                                {d.display_name}
                              </span>
                              {d.hardware && !d.is_gateway && (
                                <span className="text-[11px] px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300 font-medium">
                                  {d.hardware.category_label}
                                </span>
                              )}
                              {d.is_self && (
                                <span className="text-[11px] px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300 font-medium">
                                  本机
                                </span>
                              )}
                              {d.is_gateway && (
                                <span className="text-[11px] px-2 py-0.5 rounded-full bg-indigo-50 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300 font-medium">
                                  主路由器
                                </span>
                              )}
                              {d.is_new && !d.blocked && (
                                <span className="text-[11px] px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-500/20 dark:text-amber-300 font-medium">
                                  新设备
                                </span>
                              )}
                              {d.trusted && !d.is_self && !d.is_gateway && !d.blocked && (
                                <span className="text-[11px] px-2 py-0.5 rounded-full bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300 font-medium">
                                  熟人
                                </span>
                              )}
                            </div>

                            <div className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5 flex items-center gap-1.5 flex-wrap">
                              <span
                                className={`w-1.5 h-1.5 rounded-full ${
                                  d.ping_alive ? "bg-emerald-500" : "bg-slate-300 dark:bg-zinc-600"
                                }`}
                              />
                              <span>{d.vendor}</span>
                              <span>·</span>
                              <span>
                                {d.latency_ms
                                  ? `${d.latency_ms} ms`
                                  : d.ping_alive
                                    ? "在线"
                                    : "静默在线"}
                              </span>
                            </div>

                            {showTech && (
                              <div className="text-[11px] font-mono text-slate-400 dark:text-zinc-500 mt-1">
                                {d.ip} · {d.mac}
                                {d.hostname ? ` · ${d.hostname}` : ""}
                              </div>
                            )}
                          </div>
                        </div>

                        <div className="flex items-center gap-1.5 shrink-0">
                          {!d.blocked ? (
                            <>
                              {d.hardware && (
                                <button
                                  onClick={() => handleOpenHardwareDrawer(d)}
                                  className="text-xs font-medium px-2.5 py-1.5 rounded-lg bg-blue-50 text-blue-700 hover:bg-blue-100 dark:bg-blue-500/15 dark:text-blue-300 transition cursor-pointer"
                                >
                                  管理
                                </button>
                              )}
                              {d.is_new && (
                                <button
                                  onClick={() => handleTrustDevice(d)}
                                  className="text-xs font-medium px-2.5 py-1.5 rounded-lg bg-emerald-50 text-emerald-700 hover:bg-emerald-100 dark:bg-emerald-500/15 dark:text-emerald-300 transition cursor-pointer"
                                >
                                  标为熟人
                                </button>
                              )}
                              {!d.is_gateway && (
                                <button
                                  onClick={() => {
                                    setRenamingDevice(d);
                                    setRenameInput(d.alias ?? d.display_name);
                                  }}
                                  className="inline-flex items-center gap-1 text-xs px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                                >
                                  <Pencil className="w-3 h-3" />
                                  备注
                                </button>
                              )}
                              {!d.is_self && !d.is_gateway && canRouterBlock && (
                                <button
                                  onClick={() => handleRouterBlock(d, true)}
                                  className="inline-flex items-center gap-1 text-xs px-2.5 py-1.5 rounded-lg text-rose-600 hover:bg-rose-50 dark:hover:bg-rose-500/10 transition cursor-pointer"
                                  title="通过路由器无线黑名单拉黑该设备"
                                >
                                  <Ban className="w-3 h-3" />
                                  拉黑
                                </button>
                              )}
                              {!d.is_self && !d.is_gateway && (
                                <button
                                  onClick={() => handleToggleHidden(d, true)}
                                  className="text-xs px-2.5 py-1.5 rounded-lg text-slate-400 hover:text-rose-600 hover:bg-rose-50 dark:hover:bg-rose-500/10 transition cursor-pointer"
                                >
                                  隐藏
                                </button>
                              )}
                            </>
                          ) : (
                            <div className="flex items-center gap-1.5">
                              {canRouterBlock && (
                                <button
                                  onClick={() => handleRouterBlock(d, false)}
                                  className="text-xs px-2.5 py-1.5 rounded-lg bg-blue-50 text-blue-600 hover:bg-blue-100 dark:bg-blue-500/15 dark:text-blue-300 transition cursor-pointer"
                                >
                                  解除拉黑
                                </button>
                              )}
                              <button
                                onClick={() => handleToggleHidden(d, false)}
                                className="text-xs px-2.5 py-1.5 rounded-lg bg-slate-100 dark:bg-zinc-800 hover:bg-slate-200 transition cursor-pointer"
                              >
                                恢复显示
                              </button>
                            </div>
                          )}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </>
          )}

          {/* TAB 2: 扫码连网与改密码 */}
          {activeTab === "share" && (
            <div className="grid grid-cols-1 lg:grid-cols-12 gap-5">
              <div className="lg:col-span-7 space-y-5">
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-4 shadow-2xs">
                  <div>
                    <h2 className="text-sm font-bold">读取当前 Wi-Fi 密码</h2>
                    <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
                      不需要路由器账号，直接从系统安全钥匙串获取密码。
                    </p>
                  </div>

                  {overview?.ssid_redacted && (
                    <div className="text-xs p-3 rounded-xl bg-amber-50 text-amber-800 dark:bg-amber-500/10 dark:text-amber-300 border border-amber-200/70 dark:border-amber-500/20">
                      macOS 未开启定位权限时会隐藏当前 Wi-Fi 名，已在右侧下拉框列出你最近连过的网络供直接选择。
                    </div>
                  )}

                  <div className="space-y-3">
                    <div>
                      <label className="block text-xs font-medium text-slate-500 dark:text-zinc-400 mb-1.5">
                        Wi-Fi 名称
                      </label>
                      <div className="flex gap-2">
                        <input
                          value={selectedSsid}
                          onChange={(e) => setSelectedSsid(e.target.value)}
                          placeholder="输入或选择 Wi-Fi 名称"
                          className="flex-1 text-sm px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                        {(overview?.preferred_ssids.length ?? 0) > 0 && (
                          <select
                            value={selectedSsid}
                            onChange={(e) => setSelectedSsid(e.target.value)}
                            className="text-xs px-2.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 max-w-44"
                          >
                            {overview?.preferred_ssids.map((s) => (
                              <option key={s} value={s}>
                                {s}
                              </option>
                            ))}
                          </select>
                        )}
                      </div>
                    </div>

                    <div>
                      <label className="block text-xs font-medium text-slate-500 dark:text-zinc-400 mb-1.5">
                        Wi-Fi 密码
                      </label>
                      <div className="flex gap-2">
                        <input
                          type={showPassword ? "text" : "password"}
                          value={wifiPassword}
                          onChange={(e) => setWifiPassword(e.target.value)}
                          placeholder="点击下方「读取本机密码」或手动输入"
                          className="flex-1 text-sm px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                        <button
                          type="button"
                          onClick={() => setShowPassword((v) => !v)}
                          className="px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                        >
                          {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                        </button>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 pt-1 flex-wrap">
                      <button
                        onClick={handleReadSavedPassword}
                        disabled={readingPw}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 transition cursor-pointer"
                      >
                        <Lock className="w-3.5 h-3.5" />
                        {readingPw ? "正在读取..." : "读取本机保存的密码"}
                      </button>
                      <button
                        onClick={handleCopyWifiCard}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-slate-100 dark:bg-zinc-800 hover:bg-slate-200 dark:hover:bg-zinc-700 transition cursor-pointer"
                      >
                        {copied ? <Check className="w-3.5 h-3.5 text-emerald-500" /> : <Copy className="w-3.5 h-3.5" />}
                        {copied ? "已复制" : "复制发微信"}
                      </button>
                    </div>
                  </div>
                </div>

                {/* 修改路由器 Wi-Fi 密码卡片（适配型号支持一键修改 + 强度检测 + 好记强密码生成器） */}
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3.5 shadow-2xs">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-bold">修改路由器 Wi-Fi 密码</h3>
                    <span className="text-[11px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-zinc-800 text-slate-600 dark:text-zinc-300">
                      {overview?.router.brand ?? "路由器"}
                    </span>
                  </div>

                  {canRouterWifi && isRouterBound ? (
                    <div className="space-y-3 pt-1">
                      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                        <div>
                          <label className="block text-[11px] font-medium text-slate-500 mb-1">
                            新 Wi-Fi 名称
                          </label>
                          <input
                            value={newWifiSsid}
                            onChange={(e) => setNewWifiSsid(e.target.value)}
                            placeholder="例如 MyHome_5G"
                            className="w-full text-xs px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                          />
                        </div>
                        <div>
                          <label className="block text-[11px] font-medium text-slate-500 mb-1">
                            新 Wi-Fi 密码（至少 8 位）
                          </label>
                          <div className="flex gap-1.5">
                            <input
                              type={showNewWifiPw ? "text" : "password"}
                              value={newWifiPassword}
                              onChange={(e) => setNewWifiPassword(e.target.value)}
                              placeholder="输入新密码"
                              className="flex-1 min-w-0 text-xs px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                            />
                            <button
                              type="button"
                              onClick={() => setShowNewWifiPw((v) => !v)}
                              className="px-2.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 cursor-pointer"
                            >
                              {showNewWifiPw ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                            </button>
                          </div>
                        </div>
                      </div>

                      {/* 密码强度条 + 生成器 */}
                      <div className="space-y-1.5">
                        <div className="flex items-center justify-between text-[11px]">
                          <span className={newPwStrength.textClass}>
                            密码强度：{newPwStrength.label}
                          </span>
                          <button
                            type="button"
                            onClick={() => {
                              const pw = generateMemorablePassword();
                              setNewWifiPassword(pw);
                              setShowNewWifiPw(true);
                            }}
                            className="inline-flex items-center gap-1 text-blue-600 dark:text-blue-400 hover:underline cursor-pointer font-medium"
                          >
                            <Wand2 className="w-3 h-3" />
                            生成好记强密码
                          </button>
                        </div>
                        <div className="h-1.5 rounded-full bg-slate-100 dark:bg-zinc-800 overflow-hidden">
                          <div
                            className={`h-full transition-all duration-300 ${newPwStrength.barClass}`}
                            style={{ width: `${newPwStrength.percent}%` }}
                          />
                        </div>
                      </div>

                      {!confirmingWifiSave ? (
                        <div className="flex items-center gap-2 pt-1 flex-wrap">
                          <button
                            onClick={() => setConfirmingWifiSave(true)}
                            disabled={!newWifiSsid.trim() || newWifiPassword.length < 8}
                            className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 transition cursor-pointer"
                          >
                            一键修改路由器密码
                          </button>
                          <button
                            onClick={handleOpenRouterAdmin}
                            className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                          >
                            <span>网页后台</span>
                            <ArrowUpRight className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ) : (
                        <div className="rounded-xl bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/25 p-3 space-y-2">
                          <div className="text-xs text-amber-900 dark:text-amber-200 font-medium">
                            修改后家里所有手机、电视和智能设备需要用新密码重新连一次，确定立即应用吗？
                          </div>
                          <div className="flex items-center gap-2">
                            <button
                              onClick={handleApplyRouterWifi}
                              disabled={updatingWifi}
                              className="text-xs font-semibold px-3 py-1.5 rounded-lg bg-blue-600 text-white hover:bg-blue-700 cursor-pointer"
                            >
                              {updatingWifi ? "正在提交..." : "确认修改并同步二维码"}
                            </button>
                            <button
                              onClick={() => setConfirmingWifiSave(false)}
                              className="text-xs px-2.5 py-1.5 rounded-lg text-slate-500 hover:bg-white/60 cursor-pointer"
                            >
                              取消
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  ) : (
                    <>
                      {canRouterWifi && !isRouterBound && (
                        <div className="text-xs rounded-xl bg-blue-50/70 dark:bg-blue-500/10 border border-blue-200/60 dark:border-blue-500/20 p-3 flex items-center justify-between gap-3">
                          <span className="text-slate-700 dark:text-zinc-200">
                            你的路由器已支持直接改密！先绑定管理员密码即可在这里一键修改。
                          </span>
                          <button
                            onClick={() => setActiveTab("router")}
                            className="text-xs font-semibold px-3 py-1.5 rounded-lg bg-blue-600 text-white hover:bg-blue-700 shrink-0 cursor-pointer"
                          >
                            去绑定
                          </button>
                        </div>
                      )}
                      <div className="space-y-2 pt-1">
                        {(overview?.router.guide_steps ?? []).map((step, i) => (
                          <div key={i} className="flex items-start gap-2.5 text-xs text-slate-600 dark:text-zinc-300">
                            <span className="w-5 h-5 rounded-full bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400 font-semibold flex items-center justify-center shrink-0 text-[11px]">
                              {i + 1}
                            </span>
                            <span className="pt-0.5 leading-relaxed">{step}</span>
                          </div>
                        ))}
                      </div>
                      <div className="pt-1">
                        <button
                          onClick={handleOpenRouterAdmin}
                          className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                        >
                          <span>打开路由器后台 ({overview?.router.admin_url})</span>
                          <ArrowUpRight className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </>
                  )}
                </div>
              </div>

              {/* 右侧：分享二维码通行卡 + 临时访客 Wi-Fi */}
              <div className="lg:col-span-5 space-y-5">
                <div className="bg-gradient-to-b from-white to-slate-50 dark:from-zinc-900 dark:to-zinc-950 border border-slate-200/80 dark:border-zinc-800 rounded-3xl p-6 flex flex-col items-center text-center space-y-4 shadow-sm">
                  <div className="inline-flex items-center gap-1.5 text-xs font-medium text-blue-600 dark:text-blue-400 bg-blue-50 dark:bg-blue-500/10 px-3 py-1 rounded-full">
                    <Sparkles className="w-3.5 h-3.5" />
                    <span>亲友扫码连网通行卡</span>
                  </div>

                  <div className="p-3 rounded-2xl bg-white border border-slate-200/80 shadow-xs">
                    {selectedSsid.trim() ? (
                      <canvas ref={qrCanvasRef} className="block" />
                    ) : (
                      <div className="w-44 h-44 flex items-center justify-center text-xs text-slate-400">
                        选择 Wi-Fi 名称后生成
                      </div>
                    )}
                  </div>

                  <div className="space-y-1">
                    <div className="text-sm font-bold">{selectedSsid || "未选择网络"}</div>
                    <div className="text-xs text-slate-500 dark:text-zinc-400">
                      {wifiPassword ? "已包含密码 · 扫一扫直接连接" : "暂未填入密码 · 仅名称"}
                    </div>
                  </div>

                  <p className="text-[11px] text-slate-400 dark:text-zinc-500 leading-relaxed max-w-xs">
                    请使用 iPhone 或安卓手机的系统自带相机扫码即可一键连入 Wi-Fi（微信扫一扫不支持直接连网，远程发送请点左侧「复制发微信」）。
                  </p>
                </div>

                {/* 临时访客 Wi-Fi 控制卡 */}
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3 shadow-2xs">
                  <div className="flex items-center justify-between gap-2">
                    <div>
                      <h3 className="text-sm font-bold">临时访客 Wi-Fi</h3>
                      <p className="text-[11px] text-slate-500 dark:text-zinc-400">
                        客人来时开独立网络，到期自动关闭不泄密
                      </p>
                    </div>
                    <span
                      className={`text-[11px] px-2 py-0.5 rounded-full font-medium ${
                        preferences?.guest_wifi?.enabled
                          ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300"
                          : "bg-slate-100 text-slate-500 dark:bg-zinc-800 dark:text-zinc-400"
                      }`}
                    >
                      {preferences?.guest_wifi?.enabled ? "已开启" : "未开启"}
                    </span>
                  </div>

                  {overview?.router.capabilities?.supports_guest_wifi && isRouterBound ? (
                    <div className="space-y-2.5 text-xs">
                      <div className="grid grid-cols-2 gap-2">
                        <input
                          value={guestSsid}
                          onChange={(e) => setGuestSsid(e.target.value)}
                          placeholder="访客网络名"
                          className="px-2.5 py-1.5 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                        />
                        <input
                          value={guestPassword}
                          onChange={(e) => setGuestPassword(e.target.value)}
                          placeholder="密码(留空无密码)"
                          className="px-2.5 py-1.5 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                        />
                      </div>
                      <div className="flex items-center justify-between gap-2">
                        <select
                          value={guestDurationMins}
                          onChange={(e) => setGuestDurationMins(Number(e.target.value))}
                          className="px-2.5 py-1.5 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                        >
                          <option value={120}>2 小时后自动关闭</option>
                          <option value={240}>4 小时后自动关闭</option>
                          <option value={720}>12 小时后自动关闭</option>
                          <option value={0}>手动关闭</option>
                        </select>
                        <button
                          type="button"
                          disabled={updatingGuest}
                          onClick={() => handleToggleGuestWifi(!preferences?.guest_wifi?.enabled)}
                          className={`px-3 py-1.5 rounded-xl font-medium text-white cursor-pointer disabled:opacity-60 ${
                            preferences?.guest_wifi?.enabled
                              ? "bg-rose-600 hover:bg-rose-700"
                              : "bg-blue-600 hover:bg-blue-700"
                          }`}
                        >
                          {updatingGuest
                            ? "处理中..."
                            : preferences?.guest_wifi?.enabled
                              ? "立即关闭访客网"
                              : "开启并生成二维码"}
                        </button>
                      </div>
                    </div>
                  ) : (
                    <div className="text-[11px] text-slate-500 dark:text-zinc-400 leading-relaxed">
                      {overview?.router.capabilities?.supports_guest_wifi
                        ? "请先去「路由器与守护」绑定管理员账号即可一键开关访客网络。"
                        : "当前路由器暂不支持远程开关访客网络（支持小米 / TP-Link / OpenWrt），可在路由器网页后台中开启。"}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* TAB 3: 一键网络体检 + 带宽测速 */}
          {activeTab === "health" && (
            <div className="space-y-5">
              {!health ? (
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-10 flex flex-col items-center text-center space-y-4 shadow-2xs">
                  <div className="w-14 h-14 rounded-2xl bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400 flex items-center justify-center">
                    <Activity className="w-7 h-7" />
                  </div>
                  <div className="space-y-1 max-w-md">
                    <h2 className="text-base font-bold">网络卡顿？2 秒查明原因</h2>
                    <p className="text-xs text-slate-500 dark:text-zinc-400 leading-relaxed">
                      会分别检测「本机到路由器信号延迟」和「路由器到外部宽带连通性」，直接告诉你该怎么解决。
                    </p>
                  </div>
                  <div className="flex items-center gap-2.5 flex-wrap justify-center">
                    <button
                      onClick={handleRunHealth}
                      disabled={checkingHealth}
                      className="inline-flex items-center gap-2 text-xs font-semibold px-5 py-2.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 shadow-sm shadow-blue-600/20 transition cursor-pointer"
                    >
                      <Activity className={`w-4 h-4 ${checkingHealth ? "animate-pulse" : ""}`} />
                      {checkingHealth ? "正在检测中..." : "立即开始体检"}
                    </button>
                    <button
                      onClick={handleRunSpeedTest}
                      disabled={testingSpeed}
                      className="inline-flex items-center gap-2 text-xs font-semibold px-5 py-2.5 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-50 dark:hover:bg-zinc-800 disabled:opacity-60 transition cursor-pointer"
                    >
                      <Gauge className={`w-4 h-4 ${testingSpeed ? "animate-spin" : ""}`} />
                      {testingSpeed ? "测速中..." : "测一下真实网速"}
                    </button>
                  </div>
                </div>
              ) : (
                <>
                  <div
                    className={`rounded-2xl p-5 border flex items-start gap-4 shadow-2xs ${
                      health.overall_status === "good"
                        ? "bg-emerald-50/70 border-emerald-200 dark:bg-emerald-500/10 dark:border-emerald-500/25"
                        : health.overall_status === "warning"
                          ? "bg-amber-50/70 border-amber-200 dark:bg-amber-500/10 dark:border-amber-500/25"
                          : "bg-rose-50/70 border-rose-200 dark:bg-rose-500/10 dark:border-rose-500/25"
                    }`}
                  >
                    <div className="mt-0.5 shrink-0">
                      {health.overall_status === "good" ? (
                        <CheckCircle2 className="w-6 h-6 text-emerald-600 dark:text-emerald-400" />
                      ) : (
                        <AlertTriangle
                          className={`w-6 h-6 ${
                            health.overall_status === "warning"
                              ? "text-amber-600 dark:text-amber-400"
                              : "text-rose-600 dark:text-rose-400"
                          }`}
                        />
                      )}
                    </div>
                    <div className="space-y-1">
                      <div className="text-sm font-bold">{health.summary_title}</div>
                      <p className="text-xs text-slate-600 dark:text-zinc-300 leading-relaxed">
                        {health.summary_advice}
                      </p>
                    </div>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                    {health.steps.map((s) => (
                      <div
                        key={s.id}
                        className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-4 space-y-2 shadow-2xs"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-xs font-semibold">{s.title}</span>
                          <span
                            className={`text-[11px] font-medium px-2 py-0.5 rounded-full ${
                              s.status === "good"
                                ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300"
                                : s.status === "warning"
                                  ? "bg-amber-50 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300"
                                  : "bg-rose-50 text-rose-700 dark:bg-rose-500/15 dark:text-rose-300"
                            }`}
                          >
                            {s.badge}
                          </span>
                        </div>
                        <p className="text-xs text-slate-500 dark:text-zinc-400 leading-relaxed">
                          {s.detail}
                        </p>
                      </div>
                    ))}
                  </div>
                </>
              )}

              {/* 真实带宽测速卡 */}
              <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 flex flex-col sm:flex-row sm:items-center justify-between gap-4 shadow-2xs">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <Gauge className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                    <h3 className="text-sm font-bold">真实下载带宽测速</h3>
                    {speedResult && (
                      <span className="text-[11px] px-2 py-0.5 rounded-full bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300 font-medium">
                        {speedResult.quality_label}
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-slate-500 dark:text-zinc-400">
                    {speedResult
                      ? `${speedResult.advice}（节点：${speedResult.source}）`
                      : "测试真实下行带宽（Mbps），直观告诉你当前网络适不适合看 4K 视频或开会。"}
                  </p>
                </div>

                <div className="flex items-center gap-4 shrink-0">
                  {speedResult?.download_mbps !== null && speedResult?.download_mbps !== undefined && (
                    <div className="text-right">
                      <div className="text-2xl font-bold tabular-nums tracking-tight text-blue-600 dark:text-blue-400">
                        {speedResult.download_mbps}
                        <span className="text-xs font-normal text-slate-400 ml-1">Mbps</span>
                      </div>
                    </div>
                  )}
                  <button
                    onClick={handleRunSpeedTest}
                    disabled={testingSpeed}
                    className="inline-flex items-center gap-1.5 text-xs font-medium px-4 py-2 rounded-xl bg-slate-100 hover:bg-slate-200 dark:bg-zinc-800 dark:hover:bg-zinc-700 disabled:opacity-60 transition cursor-pointer"
                  >
                    <Gauge className={`w-3.5 h-3.5 ${testingSpeed ? "animate-spin" : ""}`} />
                    {testingSpeed ? "测速中 (约 5 秒)..." : speedResult ? "重新测速" : "开始测速"}
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* TAB 4: 路由器与守护 */}
          {activeTab === "router" && (
            <RouterGuardianTab
              overview={overview}
              preferences={preferences}
              devices={scan?.devices ?? []}
              onOpenRouterAdmin={handleOpenRouterAdmin}
              onUpdateSettings={handleUpdateSettings}
              onClearEvents={handleClearEvents}
              onRefreshPreferences={refreshPreferences}
              notify={notify}
            />
          )}
        </main>
      </div>

      {/* 首次启动 30 秒引导向导 */}
      {preferences && !preferences.onboarding_done && (
        <OnboardingModal
          overview={overview}
          devices={scan?.devices ?? []}
          onSaveAlias={saveAliasByMac}
          onFinish={handleFinishOnboarding}
        />
      )}

      {/* 底部浮动 Toast */}
      {toast && (
        <div className="fixed bottom-5 right-5 z-50 bg-slate-900 dark:bg-zinc-800 text-white text-xs px-4 py-2.5 rounded-xl shadow-lg flex items-center gap-3">
          <span>{toast}</span>
          <button
            onClick={() => setToast(null)}
            className="text-slate-400 hover:text-white cursor-pointer"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* 右侧硬件管理详情抽屉 */}
      <HardwareDrawer
        device={activeHardwareDevice}
        runningKey={runningHardwareKey}
        refreshing={refreshingHardware}
        lastResult={hardwareActionResult}
        onClose={() => setActiveHardwareMac(null)}
        onRunAction={handleRunHardwareAction}
        onRefreshSingle={handleRefreshSingleHardware}
        onRename={(d) => {
          setRenamingDevice(d);
          setRenameInput(d.alias ?? d.display_name);
        }}
        notify={notify}
      />

      {/* 重命名弹窗 */}
      {renamingDevice && (
        <div className="fixed inset-0 bg-black/40 backdrop-blur-xs flex items-center justify-center p-4 z-50">
          <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 w-full max-w-sm space-y-3.5 shadow-xl">
            <div>
              <div className="text-sm font-bold">给设备起个好记的名字</div>
              <div className="text-xs text-slate-500 mt-0.5">
                例如：“我的电脑”、“客厅电视”、“妈妈的手机”
              </div>
            </div>
            <input
              autoFocus
              value={renameInput}
              onChange={(e) => setRenameInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSaveAlias()}
              className="w-full text-sm px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setRenamingDevice(null)}
                className="text-xs px-3 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 cursor-pointer"
              >
                取消
              </button>
              <button
                onClick={handleSaveAlias}
                className="text-xs font-medium px-3.5 py-1.5 rounded-lg bg-blue-600 text-white hover:bg-blue-700 cursor-pointer"
              >
                保存备注
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
