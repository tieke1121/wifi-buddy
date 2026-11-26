import { useState } from "react";
import {
  AlertCircle,
  ArrowUpRight,
  CheckCircle2,
  Copy,
  Pencil,
  RefreshCw,
  Wrench,
  X,
} from "lucide-react";
import type { CapabilityAction, DeviceView, HardwareActionResult } from "../types";
import { HardwareKindIcon } from "./HardwareCardsSection";

interface HardwareDrawerProps {
  device: DeviceView | null;
  runningKey: string | null;
  refreshing: boolean;
  lastResult: HardwareActionResult | null;
  onClose: () => void;
  onRunAction: (device: DeviceView, action: CapabilityAction) => void;
  onRefreshSingle: (device: DeviceView) => void;
  onRename: (device: DeviceView) => void;
  notify: (msg: string) => void;
}

export function HardwareDrawer({
  device,
  runningKey,
  refreshing,
  lastResult,
  onClose,
  onRunAction,
  onRefreshSingle,
  onRename,
  notify,
}: HardwareDrawerProps) {
  const [copiedIp, setCopiedIp] = useState(false);

  if (!device || !device.hardware) {
    return null;
  }

  const hw = device.hardware;

  function handleCopyIp() {
    if (!device) return;
    navigator.clipboard.writeText(device.ip).then(
      () => {
        setCopiedIp(true);
        setTimeout(() => setCopiedIp(false), 1800);
        notify(`已复制硬件 IP：${device.ip}`);
      },
      () => notify("复制 IP 失败")
    );
  }

  return (
    <div
      className="fixed inset-0 z-50 flex justify-end bg-black/35 backdrop-blur-xs"
      onClick={onClose}
    >
      <aside
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md h-full bg-white dark:bg-zinc-900 border-l border-slate-200 dark:border-zinc-800 shadow-2xl flex flex-col overflow-hidden"
      >
        {/* 头部 */}
        <div className="px-5 py-4 border-b border-slate-100 dark:border-zinc-800 flex items-start justify-between gap-3">
          <div className="flex items-start gap-3 min-w-0">
            <div className="w-11 h-11 rounded-2xl bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400 flex items-center justify-center shrink-0">
              <HardwareKindIcon kind={hw.kind} className="w-5 h-5" />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2 flex-wrap">
                <h2 className="text-base font-bold truncate">{device.display_name}</h2>
                <span className="text-[11px] font-medium px-2 py-0.5 rounded-full bg-slate-100 dark:bg-zinc-800 text-slate-600 dark:text-zinc-300">
                  {hw.category_label}
                </span>
              </div>
              <div className="text-xs text-slate-500 dark:text-zinc-400 mt-1 flex items-center gap-1.5">
                <span
                  className={`w-2 h-2 rounded-full ${
                    device.ping_alive ? "bg-emerald-500" : "bg-slate-300 dark:bg-zinc-600"
                  }`}
                />
                <span>{hw.status_text}</span>
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* 内容滚动区 */}
        <div className="flex-1 overflow-y-auto p-5 space-y-5">
          {/* 操作结果提示横幅 */}
          {lastResult && (
            <div
              className={`rounded-2xl p-3.5 border text-xs space-y-1.5 overflow-hidden ${
                lastResult.success
                  ? "bg-emerald-50/80 border-emerald-200 text-emerald-900 dark:bg-emerald-500/10 dark:border-emerald-500/25 dark:text-emerald-200"
                  : "bg-amber-50/80 border-amber-200 text-amber-900 dark:bg-amber-500/10 dark:border-amber-500/25 dark:text-amber-200"
              }`}
            >
              <div className="flex items-start gap-2 font-semibold leading-relaxed break-words [overflow-wrap:anywhere]">
                {lastResult.success ? (
                  <CheckCircle2 className="w-4 h-4 text-emerald-600 dark:text-emerald-400 shrink-0 mt-0.5" />
                ) : (
                  <AlertCircle className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                )}
                <span className="min-w-0 flex-1">{lastResult.message}</span>
              </div>
              {lastResult.detail && (
                <p className="pl-6 text-[11px] opacity-90 leading-relaxed break-words [overflow-wrap:anywhere]">
                  {lastResult.detail}
                </p>
              )}
            </div>
          )}

          {/* 硬件基础档案 */}
          <div className="rounded-2xl bg-slate-50 dark:bg-zinc-950/70 border border-slate-200/80 dark:border-zinc-800 p-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-slate-700 dark:text-zinc-200">
                硬件信息档案
              </span>
              {!device.is_gateway && (
                <button
                  type="button"
                  onClick={() => onRename(device)}
                  className="inline-flex items-center gap-1 text-[11px] font-medium text-blue-600 dark:text-blue-400 hover:underline cursor-pointer"
                >
                  <Pencil className="w-3 h-3" />
                  <span>修改备注</span>
                </button>
              )}
            </div>

            <div className="text-xs divide-y divide-slate-200/60 dark:divide-zinc-800">
              <div className="py-2 flex items-center justify-between gap-2">
                <span className="text-slate-500 dark:text-zinc-400">品牌厂商</span>
                <span className="font-medium">{hw.manufacturer ?? device.vendor}</span>
              </div>
              {hw.model && (
                <div className="py-2 flex items-center justify-between gap-2">
                  <span className="text-slate-500 dark:text-zinc-400">识别型号</span>
                  <span className="font-semibold">{hw.model}</span>
                </div>
              )}
              <div className="py-2 flex items-center justify-between gap-2">
                <span className="text-slate-500 dark:text-zinc-400">局域网 IP</span>
                <button
                  type="button"
                  onClick={handleCopyIp}
                  className="inline-flex items-center gap-1.5 font-mono text-slate-800 dark:text-zinc-200 hover:text-blue-600 cursor-pointer"
                >
                  <span>{device.ip}</span>
                  <Copy className="w-3 h-3 text-slate-400" />
                  {copiedIp && <span className="text-[10px] text-emerald-600">已复制</span>}
                </button>
              </div>
              <div className="py-2 flex items-center justify-between gap-2">
                <span className="text-slate-500 dark:text-zinc-400">MAC 地址</span>
                <span className="font-mono text-slate-600 dark:text-zinc-300">{device.mac}</span>
              </div>
              {hw.system_printer_name && (
                <div className="py-2 flex items-center justify-between gap-2">
                  <span className="text-slate-500 dark:text-zinc-400">系统打印队列</span>
                  <span className="font-medium text-emerald-600 dark:text-emerald-400">
                    {hw.system_printer_name}
                  </span>
                </div>
              )}
            </div>

            {hw.services.length > 0 && (
              <div className="pt-1 flex flex-wrap gap-1.5">
                {hw.services.map((svc) => (
                  <span
                    key={svc}
                    className="text-[11px] px-2 py-0.5 rounded-md bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-700 text-slate-600 dark:text-zinc-300"
                  >
                    {svc}
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* 实用工具操作区 */}
          <div className="space-y-3">
            <div className="flex items-center gap-1.5 text-xs font-bold text-slate-700 dark:text-zinc-200">
              <Wrench className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" />
              <span>可用操作与系统对接</span>
            </div>

            <div className="space-y-2.5">
              {hw.actions.map((action) => {
                const busy = runningKey === `${device.mac}:${action.id}`;
                const btnStyle =
                  action.style === "primary"
                    ? "bg-blue-600 text-white hover:bg-blue-700 border-transparent shadow-xs shadow-blue-600/20"
                    : action.style === "secondary"
                      ? "bg-white dark:bg-zinc-900 border-slate-200 dark:border-zinc-700 hover:bg-slate-50 dark:hover:bg-zinc-800 text-slate-800 dark:text-zinc-100"
                      : "bg-slate-50 dark:bg-zinc-950 border-slate-200/70 dark:border-zinc-800 hover:bg-slate-100 dark:hover:bg-zinc-800 text-slate-700 dark:text-zinc-200";

                return (
                  <div
                    key={action.id}
                    className="rounded-2xl border border-slate-200/80 dark:border-zinc-800 p-3.5 space-y-2 bg-white dark:bg-zinc-900"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-xs font-semibold">{action.label}</span>
                      {action.badge && (
                        <span className="text-[10px] font-medium px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300">
                          {action.badge}
                        </span>
                      )}
                    </div>
                    <p className="text-[11px] text-slate-500 dark:text-zinc-400 leading-relaxed">
                      {action.description}
                    </p>
                    <button
                      type="button"
                      disabled={action.disabled || busy}
                      onClick={() => onRunAction(device, action)}
                      className={`w-full inline-flex items-center justify-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border transition cursor-pointer disabled:opacity-55 ${btnStyle}`}
                    >
                      <span>{busy ? "正在处理..." : action.label}</span>
                      {action.url && <ArrowUpRight className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        {/* 底部操作条 */}
        <div className="px-5 py-3.5 border-t border-slate-100 dark:border-zinc-800 bg-slate-50/70 dark:bg-zinc-950/60 flex items-center justify-between gap-3">
          <button
            type="button"
            disabled={refreshing}
            onClick={() => onRefreshSingle(device)}
            className="inline-flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 hover:bg-slate-50 dark:hover:bg-zinc-800 disabled:opacity-60 transition cursor-pointer"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${refreshing ? "animate-spin" : ""}`} />
            <span>{refreshing ? "正在探测..." : "刷新实时状态"}</span>
          </button>

          <button
            type="button"
            onClick={onClose}
            className="text-xs font-medium px-3.5 py-2 rounded-xl text-slate-500 hover:bg-slate-200/70 dark:hover:bg-zinc-800 transition cursor-pointer"
          >
            关闭
          </button>
        </div>
      </aside>
    </div>
  );
}
