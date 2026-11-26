import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";
import {
  Activity,
  AlertTriangle,
  ArrowUpRight,
  Check,
  CheckCircle2,
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  Laptop,
  Lock,
  Pencil,
  Radar,
  RefreshCw,
  Router,
  Search,
  Settings2,
  ShieldCheck,
  Smartphone,
  Sparkles,
  Tv,
  Wifi,
  X,
} from "lucide-react";
import type {
  DeviceView,
  HealthReport,
  NetworkOverview,
  ScanResponse,
  WifiPasswordResult,
} from "./types";

type TabId = "devices" | "share" | "health" | "router";
type DeviceFilter = "all" | "new" | "personal" | "iot" | "blocked";

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
  const [scan, setScan] = useState<ScanResponse | null>(null);
  const [scanning, setScanning] = useState(false);

  const [filter, setFilter] = useState<DeviceFilter>("all");
  const [search, setSearch] = useState("");
  const [showTech, setShowTech] = useState(false);

  const [renamingDevice, setRenamingDevice] = useState<DeviceView | null>(null);
  const [renameInput, setRenameInput] = useState("");

  // 分享与密码
  const [selectedSsid, setSelectedSsid] = useState("");
  const [wifiPassword, setWifiPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [readingPw, setReadingPw] = useState(false);
  const [copied, setCopied] = useState(false);
  const qrCanvasRef = useRef<HTMLCanvasElement | null>(null);

  // 体检
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);

  // Toast
  const [toast, setToast] = useState<string | null>(null);

  const notify = (msg: string) => setToast(msg);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(t);
  }, [toast]);

  async function loadOverviewAndScan() {
    setScanning(true);
    try {
      const ov = await invoke<NetworkOverview>("get_network_overview");
      setOverview(ov);
      const initialSsid = ov.ssid || ov.preferred_ssids[0] || "";
      setSelectedSsid((prev) => prev || initialSsid);
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
    } catch (e) {
      notify(`读取网络信息失败：${String(e)}`);
    } finally {
      setScanning(false);
    }
  }

  useEffect(() => {
    loadOverviewAndScan();
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
      notify(`扫描完成，当前共有 ${res.total_clients} 台终端在线`);
    } catch (e) {
      notify(`扫描出错：${String(e)}`);
    } finally {
      setScanning(false);
    }
  }

  async function handleSaveAlias() {
    if (!renamingDevice) return;
    try {
      await invoke("save_device_meta", {
        mac: renamingDevice.mac,
        alias: renameInput,
        trusted: true,
        blocked: null,
      });
      setRenamingDevice(null);
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
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

  async function handleOpenRouterAdmin() {
    if (!overview?.router.admin_url) return;
    try {
      await openUrl(overview.router.admin_url);
    } catch (e) {
      notify(`打开路由器后台失败：${String(e)}`);
    }
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

  const sigMeta = getSignalMeta(overview?.signal_dbm ?? null);

  const tabMeta: Record<TabId, { title: string; subtitle: string }> = {
    devices: {
      title: "谁在用我的网",
      subtitle: overview
        ? `子网 ${overview.subnet_cidr} · 已为你自动识别品牌与隐私随机地址`
        : "正在扫描局域网活跃设备...",
    },
    share: {
      title: "扫码连网与改密码",
      subtitle: "读取本机保存的密码生成亲友二维码，或一键前往路由器修改密码",
    },
    health: {
      title: "一键网络体检",
      subtitle: "快速定位卡顿源头：信号弱、带宽被占还是外网宽带故障",
    },
    router: {
      title: "路由器与版本说明",
      subtitle: "自动识别网关型号，了解 V0.1 通用能力与后续控制规划",
    },
  };

  return (
    <div className="h-screen w-screen flex bg-[#f4f5f7] text-slate-900 dark:bg-[#0d0f14] dark:text-zinc-100 select-none overflow-hidden">
      {/* 左侧 macOS 风格边栏 */}
      <aside className="w-60 shrink-0 border-r border-slate-200/80 dark:border-zinc-800/80 bg-white/80 dark:bg-zinc-900/70 backdrop-blur-xl flex flex-col justify-between p-4">
        <div className="space-y-6">
          {/* Brand */}
          <div className="flex items-center gap-3 px-1">
            <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 text-white flex items-center justify-center shadow-sm shadow-blue-500/25 shrink-0">
              <Wifi className="w-5 h-5" />
            </div>
            <div className="min-w-0">
              <div className="text-sm font-bold tracking-tight truncate">Wi-Fi 小管家</div>
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
                  label: "谁在用我的网",
                  icon: Radar,
                  badge:
                    (scan?.new_count ?? 0) > 0 ? `${scan?.new_count}` : null,
                },
                { id: "share", label: "扫码连与改密码", icon: KeyRound, badge: null },
                { id: "health", label: "一键网络体检", icon: Activity, badge: null },
                { id: "router", label: "路由器与说明", icon: Settings2, badge: null },
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
            <span>网关</span>
            <span className="font-mono text-slate-700 dark:text-zinc-300">
              {overview?.router.gateway_ip ?? "--"}
            </span>
          </div>
        </div>
      </aside>

      {/* 右侧主内容区 */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* 顶部标题栏 */}
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
              <button
                onClick={handleRunHealth}
                disabled={checkingHealth}
                className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 shadow-xs shadow-blue-600/20 transition cursor-pointer"
              >
                <Activity className={`w-3.5 h-3.5 ${checkingHealth ? "animate-pulse" : ""}`} />
                {checkingHealth ? "正在体检..." : "开始体检"}
              </button>
            )}
          </div>
        </header>

        {/* 内容滚动区 */}
        <main className="flex-1 overflow-y-auto p-6 space-y-5 select-text">
          {/* TAB 1: 设备雷达 */}
          {activeTab === "devices" && (
            <>
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
                    手机/电脑 {personalCount} · 固定/家居 {iotCount}
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

              {/* 列表卡片 */}
              <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl shadow-2xs overflow-hidden">
                <div className="px-4 py-3 border-b border-slate-100 dark:border-zinc-800 flex flex-wrap items-center justify-between gap-3">
                  {/* 分段筛选 */}
                  <div className="inline-flex items-center p-1 rounded-xl bg-slate-100 dark:bg-zinc-950 gap-0.5">
                    {(
                      [
                        ["all", "全部"],
                        ["new", "陌生设备"],
                        ["personal", "手机/电脑"],
                        ["iot", "固定/家居"],
                        ["blocked", "已隐藏"],
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

                  {/* 搜索与技术开关 */}
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

                {/* 设备行 */}
                <div className="divide-y divide-slate-100 dark:divide-zinc-800/80">
                  {scanning && !scan ? (
                    <div className="py-14 flex flex-col items-center justify-center gap-2 text-xs text-slate-500">
                      <RefreshCw className="w-5 h-5 animate-spin text-blue-500" />
                      <span>正在探测局域网设备，请稍候...</span>
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
                            <DeviceIcon type={d.device_type} />
                          </div>

                          <div className="min-w-0">
                            <div className="flex items-center gap-2 flex-wrap">
                              <span className="text-[13px] font-semibold truncate">
                                {d.display_name}
                              </span>
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
                            <button
                              onClick={() => handleToggleHidden(d, false)}
                              className="text-xs px-2.5 py-1.5 rounded-lg bg-slate-100 dark:bg-zinc-800 hover:bg-slate-200 transition cursor-pointer"
                            >
                              恢复显示
                            </button>
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

                {/* 改密码引导卡 */}
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3 shadow-2xs">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-bold">修改路由器 Wi-Fi 密码</h3>
                    <span className="text-[11px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-zinc-800 text-slate-600 dark:text-zinc-300">
                      {overview?.router.brand ?? "路由器"}
                    </span>
                  </div>
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
                </div>
              </div>

              {/* 右侧：精致的分享二维码通行卡 */}
              <div className="lg:col-span-5">
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
                    打开 iPhone 相机、安卓相机或微信扫一扫，对准上方二维码即可一键连入 Wi-Fi。
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* TAB 3: 一键网络体检 */}
          {activeTab === "health" && (
            <div className="space-y-5">
              {!health ? (
                <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-12 flex flex-col items-center text-center space-y-4 shadow-2xs">
                  <div className="w-14 h-14 rounded-2xl bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400 flex items-center justify-center">
                    <Activity className="w-7 h-7" />
                  </div>
                  <div className="space-y-1 max-w-md">
                    <h2 className="text-base font-bold">网络卡顿？2 秒查明原因</h2>
                    <p className="text-xs text-slate-500 dark:text-zinc-400 leading-relaxed">
                      会分别检测「本机到路由器信号延迟」和「路由器到外部宽带连通性」，直接告诉你该怎么解决。
                    </p>
                  </div>
                  <button
                    onClick={handleRunHealth}
                    disabled={checkingHealth}
                    className="inline-flex items-center gap-2 text-xs font-semibold px-5 py-2.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 shadow-sm shadow-blue-600/20 transition cursor-pointer"
                  >
                    <Activity className={`w-4 h-4 ${checkingHealth ? "animate-pulse" : ""}`} />
                    {checkingHealth ? "正在检测中..." : "立即开始体检"}
                  </button>
                </div>
              ) : (
                <>
                  {/* 结论横幅 */}
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

                  {/* 三步卡片 */}
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
            </div>
          )}

          {/* TAB 4: 路由器与说明 */}
          {activeTab === "router" && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-4 shadow-2xs">
                <div className="flex items-center justify-between">
                  <h2 className="text-sm font-bold">当前路由器识别信息</h2>
                  <ShieldCheck className="w-4 h-4 text-emerald-500" />
                </div>

                <div className="text-xs divide-y divide-slate-100 dark:divide-zinc-800">
                  <div className="py-2.5 flex justify-between">
                    <span className="text-slate-500 dark:text-zinc-400">识别品牌</span>
                    <span className="font-semibold">{overview?.router.brand ?? "--"}</span>
                  </div>
                  <div className="py-2.5 flex justify-between">
                    <span className="text-slate-500 dark:text-zinc-400">管理后台</span>
                    <span className="font-mono">{overview?.router.admin_url ?? "--"}</span>
                  </div>
                  <div className="py-2.5 flex justify-between">
                    <span className="text-slate-500 dark:text-zinc-400">网关 MAC</span>
                    <span className="font-mono">{overview?.router.gateway_mac ?? "--"}</span>
                  </div>
                  <div className="py-2.5 flex justify-between">
                    <span className="text-slate-500 dark:text-zinc-400">页面标题</span>
                    <span className="truncate max-w-44">
                      {overview?.router.page_title ?? "默认管理页"}
                    </span>
                  </div>
                </div>

                <button
                  onClick={handleOpenRouterAdmin}
                  className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 transition cursor-pointer"
                >
                  <span>在浏览器打开后台</span>
                  <ArrowUpRight className="w-3.5 h-3.5" />
                </button>
              </div>

              <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3 shadow-2xs">
                <h2 className="text-sm font-bold">版本路线（V0.1 → V0.2）</h2>
                <p className="text-xs text-slate-500 dark:text-zinc-400 leading-relaxed">
                  当前是 V0.1 通用基础版：无需登录路由器即可看清局域网设备、做陌生设备备注、读本机密码扫码分享及网络体检。
                </p>
                <div className="rounded-xl bg-slate-50 dark:bg-zinc-950 border border-slate-200/70 dark:border-zinc-800 p-3.5 space-y-2 text-xs text-slate-600 dark:text-zinc-300">
                  <div className="font-semibold text-slate-800 dark:text-zinc-100">
                    即将上线（V0.2）：
                  </div>
                  <ul className="space-y-1">
                    <li>• 对接中兴 / 小米 / TP-Link / OpenWrt 一键改密与踢人</li>
                    <li>• 后台定时静默巡检 + 新设备上线桌面弹窗提醒</li>
                    <li>• 真实带宽测速（Mbps）与智能家居 SSDP 设备识别</li>
                  </ul>
                </div>
              </div>
            </div>
          )}
        </main>
      </div>

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
