import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Copy,
  Eye,
  EyeOff,
  ExternalLink,
  KeyRound,
  Laptop,
  Pencil,
  Radar,
  RefreshCw,
  Router,
  Search,
  Settings,
  ShieldCheck,
  Smartphone,
  Tv,
  Wifi,
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

function signalText(dbm: number | null): string {
  if (dbm === null) return "信号未知";
  if (dbm >= -55) return "信号极佳";
  if (dbm >= -68) return "信号良好";
  if (dbm >= -78) return "信号一般";
  return "信号偏弱";
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

  // Wi-Fi 分享与密码
  const [selectedSsid, setSelectedSsid] = useState("");
  const [wifiPassword, setWifiPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [readingPw, setReadingPw] = useState(false);
  const qrCanvasRef = useRef<HTMLCanvasElement | null>(null);

  // 一键体检
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [checkingHealth, setCheckingHealth] = useState(false);

  // 全局提示
  const [toast, setToast] = useState<string | null>(null);

  const notify = (msg: string) => {
    setToast(msg);
  };

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4000);
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
      notify(`读取网络失败：${String(e)}`);
    } finally {
      setScanning(false);
    }
  }

  useEffect(() => {
    loadOverviewAndScan();
  }, []);

  // 绘制扫码连 Wi-Fi 二维码
  useEffect(() => {
    const canvas = qrCanvasRef.current;
    if (!canvas || !selectedSsid.trim()) return;
    const escapeField = (v: string) => v.replace(/([\\;,:"])/g, "\\$1");
    const payload = wifiPassword
      ? `WIFI:T:WPA;S:${escapeField(selectedSsid)};P:${escapeField(wifiPassword)};;`
      : `WIFI:T:nopass;S:${escapeField(selectedSsid)};;`;
    QRCode.toCanvas(canvas, payload, {
      width: 176,
      margin: 2,
      color: { dark: "#0f172a", light: "#ffffff" },
    }).catch(() => {});
  }, [selectedSsid, wifiPassword, activeTab]);

  async function handleRescan() {
    setScanning(true);
    try {
      const res = await invoke<ScanResponse>("scan_devices");
      setScan(res);
      notify(`扫描完成：当前局域网在线 ${res.total_clients} 台终端`);
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
      await handleRescanOnlyMetaRefresh();
      notify(`已备注为「${renameInput.trim() || renamingDevice.default_name}」`);
    } catch (e) {
      notify(`保存失败：${String(e)}`);
    }
  }

  async function handleRescanOnlyMetaRefresh() {
    const res = await invoke<ScanResponse>("scan_devices");
    setScan(res);
  }

  async function handleTrustDevice(d: DeviceView) {
    try {
      await invoke("save_device_meta", {
        mac: d.mac,
        alias: null,
        trusted: true,
        blocked: false,
      });
      await handleRescanOnlyMetaRefresh();
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
      await handleRescanOnlyMetaRefresh();
      notify(blocked ? `已将「${d.display_name}」加入本地关注/屏蔽列表` : `已恢复「${d.display_name}」显示`);
    } catch (e) {
      notify(`操作失败：${String(e)}`);
    }
  }

  async function handleReadSavedPassword() {
    if (!selectedSsid.trim()) {
      notify("请先输入或选择当前的 Wi-Fi 名称");
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
        notify(`已成功读取「${res.ssid}」的保存密码`);
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
      notify("网络体检完成");
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
      () => notify("已复制 Wi-Fi 信息，可以直接去微信粘贴发送"),
      () => notify("复制失败，请手动选中复制")
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

  const personalCount = useMemo(() => {
    return (scan?.devices ?? []).filter(
      (d) => !d.is_gateway && !d.blocked && (d.device_type === "phone" || d.device_type === "laptop")
    ).length;
  }, [scan]);

  const iotCount = useMemo(() => {
    return (scan?.devices ?? []).filter(
      (d) => !d.is_gateway && !d.blocked && d.device_type !== "phone" && d.device_type !== "laptop"
    ).length;
  }, [scan]);

  return (
    <div className="min-h-screen bg-slate-50 text-slate-900 dark:bg-zinc-950 dark:text-zinc-100">
      <div className="max-w-6xl mx-auto p-4 md:p-6 space-y-4">
        {/* 顶部栏 */}
        <header className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 shadow-xs">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-blue-600 text-white flex items-center justify-center shrink-0">
              <Wifi className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center gap-2 flex-wrap">
                <h1 className="text-base font-bold">Wi-Fi 小管家</h1>
                <span className="text-xs px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300 font-medium">
                  {overview?.router.brand ?? "正在识别路由器..."}
                </span>
              </div>
              <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
                {overview
                  ? `${overview.ssid || (overview.ssid_redacted ? "当前 Wi-Fi（系统隐藏名称）" : "已连接网络")} · ${signalText(overview.signal_dbm)} · 网关 ${overview.router.gateway_ip}`
                  : "正在读取本机网络信息..."}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={handleRescan}
              disabled={scanning}
              className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 transition cursor-pointer"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${scanning ? "animate-spin" : ""}`} />
              {scanning ? "正在扫描设备..." : "刷新设备列表"}
            </button>
          </div>
        </header>

        {toast && (
          <div className="bg-slate-900 text-white dark:bg-zinc-800 text-xs px-4 py-2.5 rounded-xl flex items-center justify-between shadow-sm">
            <span>{toast}</span>
            <button onClick={() => setToast(null)} className="text-slate-300 hover:text-white cursor-pointer">
              知道了
            </button>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
          {/* 左侧导航 */}
          <aside className="lg:col-span-3 space-y-3">
            <nav className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-2 space-y-1">
              <button
                onClick={() => setActiveTab("devices")}
                className={`w-full text-left px-3 py-2.5 rounded-xl text-sm font-medium flex items-center justify-between transition cursor-pointer ${
                  activeTab === "devices"
                    ? "bg-blue-600 text-white"
                    : "hover:bg-slate-100 dark:hover:bg-zinc-800"
                }`}
              >
                <span className="flex items-center gap-2">
                  <Radar className="w-4 h-4" />
                  谁在用我的网
                </span>
                {(scan?.new_count ?? 0) > 0 && (
                  <span
                    className={`text-xs px-1.5 py-0.5 rounded-full ${
                      activeTab === "devices"
                        ? "bg-white/20 text-white"
                        : "bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300"
                    }`}
                  >
                    {scan?.new_count} 新
                  </span>
                )}
              </button>

              <button
                onClick={() => setActiveTab("share")}
                className={`w-full text-left px-3 py-2.5 rounded-xl text-sm font-medium flex items-center gap-2 transition cursor-pointer ${
                  activeTab === "share"
                    ? "bg-blue-600 text-white"
                    : "hover:bg-slate-100 dark:hover:bg-zinc-800"
                }`}
              >
                <KeyRound className="w-4 h-4" />
                扫码连网与改密码
              </button>

              <button
                onClick={() => setActiveTab("health")}
                className={`w-full text-left px-3 py-2.5 rounded-xl text-sm font-medium flex items-center justify-between transition cursor-pointer ${
                  activeTab === "health"
                    ? "bg-blue-600 text-white"
                    : "hover:bg-slate-100 dark:hover:bg-zinc-800"
                }`}
              >
                <span className="flex items-center gap-2">
                  <Activity className="w-4 h-4" />
                  一键网络体检
                </span>
                {health && (
                  <span
                    className={`w-2 h-2 rounded-full ${
                      health.overall_status === "good"
                        ? "bg-emerald-400"
                        : health.overall_status === "warning"
                          ? "bg-amber-400"
                          : "bg-rose-400"
                    }`}
                  />
                )}
              </button>

              <button
                onClick={() => setActiveTab("router")}
                className={`w-full text-left px-3 py-2.5 rounded-xl text-sm font-medium flex items-center gap-2 transition cursor-pointer ${
                  activeTab === "router"
                    ? "bg-blue-600 text-white"
                    : "hover:bg-slate-100 dark:hover:bg-zinc-800"
                }`}
              >
                <Settings className="w-4 h-4" />
                路由器与说明
              </button>
            </nav>

            <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4 space-y-2">
              <div className="text-xs font-semibold">小白使用小贴士</div>
              <ul className="text-xs text-slate-500 dark:text-zinc-400 space-y-1.5 leading-relaxed">
                <li>• 手机连 Wi-Fi 常会开启“随机 MAC”，不同时段可能显示成新设备。</li>
                <li>• 建议把家人的手机和电脑点击“改备注”，剩下的陌生设备就一目了然。</li>
                <li>• 所有备注仅保存在你自己的电脑里。</li>
              </ul>
            </div>
          </aside>

          {/* 右侧主区域 */}
          <main className="lg:col-span-9 space-y-4">
            {/* TAB 1: 设备雷达 */}
            {activeTab === "devices" && (
              <section className="space-y-4">
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4">
                    <div className="text-xs text-slate-500 dark:text-zinc-400">当前在线终端</div>
                    <div className="text-2xl font-bold mt-1 tabular-nums">
                      {scan ? `${scan.total_clients} 台` : "--"}
                    </div>
                    <div className="text-xs text-slate-500 dark:text-zinc-400 mt-1">
                      手机/电脑 {personalCount} 台 · 智能/固定设备 {iotCount} 台
                    </div>
                  </div>

                  <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4">
                    <div className="text-xs text-slate-500 dark:text-zinc-400">待确认的新设备</div>
                    <div className="text-2xl font-bold mt-1 tabular-nums text-amber-600 dark:text-amber-400">
                      {scan ? `${scan.new_count} 台` : "--"}
                    </div>
                    <div className="text-xs text-slate-500 dark:text-zinc-400 mt-1">
                      未备注过的陌生设备，点击可标为熟人
                    </div>
                  </div>

                  <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4">
                    <div className="text-xs text-slate-500 dark:text-zinc-400">当前无线信号</div>
                    <div className="text-lg font-bold mt-1">
                      {signalText(overview?.signal_dbm ?? null)}
                    </div>
                    <div className="text-xs text-slate-500 dark:text-zinc-400 mt-1">
                      {overview?.channel ?? "标准无线网络"}
                      {overview?.tx_rate_mbps ? ` · ${overview.tx_rate_mbps} Mbps` : ""}
                    </div>
                  </div>
                </div>

                <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-4 space-y-3">
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      {(
                        [
                          ["all", "全部"],
                          ["new", "陌生新设备"],
                          ["personal", "手机 / 电脑"],
                          ["iot", "固定 / 家居"],
                          ["blocked", "已屏蔽显示"],
                        ] as [DeviceFilter, string][]
                      ).map(([id, label]) => (
                        <button
                          key={id}
                          onClick={() => setFilter(id)}
                          className={`text-xs px-3 py-1.5 rounded-lg border transition cursor-pointer ${
                            filter === id
                              ? "bg-blue-600 text-white border-blue-600 font-medium"
                              : "border-slate-200 dark:border-zinc-700 text-slate-600 dark:text-zinc-300 hover:bg-slate-100 dark:hover:bg-zinc-800"
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
                          placeholder="搜索名称/品牌/IP..."
                          className="text-xs pl-8 pr-3 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500 w-44"
                        />
                      </div>

                      <label className="text-xs text-slate-500 dark:text-zinc-400 flex items-center gap-1.5 cursor-pointer select-none">
                        <input
                          type="checkbox"
                          checked={showTech}
                          onChange={(e) => setShowTech(e.target.checked)}
                          className="accent-blue-600"
                        />
                        显示 IP / MAC
                      </label>
                    </div>
                  </div>

                  <div className="divide-y divide-slate-100 dark:divide-zinc-800">
                    {scanning && !scan ? (
                      <div className="py-10 text-center text-xs text-slate-500">
                        正在扫描局域网中的所有在线设备，通常需要 3～6 秒...
                      </div>
                    ) : filteredDevices.length === 0 ? (
                      <div className="py-10 text-center text-xs text-slate-500">
                        当前筛选条件下没有设备
                      </div>
                    ) : (
                      filteredDevices.map((d) => (
                        <div
                          key={`${d.ip}-${d.mac}`}
                          className="py-3 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
                        >
                          <div className="flex items-start gap-3">
                            <div
                              className={`w-9 h-9 rounded-xl flex items-center justify-center shrink-0 ${
                                d.is_self
                                  ? "bg-blue-50 text-blue-600 dark:bg-blue-950 dark:text-blue-300"
                                  : d.is_new
                                    ? "bg-amber-50 text-amber-600 dark:bg-amber-950 dark:text-amber-300"
                                    : "bg-slate-100 text-slate-600 dark:bg-zinc-800 dark:text-zinc-300"
                              }`}
                            >
                              <DeviceIcon type={d.device_type} />
                            </div>

                            <div>
                              <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-sm font-medium">{d.display_name}</span>
                                {d.is_self && (
                                  <span className="text-xs px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300">
                                    本机
                                  </span>
                                )}
                                {d.is_gateway && (
                                  <span className="text-xs px-2 py-0.5 rounded-full bg-slate-100 text-slate-700 dark:bg-zinc-800 dark:text-zinc-300">
                                    路由器
                                  </span>
                                )}
                                {d.is_new && !d.blocked && (
                                  <span className="text-xs px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300">
                                    陌生新设备
                                  </span>
                                )}
                                {d.trusted && !d.is_self && !d.is_gateway && !d.blocked && (
                                  <span className="text-xs px-2 py-0.5 rounded-full bg-emerald-50 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
                                    熟人
                                  </span>
                                )}
                              </div>

                              <div className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
                                {d.vendor}
                                {d.latency_ms ? ` · 响应 ${d.latency_ms} ms` : d.ping_alive ? " · 在线" : " · 静默在线（禁 Ping）"}
                              </div>

                              {showTech && (
                                <div className="text-xs font-mono text-slate-400 dark:text-zinc-500 mt-1">
                                  IP: {d.ip} · MAC: {d.mac}
                                  {d.hostname ? ` · ${d.hostname}` : ""}
                                </div>
                              )}
                            </div>
                          </div>

                          <div className="flex items-center gap-2 flex-wrap shrink-0">
                            {!d.blocked ? (
                              <>
                                {d.is_new && (
                                  <button
                                    onClick={() => handleTrustDevice(d)}
                                    className="text-xs px-2.5 py-1.5 rounded-lg bg-emerald-50 text-emerald-700 hover:bg-emerald-100 dark:bg-emerald-950 dark:text-emerald-300 cursor-pointer"
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
                                    className="inline-flex items-center gap-1 text-xs px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 cursor-pointer"
                                  >
                                    <Pencil className="w-3 h-3" />
                                    改备注
                                  </button>
                                )}
                                {!d.is_self && !d.is_gateway && (
                                  <button
                                    onClick={() => handleToggleHidden(d, true)}
                                    className="text-xs px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 text-rose-600 dark:text-rose-400 hover:bg-rose-50 dark:hover:bg-rose-950/40 cursor-pointer"
                                  >
                                    隐藏设备
                                  </button>
                                )}
                              </>
                            ) : (
                              <button
                                onClick={() => handleToggleHidden(d, false)}
                                className="text-xs px-2.5 py-1.5 rounded-lg bg-slate-100 dark:bg-zinc-800 cursor-pointer"
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
              </section>
            )}

            {/* TAB 2: 扫码连网与改密码 */}
            {activeTab === "share" && (
              <section className="grid grid-cols-1 md:grid-cols-12 gap-4">
                <div className="md:col-span-7 bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 space-y-4">
                  <div>
                    <h2 className="text-sm font-bold">查看并分享当前 Wi-Fi 密码</h2>
                    <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
                      无需登录路由器，直接从本机系统安全存储读取已连 Wi-Fi 的密码并生成二维码。
                    </p>
                  </div>

                  {overview?.ssid_redacted && (
                    <div className="text-xs p-3 rounded-xl bg-amber-50 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300 border border-amber-200 dark:border-amber-900">
                      macOS 未授予“定位权限”时会隐藏当前 Wi-Fi 名称，已为你列出本机最近连过的网络，选择对应名称即可读取密码：
                    </div>
                  )}

                  <div className="space-y-3">
                    <div>
                      <label className="block text-xs text-slate-500 dark:text-zinc-400 mb-1">
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
                      <label className="block text-xs text-slate-500 dark:text-zinc-400 mb-1">
                        Wi-Fi 密码
                      </label>
                      <div className="flex gap-2">
                        <input
                          type={showPassword ? "text" : "password"}
                          value={wifiPassword}
                          onChange={(e) => setWifiPassword(e.target.value)}
                          placeholder="点击右侧按钮从系统读取，或手动输入"
                          className="flex-1 text-sm px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                        <button
                          type="button"
                          onClick={() => setShowPassword((v) => !v)}
                          className="px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 cursor-pointer"
                          title={showPassword ? "隐藏" : "显示"}
                        >
                          {showPassword ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                        </button>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 pt-1 flex-wrap">
                      <button
                        onClick={handleReadSavedPassword}
                        disabled={readingPw}
                        className="text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 cursor-pointer"
                      >
                        {readingPw ? "正在读取..." : "🔐 读取本机已保存的密码"}
                      </button>
                      <button
                        onClick={handleCopyWifiCard}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-slate-100 dark:bg-zinc-800 hover:bg-slate-200 dark:hover:bg-zinc-700 cursor-pointer"
                      >
                        <Copy className="w-3.5 h-3.5" />
                        复制发微信
                      </button>
                    </div>
                  </div>

                  {/* 修改路由器密码引导卡片 */}
                  <div className="border-t border-slate-100 dark:border-zinc-800 pt-4 space-y-2.5">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-bold">想修改家里 Wi-Fi 密码？</h3>
                      <span className="text-xs text-slate-500">
                        已识别：{overview?.router.brand ?? "路由器"}
                      </span>
                    </div>
                    <p className="text-xs text-slate-500 dark:text-zinc-400">
                      V0.1 通用版支持一键打开你家路由器后台，按下面 3 步修改即可（V0.2 将支持中兴/TP-Link/小米一键直改）：
                    </p>
                    <ol className="text-xs text-slate-600 dark:text-zinc-300 space-y-1 list-decimal list-inside">
                      {(overview?.router.guide_steps ?? []).map((step, i) => (
                        <li key={i}>{step}</li>
                      ))}
                    </ol>
                    <button
                      onClick={handleOpenRouterAdmin}
                      className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 cursor-pointer"
                    >
                      <ExternalLink className="w-3.5 h-3.5" />
                      打开路由器管理页 ({overview?.router.admin_url})
                    </button>
                  </div>
                </div>

                {/* 右侧扫码卡片 */}
                <div className="md:col-span-5 bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 flex flex-col items-center justify-center text-center space-y-3">
                  <div className="text-sm font-bold">亲友手机扫一扫直接连</div>
                  <div className="p-2 rounded-2xl bg-white border border-slate-200">
                    {selectedSsid.trim() ? (
                      <canvas ref={qrCanvasRef} className="block" />
                    ) : (
                      <div className="w-44 h-44 flex items-center justify-center text-xs text-slate-400">
                        选择 Wi-Fi 名称后生成二维码
                      </div>
                    )}
                  </div>
                  <div className="text-xs text-slate-500 dark:text-zinc-400">
                    网络：<span className="font-semibold text-slate-800 dark:text-zinc-200">{selectedSsid || "未选择"}</span>
                  </div>
                  <p className="text-xs text-slate-400 dark:text-zinc-500 leading-relaxed">
                    支持 iPhone 原生相机、安卓系统相机与微信扫一扫识别连接。
                  </p>
                </div>
              </section>
            )}

            {/* TAB 3: 一键网络体检 */}
            {activeTab === "health" && (
              <section className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 space-y-4">
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                  <div>
                    <h2 className="text-sm font-bold">网速慢、老断线？一键查出哪里卡了</h2>
                    <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
                      真实测试你本机到路由器、再到外网宽带的延迟与连通性，用大白话给出排查建议。
                    </p>
                  </div>
                  <button
                    onClick={handleRunHealth}
                    disabled={checkingHealth}
                    className="text-xs font-medium px-4 py-2.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 cursor-pointer shrink-0"
                  >
                    {checkingHealth ? "正在体检中..." : "🩺 开始一键体检"}
                  </button>
                </div>

                {!health ? (
                  <div className="py-10 text-center text-xs text-slate-500 border border-dashed border-slate-200 dark:border-zinc-800 rounded-xl">
                    点击右上角「开始一键体检」，大约 2 秒即可得出诊断结果。
                  </div>
                ) : (
                  <div className="space-y-4">
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                      {health.steps.map((s) => (
                        <div
                          key={s.id}
                          className="p-4 rounded-xl bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 space-y-1.5"
                        >
                          <div className="flex items-center justify-between text-xs">
                            <span className="font-semibold">{s.title}</span>
                            <span
                              className={`font-medium ${
                                s.status === "good"
                                  ? "text-emerald-600 dark:text-emerald-400"
                                  : s.status === "warning"
                                    ? "text-amber-600 dark:text-amber-400"
                                    : "text-rose-600 dark:text-rose-400"
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

                    <div className="p-4 rounded-xl border border-slate-200 dark:border-zinc-800 bg-slate-50 dark:bg-zinc-950 flex items-start gap-3">
                      {health.overall_status === "good" ? (
                        <CheckCircle2 className="w-5 h-5 text-emerald-500 shrink-0 mt-0.5" />
                      ) : (
                        <AlertTriangle className="w-5 h-5 text-amber-500 shrink-0 mt-0.5" />
                      )}
                      <div className="text-xs space-y-1">
                        <div className="font-bold text-sm">{health.summary_title}</div>
                        <p className="text-slate-600 dark:text-zinc-300 leading-relaxed">
                          {health.summary_advice}
                        </p>
                      </div>
                    </div>
                  </div>
                )}
              </section>
            )}

            {/* TAB 4: 路由器与说明 */}
            {activeTab === "router" && (
              <section className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 space-y-3">
                  <div className="flex items-center justify-between">
                    <h2 className="text-sm font-bold">已识别的路由器指纹</h2>
                    <ShieldCheck className="w-4 h-4 text-emerald-500" />
                  </div>
                  <div className="text-xs space-y-2 text-slate-600 dark:text-zinc-300">
                    <div className="flex justify-between py-1.5 border-b border-slate-100 dark:border-zinc-800">
                      <span className="text-slate-500">品牌识别</span>
                      <span className="font-medium">{overview?.router.brand ?? "--"}</span>
                    </div>
                    <div className="flex justify-between py-1.5 border-b border-slate-100 dark:border-zinc-800">
                      <span className="text-slate-500">管理地址</span>
                      <span className="font-mono">{overview?.router.admin_url ?? "--"}</span>
                    </div>
                    <div className="flex justify-between py-1.5 border-b border-slate-100 dark:border-zinc-800">
                      <span className="text-slate-500">路由器 MAC</span>
                      <span className="font-mono">{overview?.router.gateway_mac ?? "--"}</span>
                    </div>
                    <div className="flex justify-between py-1.5">
                      <span className="text-slate-500">后台页面标题</span>
                      <span>{overview?.router.page_title ?? "默认管理页"}</span>
                    </div>
                  </div>
                  <button
                    onClick={handleOpenRouterAdmin}
                    className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 cursor-pointer"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    在浏览器中打开路由器后台
                  </button>
                </div>

                <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 space-y-3">
                  <h2 className="text-sm font-bold">关于当前版本 (V0.1 MVP)</h2>
                  <p className="text-xs text-slate-500 dark:text-zinc-400 leading-relaxed">
                    当前版本已实现完全通用的局域网发现、本地熟人备注持久化、系统 Wi-Fi 密码读取扫码以及三步网络体检，支持 macOS 与 Windows 双端编译。
                  </p>
                  <div className="text-xs bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 rounded-xl p-3 space-y-1 text-slate-600 dark:text-zinc-300">
                    <div className="font-semibold">下一阶段规划（V0.2）：</div>
                    <div>• 接入中兴（ZTE）/ OpenWrt / 小米 / TP-Link 路由器适配器</div>
                    <div>• 软件内直接修改 Wi-Fi 密码与一键踢出陌生设备</div>
                    <div>• 后台定时静默扫描 + 新设备接入系统通知</div>
                  </div>
                </div>
              </section>
            )}
          </main>
        </div>
      </div>

      {/* 重命名弹窗 */}
      {renamingDevice && (
        <div className="fixed inset-0 bg-black/40 flex items-center justify-center p-4 z-50">
          <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-2xl p-5 w-full max-w-sm space-y-3 shadow-xl">
            <div className="text-sm font-bold">给设备起个好记的名字</div>
            <div className="text-xs text-slate-500">
              例如：“我的电脑”、“客厅电视”、“妈妈的手机”
            </div>
            <input
              autoFocus
              value={renameInput}
              onChange={(e) => setRenameInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleSaveAlias()}
              className="w-full text-sm px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
            />
            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={() => setRenamingDevice(null)}
                className="text-xs px-3 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 cursor-pointer"
              >
                取消
              </button>
              <button
                onClick={handleSaveAlias}
                className="text-xs px-3 py-1.5 rounded-lg bg-blue-600 text-white cursor-pointer"
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
