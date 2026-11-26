import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowUpRight,
  Bell,
  Clock,
  KeyRound,
  Moon,
  PlugZap,
  Plus,
  Power,
  RefreshCw,
  Router,
  ShieldCheck,
  Trash2,
  Unplug,
  Wifi,
} from "lucide-react";
import type {
  AppPreferences,
  AppSettings,
  DeviceView,
  NetworkOverview,
  ParentalRule,
  RouterBindResult,
  RouterClientInfo,
} from "../types";

interface RouterGuardianTabProps {
  overview: NetworkOverview | null;
  preferences: AppPreferences | null;
  devices: DeviceView[];
  onOpenRouterAdmin: () => void;
  onUpdateSettings: (next: AppSettings) => Promise<void>;
  onClearEvents: () => Promise<void>;
  onRefreshPreferences: () => Promise<void>;
  notify: (msg: string) => void;
}

function formatEventTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function RouterGuardianTab({
  overview,
  preferences,
  devices,
  onOpenRouterAdmin,
  onUpdateSettings,
  onClearEvents,
  onRefreshPreferences,
  notify,
}: RouterGuardianTabProps) {
  const router = overview?.router;
  const settings = preferences?.settings ?? {
    auto_scan_enabled: true,
    scan_interval_secs: 180,
    notify_new_device: true,
    minimize_to_tray: true,
  };

  const [username, setUsername] = useState(
    preferences?.router_binding?.username ?? router?.capabilities?.default_username ?? "admin"
  );
  const [password, setPassword] = useState("");
  const [saveCreds, setSaveCreds] = useState(true);
  const [binding, setBinding] = useState(false);

  const [routerClients, setRouterClients] = useState<RouterClientInfo[] | null>(null);
  const [loadingClients, setLoadingClients] = useState(false);
  const [rebooting, setRebooting] = useState(false);
  const [confirmReboot, setConfirmReboot] = useState(false);

  // 夜间/孩子断网守护规则
  const selectableDevices = devices.filter((d) => !d.is_self && !d.is_gateway);
  const [ruleMac, setRuleMac] = useState("");
  const [ruleStart, setRuleStart] = useState("23:00");
  const [ruleEnd, setRuleEnd] = useState("07:00");
  const [savingRule, setSavingRule] = useState(false);

  useEffect(() => {
    if (!ruleMac && selectableDevices.length > 0) {
      setRuleMac(selectableDevices[0].mac);
    }
  }, [ruleMac, selectableDevices]);

  useEffect(() => {
    if (preferences?.router_binding?.username) {
      setUsername(preferences.router_binding.username);
    } else if (router?.capabilities?.default_username) {
      setUsername(router.capabilities.default_username);
    }
  }, [preferences?.router_binding?.username, router?.capabilities?.default_username]);

  const isBound = Boolean(
    preferences?.router_connected || preferences?.has_saved_router_password
  );

  async function handleBindRouter(e: React.FormEvent) {
    e.preventDefault();
    if (!router?.adapter_id) return;
    if (!password) {
      notify("请输入路由器管理员密码");
      return;
    }
    setBinding(true);
    try {
      const res = await invoke<RouterBindResult>("bind_router_adapter", {
        gatewayIp: router.gateway_ip,
        adapterId: router.adapter_id,
        username: username.trim() || null,
        password,
        saveCredentials: saveCreds,
      });
      setPassword("");
      await onRefreshPreferences();
      notify(res.message);
    } catch (err) {
      notify(`连接失败：${String(err)}`);
    } finally {
      setBinding(false);
    }
  }

  async function handleForgetRouter() {
    if (!router) return;
    try {
      await invoke("forget_router_credentials", { gatewayIp: router.gateway_ip });
      setRouterClients(null);
      await onRefreshPreferences();
      notify("已清除本地保存的路由器密码");
    } catch (err) {
      notify(`清除失败：${String(err)}`);
    }
  }

  async function handleLoadRouterClients() {
    setLoadingClients(true);
    try {
      const list = await invoke<RouterClientInfo[]>("get_router_clients");
      setRouterClients(list);
      notify(`已从路由器读取 ${list.length} 条终端记录`);
    } catch (err) {
      notify(String(err));
    } finally {
      setLoadingClients(false);
    }
  }

  async function handleRebootRouter() {
    setRebooting(true);
    try {
      const msg = await invoke<string>("reboot_router");
      setConfirmReboot(false);
      await onRefreshPreferences();
      notify(msg);
    } catch (err) {
      notify(String(err));
    } finally {
      setRebooting(false);
    }
  }

  async function handleToggleAutostart(enabled: boolean) {
    try {
      await invoke<boolean>("set_autostart", { enabled });
      await onRefreshPreferences();
      notify(enabled ? "已开启开机自动启动" : "已关闭开机自动启动");
    } catch (err) {
      notify(`设置开机自启失败：${String(err)}`);
    }
  }

  async function handleAddParentalRule() {
    const dev = selectableDevices.find((d) => d.mac === ruleMac);
    if (!dev) {
      notify("请先在下拉框选择需要守护的设备");
      return;
    }
    setSavingRule(true);
    try {
      const rule: ParentalRule = {
        id: `${dev.mac}-${Date.now()}`,
        mac: dev.mac,
        device_name: dev.display_name,
        start_hhmm: ruleStart,
        end_hhmm: ruleEnd,
        enabled: true,
      };
      await invoke("save_parental_rule", { rule });
      await onRefreshPreferences();
      notify(`已为「${dev.display_name}」添加 ${ruleStart} ~ ${ruleEnd} 守护规则`);
    } catch (err) {
      notify(`保存规则失败：${String(err)}`);
    } finally {
      setSavingRule(false);
    }
  }

  async function handleToggleParentalRule(rule: ParentalRule) {
    try {
      await invoke("save_parental_rule", {
        rule: { ...rule, enabled: !rule.enabled },
      });
      await onRefreshPreferences();
    } catch (err) {
      notify(`更新失败：${String(err)}`);
    }
  }

  async function handleDeleteParentalRule(id: string) {
    try {
      await invoke("delete_parental_rule", { id });
      await onRefreshPreferences();
      notify("已删除守护规则");
    } catch (err) {
      notify(`删除失败：${String(err)}`);
    }
  }

  return (
    <div className="space-y-5">
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-5">
        {/* 左列：路由器识别与适配器控制 */}
        <div className="lg:col-span-7 space-y-5">
          <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-4 shadow-2xs">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Router className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                <h2 className="text-sm font-bold">当前路由器识别与控制</h2>
              </div>
              <span
                className={`text-[11px] font-medium px-2.5 py-0.5 rounded-full ${
                  router?.adapter_supported
                    ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300"
                    : "bg-slate-100 text-slate-600 dark:bg-zinc-800 dark:text-zinc-300"
                }`}
              >
                {router?.adapter_supported ? "已支持专属适配" : "通用引导模式"}
              </span>
            </div>

            <div className="text-xs divide-y divide-slate-100 dark:divide-zinc-800">
              <div className="py-2 flex justify-between">
                <span className="text-slate-500 dark:text-zinc-400">识别品牌</span>
                <span className="font-semibold">{router?.brand ?? "--"}</span>
              </div>
              <div className="py-2 flex justify-between">
                <span className="text-slate-500 dark:text-zinc-400">管理后台地址</span>
                <span className="font-mono">{router?.admin_url ?? "--"}</span>
              </div>
              <div className="py-2 flex justify-between">
                <span className="text-slate-500 dark:text-zinc-400">网关 MAC</span>
                <span className="font-mono">{router?.gateway_mac ?? "--"}</span>
              </div>
              <div className="py-2 flex justify-between">
                <span className="text-slate-500 dark:text-zinc-400">页面标题</span>
                <span className="truncate max-w-52">
                  {router?.page_title ?? "默认管理页"}
                </span>
              </div>
            </div>

            {router?.adapter_supported && router.capabilities ? (
              <div className="space-y-3 pt-1">
                <div className="text-xs rounded-xl bg-blue-50/70 dark:bg-blue-500/10 border border-blue-200/60 dark:border-blue-500/20 text-slate-700 dark:text-zinc-200 p-3 leading-relaxed">
                  {router.capabilities.note}
                </div>

                {!isBound ? (
                  <form onSubmit={handleBindRouter} className="space-y-3 pt-1">
                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
                      <div>
                        <label className="block text-[11px] font-medium text-slate-500 mb-1">
                          管理员账号
                        </label>
                        <input
                          value={username}
                          onChange={(e) => setUsername(e.target.value)}
                          placeholder="如 admin 或 root"
                          className="w-full text-xs px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                      </div>
                      <div>
                        <label className="block text-[11px] font-medium text-slate-500 mb-1">
                          管理员密码（见路由器背面贴纸）
                        </label>
                        <input
                          type="password"
                          value={password}
                          onChange={(e) => setPassword(e.target.value)}
                          placeholder="输入路由器管理密码"
                          className="w-full text-xs px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                      </div>
                    </div>

                    <label className="flex items-center gap-2 text-xs text-slate-600 dark:text-zinc-300 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={saveCreds}
                        onChange={(e) => setSaveCreds(e.target.checked)}
                        className="rounded border-slate-300"
                      />
                      <span>记住密码到本机系统安全钥匙串（不上传云端）</span>
                    </label>

                    <div className="flex items-center gap-2 flex-wrap">
                      <button
                        type="submit"
                        disabled={binding}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 transition cursor-pointer"
                      >
                        <PlugZap className="w-3.5 h-3.5" />
                        {binding ? "正在验证连接..." : "连接并绑定路由器"}
                      </button>
                      <button
                        type="button"
                        onClick={onOpenRouterAdmin}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                      >
                        <span>网页后台</span>
                        <ArrowUpRight className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </form>
                ) : (
                  <div className="space-y-3 pt-1">
                    <div className="flex items-center justify-between rounded-xl bg-emerald-50/70 dark:bg-emerald-500/10 border border-emerald-200/70 dark:border-emerald-500/20 px-3.5 py-2.5">
                      <div className="flex items-center gap-2 text-xs text-emerald-800 dark:text-emerald-300 font-medium">
                        <ShieldCheck className="w-4 h-4 shrink-0" />
                        <span>
                          已绑定 {router.brand}（账号：{preferences?.router_binding?.username ?? username}）
                        </span>
                      </div>
                      <button
                        onClick={handleForgetRouter}
                        className="inline-flex items-center gap-1 text-[11px] text-slate-500 hover:text-rose-600 cursor-pointer"
                      >
                        <Unplug className="w-3 h-3" />
                        解绑
                      </button>
                    </div>

                    <div className="flex items-center gap-2 flex-wrap">
                      {router.capabilities.supports_clients && (
                        <button
                          onClick={handleLoadRouterClients}
                          disabled={loadingClients}
                          className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 transition cursor-pointer"
                        >
                          <RefreshCw className={`w-3.5 h-3.5 ${loadingClients ? "animate-spin" : ""}`} />
                          {loadingClients ? "读取中..." : "读取路由器无线终端"}
                        </button>
                      )}

                      {router.capabilities.supports_reboot && !confirmReboot && (
                        <button
                          onClick={() => setConfirmReboot(true)}
                          className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-rose-50 hover:text-rose-600 hover:border-rose-200 dark:hover:bg-rose-500/10 transition cursor-pointer"
                        >
                          <Power className="w-3.5 h-3.5" />
                          重启路由器
                        </button>
                      )}

                      {confirmReboot && (
                        <div className="inline-flex items-center gap-1.5 bg-rose-50 dark:bg-rose-500/10 border border-rose-200 dark:border-rose-500/25 px-2.5 py-1.5 rounded-xl">
                          <span className="text-[11px] text-rose-700 dark:text-rose-300">
                            确认重启？会断网约 1 分钟
                          </span>
                          <button
                            onClick={handleRebootRouter}
                            disabled={rebooting}
                            className="text-[11px] font-semibold px-2 py-0.5 rounded-lg bg-rose-600 text-white hover:bg-rose-700 cursor-pointer"
                          >
                            {rebooting ? "发送中..." : "确认"}
                          </button>
                          <button
                            onClick={() => setConfirmReboot(false)}
                            className="text-[11px] px-1.5 py-0.5 text-slate-500 hover:text-slate-800 cursor-pointer"
                          >
                            取消
                          </button>
                        </div>
                      )}

                      <button
                        onClick={onOpenRouterAdmin}
                        className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                      >
                        <span>打开网页后台</span>
                        <ArrowUpRight className="w-3.5 h-3.5" />
                      </button>
                    </div>

                    {routerClients && (
                      <div className="rounded-xl border border-slate-200/80 dark:border-zinc-800 divide-y divide-slate-100 dark:divide-zinc-800 overflow-hidden">
                        <div className="px-3 py-2 bg-slate-50 dark:bg-zinc-950 text-[11px] font-semibold text-slate-500 flex justify-between">
                          <span>路由器直读终端列表</span>
                          <span>{routerClients.length} 台</span>
                        </div>
                        {routerClients.length === 0 ? (
                          <div className="p-3 text-xs text-slate-400 text-center">
                            暂无返回记录
                          </div>
                        ) : (
                          routerClients.slice(0, 12).map((c) => (
                            <div
                              key={c.mac}
                              className="px-3 py-2 text-xs flex items-center justify-between gap-2"
                            >
                              <div className="min-w-0">
                                <div className="font-medium truncate">
                                  {c.name || c.ip || c.mac}
                                </div>
                                <div className="text-[11px] font-mono text-slate-400">
                                  {c.mac}
                                  {c.ip ? ` · ${c.ip}` : ""}
                                </div>
                              </div>
                              <div className="flex items-center gap-1.5 shrink-0">
                                {c.band && (
                                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300">
                                    {c.band}
                                  </span>
                                )}
                                {c.signal_dbm !== null && (
                                  <span className="text-[11px] font-mono text-slate-400">
                                    {c.signal_dbm} dBm
                                  </span>
                                )}
                              </div>
                            </div>
                          ))
                        )}
                      </div>
                    )}
                  </div>
                )}
              </div>
            ) : (
              <div className="space-y-3 pt-1">
                <div className="text-xs rounded-xl bg-slate-50 dark:bg-zinc-950 border border-slate-200/70 dark:border-zinc-800 p-3 text-slate-600 dark:text-zinc-300 leading-relaxed">
                  当前路由器暂未内置专属 API 适配器（或为运营商定制型号），你依然可以使用全部设备雷达、扫码分享密码和网络体检功能；如需改密可点击下方直接打开路由器网页后台。
                </div>
                <button
                  onClick={onOpenRouterAdmin}
                  className="inline-flex items-center gap-1.5 text-xs font-medium px-3.5 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 transition cursor-pointer"
                >
                  <span>在浏览器打开后台</span>
                  <ArrowUpRight className="w-3.5 h-3.5" />
                </button>
              </div>
            )}
          </div>

          {/* 孩子 / 夜间定时断网守护 */}
          <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3.5 shadow-2xs">
            <div className="flex items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Moon className="w-4 h-4 text-indigo-500" />
                <h2 className="text-sm font-bold">孩子 / 夜间定时断网守护</h2>
              </div>
              <span className="text-[11px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-zinc-800 text-slate-600 dark:text-zinc-300">
                {router?.capabilities?.supports_block_mac && isBound
                  ? "路由器自动执行"
                  : "到点弹窗提醒"}
              </span>
            </div>

            <p className="text-xs text-slate-500 dark:text-zinc-400">
              设置休息时段（例如 23:00 ~ 07:00）。已绑定支持黑名单的路由器（小米 / TP-Link / OpenWrt）会自动到点暂停与恢复联网。
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-12 gap-2 items-end">
              <div className="sm:col-span-5">
                <label className="block text-[11px] text-slate-500 mb-1">选择设备</label>
                <select
                  value={ruleMac}
                  onChange={(e) => setRuleMac(e.target.value)}
                  className="w-full text-xs px-2.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                >
                  {selectableDevices.length === 0 ? (
                    <option value="">暂无可选设备</option>
                  ) : (
                    selectableDevices.map((d) => (
                      <option key={d.mac} value={d.mac}>
                        {d.display_name}
                      </option>
                    ))
                  )}
                </select>
              </div>
              <div className="sm:col-span-2">
                <label className="block text-[11px] text-slate-500 mb-1">开始</label>
                <input
                  type="time"
                  value={ruleStart}
                  onChange={(e) => setRuleStart(e.target.value)}
                  className="w-full text-xs px-2 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                />
              </div>
              <div className="sm:col-span-2">
                <label className="block text-[11px] text-slate-500 mb-1">结束</label>
                <input
                  type="time"
                  value={ruleEnd}
                  onChange={(e) => setRuleEnd(e.target.value)}
                  className="w-full text-xs px-2 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                />
              </div>
              <div className="sm:col-span-3">
                <button
                  type="button"
                  onClick={handleAddParentalRule}
                  disabled={savingRule || !ruleMac}
                  className="w-full inline-flex items-center justify-center gap-1 text-xs font-medium px-3 py-2 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 cursor-pointer"
                >
                  <Plus className="w-3.5 h-3.5" />
                  添加规则
                </button>
              </div>
            </div>

            {(preferences?.parental_rules?.length ?? 0) > 0 && (
              <div className="divide-y divide-slate-100 dark:divide-zinc-800 rounded-xl border border-slate-200/70 dark:border-zinc-800 overflow-hidden">
                {preferences!.parental_rules.map((rule) => (
                  <div
                    key={rule.id}
                    className="px-3 py-2.5 flex items-center justify-between gap-2 text-xs bg-slate-50/50 dark:bg-zinc-950/50"
                  >
                    <div className="min-w-0">
                      <div className="font-semibold truncate">{rule.device_name}</div>
                      <div className="text-[11px] text-slate-500 font-mono">
                        {rule.start_hhmm} ~ {rule.end_hhmm}
                        {rule.currently_enforced ? " · 正在休息限制中" : ""}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        onChange={() => handleToggleParentalRule(rule)}
                        className="w-4 h-4 accent-blue-600 cursor-pointer"
                      />
                      <button
                        type="button"
                        onClick={() => handleDeleteParentalRule(rule.id)}
                        className="text-slate-400 hover:text-rose-500 cursor-pointer p-1"
                        title="删除规则"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* 版本路线 V0.3 */}
          <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3 shadow-2xs">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-bold">当前版本：V0.3 多品牌适配与家庭守护</h2>
              <span className="text-[11px] px-2 py-0.5 rounded-full bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300 font-medium">
                v0.3.0
              </span>
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs text-slate-600 dark:text-zinc-300">
              <div className="rounded-xl bg-slate-50 dark:bg-zinc-950 p-3 space-y-1">
                <div className="font-semibold text-slate-800 dark:text-zinc-100 flex items-center gap-1.5">
                  <Bell className="w-3.5 h-3.5 text-blue-500" />
                  五大家用路由器生态适配
                </div>
                <p className="text-[11px] text-slate-500 dark:text-zinc-400">
                  支持 OpenWrt、小米、TP-Link、华为、中兴自动识别，并支持访客 Wi-Fi 一键开关。
                </p>
              </div>
              <div className="rounded-xl bg-slate-50 dark:bg-zinc-950 p-3 space-y-1">
                <div className="font-semibold text-slate-800 dark:text-zinc-100 flex items-center gap-1.5">
                  <Wifi className="w-3.5 h-3.5 text-emerald-500" />
                  夜间守护、开机自启与诊断导出
                </div>
                <p className="text-[11px] text-slate-500 dark:text-zinc-400">
                  Mac/Win 开机自启常驻托盘，支持定时断网守护与一键复制排障报告。
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* 右列：后台守护设置 & 最近网络动态 */}
        <div className="lg:col-span-5 space-y-5">
          <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-4 shadow-2xs">
            <div className="flex items-center gap-2">
              <KeyRound className="w-4 h-4 text-indigo-500" />
              <h2 className="text-sm font-bold">后台守护与提醒设置</h2>
            </div>

            <div className="space-y-3 text-xs">
              <label className="flex items-center justify-between gap-3 cursor-pointer">
                <div>
                  <div className="font-medium">后台定时静默巡检</div>
                  <div className="text-[11px] text-slate-400">
                    每隔一段时间自动扫描新设备并刷新托盘状态
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={settings.auto_scan_enabled}
                  onChange={(e) =>
                    onUpdateSettings({ ...settings, auto_scan_enabled: e.target.checked })
                  }
                  className="w-4 h-4 accent-blue-600 cursor-pointer"
                />
              </label>

              {settings.auto_scan_enabled && (
                <div className="flex items-center justify-between gap-3 pl-2 border-l-2 border-blue-200 dark:border-blue-500/30">
                  <span className="text-slate-500 dark:text-zinc-400">巡检频率</span>
                  <select
                    value={settings.scan_interval_secs}
                    onChange={(e) =>
                      onUpdateSettings({
                        ...settings,
                        scan_interval_secs: Number(e.target.value),
                      })
                    }
                    className="text-xs px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950"
                  >
                    <option value={60}>每 1 分钟</option>
                    <option value={180}>每 3 分钟（推荐）</option>
                    <option value={300}>每 5 分钟</option>
                    <option value={600}>每 10 分钟</option>
                  </select>
                </div>
              )}

              <label className="flex items-center justify-between gap-3 cursor-pointer">
                <div>
                  <div className="font-medium">陌生设备上线弹窗提醒</div>
                  <div className="text-[11px] text-slate-400">
                    发现从未见过的新终端时发送系统桌面通知
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={settings.notify_new_device}
                  onChange={(e) =>
                    onUpdateSettings({ ...settings, notify_new_device: e.target.checked })
                  }
                  className="w-4 h-4 accent-blue-600 cursor-pointer"
                />
              </label>

              <label className="flex items-center justify-between gap-3 cursor-pointer">
                <div>
                  <div className="font-medium">关闭窗口时最小化到托盘</div>
                  <div className="text-[11px] text-slate-400">
                    点左上角/右上角关闭时不退出，点菜单栏图标随时打开
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={settings.minimize_to_tray}
                  onChange={(e) =>
                    onUpdateSettings({ ...settings, minimize_to_tray: e.target.checked })
                  }
                  className="w-4 h-4 accent-blue-600 cursor-pointer"
                />
              </label>

              <label className="flex items-center justify-between gap-3 cursor-pointer pt-1 border-t border-slate-100 dark:border-zinc-800">
                <div>
                  <div className="font-medium">开机自动启动并常驻托盘</div>
                  <div className="text-[11px] text-slate-400">
                    登录电脑后自动在后台守护家庭 Wi-Fi 安全
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={Boolean(preferences?.autostart_enabled)}
                  onChange={(e) => handleToggleAutostart(e.target.checked)}
                  className="w-4 h-4 accent-blue-600 cursor-pointer"
                />
              </label>
            </div>
          </div>

          {/* 最近网络动态时间线 */}
          <div className="bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 rounded-2xl p-5 space-y-3 shadow-2xs">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Clock className="w-4 h-4 text-amber-500" />
                <h2 className="text-sm font-bold">最近网络动态</h2>
              </div>
              {(preferences?.events.length ?? 0) > 0 && (
                <button
                  onClick={onClearEvents}
                  className="inline-flex items-center gap-1 text-[11px] text-slate-400 hover:text-rose-500 cursor-pointer"
                >
                  <Trash2 className="w-3 h-3" />
                  清空
                </button>
              )}
            </div>

            {!preferences?.events.length ? (
              <div className="py-8 text-center text-xs text-slate-400">
                暂无新设备上线或路由器操作动态
              </div>
            ) : (
              <div className="space-y-2.5 max-h-64 overflow-y-auto pr-1">
                {preferences.events.map((ev, i) => (
                  <div
                    key={`${ev.timestamp}-${i}`}
                    className="rounded-xl bg-slate-50 dark:bg-zinc-950 border border-slate-200/60 dark:border-zinc-800 px-3 py-2.5 space-y-0.5"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-xs font-semibold truncate">{ev.title}</span>
                      <span className="text-[10px] font-mono text-slate-400 shrink-0">
                        {formatEventTime(ev.timestamp)}
                      </span>
                    </div>
                    <div className="text-[11px] text-slate-500 dark:text-zinc-400 truncate">
                      {ev.detail}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
