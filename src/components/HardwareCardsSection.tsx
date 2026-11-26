import {
  ArrowUpRight,
  Camera,
  CheckCircle2,
  ChevronRight,
  Cpu,
  Printer,
  Router,
  SlidersHorizontal,
  Tv,
} from "lucide-react";
import type { CapabilityAction, DeviceView } from "../types";

interface HardwareCardsSectionProps {
  devices: DeviceView[];
  runningKey: string | null;
  onOpenDrawer: (device: DeviceView) => void;
  onRunAction: (device: DeviceView, action: CapabilityAction) => void;
}

export function HardwareKindIcon({
  kind,
  className = "w-4 h-4",
}: {
  kind: string;
  className?: string;
}) {
  switch (kind) {
    case "printer":
      return <Printer className={className} />;
    case "router":
      return <Router className={className} />;
    case "camera":
      return <Camera className={className} />;
    case "smarthome":
      return <Cpu className={className} />;
    default:
      return <Tv className={className} />;
  }
}

function getKindColorClasses(kind: string) {
  switch (kind) {
    case "printer":
      return "bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-400";
    case "router":
      return "bg-indigo-50 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-400";
    case "camera":
      return "bg-rose-50 text-rose-600 dark:bg-rose-500/15 dark:text-rose-400";
    case "smarthome":
      return "bg-emerald-50 text-emerald-600 dark:bg-emerald-500/15 dark:text-emerald-400";
    default:
      return "bg-slate-100 text-slate-600 dark:bg-zinc-800 dark:text-zinc-300";
  }
}

export function HardwareCardsSection({
  devices,
  runningKey,
  onOpenDrawer,
  onRunAction,
}: HardwareCardsSectionProps) {
  const hardwareDevices = devices.filter(
    (d) => !d.blocked && Boolean(d.hardware)
  );

  if (hardwareDevices.length === 0) {
    return null;
  }

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between gap-2 px-0.5">
        <div className="flex items-center gap-2">
          <SlidersHorizontal className="w-4 h-4 text-blue-600 dark:text-blue-400" />
          <h2 className="text-sm font-bold tracking-tight">我的家庭硬件</h2>
          <span className="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-blue-50 text-blue-700 dark:bg-blue-500/15 dark:text-blue-300">
            {hardwareDevices.length} 台可管理
          </span>
        </div>
        <span className="text-[11px] text-slate-400 dark:text-zinc-500 hidden sm:inline">
          路由器、打印机与智能设备已自动归类，点击卡片可管理
        </span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3.5">
        {hardwareDevices.map((d) => {
          const hw = d.hardware!;
          const primaryAction = hw.actions.find((a) => a.style === "primary") ?? hw.actions[0];
          const maintenanceAction = hw.actions.find(
            (a) => a.id === "open_printer_maintenance"
          );
          const actionBusy = primaryAction
            ? runningKey === `${d.mac}:${primaryAction.id}`
            : false;
          const maintenanceBusy = maintenanceAction
            ? runningKey === `${d.mac}:${maintenanceAction.id}`
            : false;

          return (
            <div
              key={`hw-${d.ip}-${d.mac}`}
              onClick={() => onOpenDrawer(d)}
              className="group bg-white dark:bg-zinc-900 border border-slate-200/80 dark:border-zinc-800 hover:border-blue-300 dark:hover:border-blue-500/40 rounded-2xl p-4 shadow-2xs transition cursor-pointer flex flex-col justify-between gap-3.5"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="flex items-start gap-3 min-w-0">
                  <div
                    className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 ${getKindColorClasses(
                      hw.kind
                    )}`}
                  >
                    <HardwareKindIcon kind={hw.kind} className="w-5 h-5" />
                  </div>

                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="text-[13px] font-bold truncate max-w-44">
                        {d.display_name}
                      </span>
                      <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-md bg-slate-100 dark:bg-zinc-800 text-slate-600 dark:text-zinc-300 shrink-0">
                        {hw.category_label}
                      </span>
                    </div>
                    <div className="text-[11px] text-slate-500 dark:text-zinc-400 mt-1 flex items-center gap-1.5 truncate">
                      <span
                        className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                          d.ping_alive ? "bg-emerald-500" : "bg-slate-300 dark:bg-zinc-600"
                        }`}
                      />
                      <span className="truncate">{hw.status_text}</span>
                    </div>
                  </div>
                </div>

                <ChevronRight className="w-4 h-4 text-slate-300 group-hover:text-blue-500 shrink-0 transition" />
              </div>

              {/* 能力摘要与快捷操作栏 */}
              <div
                className="pt-2.5 border-t border-slate-100 dark:border-zinc-800/80 flex items-center justify-between gap-2"
                onClick={(e) => e.stopPropagation()}
              >
                {primaryAction ? (
                  primaryAction.disabled && hw.system_installed ? (
                    <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-emerald-600 dark:text-emerald-400">
                      <CheckCircle2 className="w-3.5 h-3.5" />
                      <span>本机已就绪</span>
                    </span>
                  ) : (
                    <button
                      type="button"
                      disabled={primaryAction.disabled || actionBusy}
                      onClick={() => onRunAction(d, primaryAction)}
                      className="inline-flex items-center gap-1 text-xs font-medium px-3 py-1.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-60 transition cursor-pointer"
                    >
                      <span>{actionBusy ? "执行中..." : primaryAction.label}</span>
                      {primaryAction.url && <ArrowUpRight className="w-3.5 h-3.5" />}
                    </button>
                  )
                ) : (
                  <span className="text-[11px] text-slate-400">{hw.manufacturer ?? d.vendor}</span>
                )}

                <div className="flex items-center gap-1.5 shrink-0">
                  {maintenanceAction && (
                    <button
                      type="button"
                      disabled={maintenanceAction.disabled || maintenanceBusy}
                      onClick={() => onRunAction(d, maintenanceAction)}
                      className="text-xs font-medium px-2.5 py-1.5 rounded-xl bg-slate-100 dark:bg-zinc-800 text-slate-700 dark:text-zinc-200 hover:bg-blue-50 hover:text-blue-700 dark:hover:bg-blue-500/20 dark:hover:text-blue-300 disabled:opacity-60 transition cursor-pointer shrink-0"
                    >
                      {maintenanceBusy ? "打开中..." : "喷头清洗"}
                    </button>
                  )}

                  <button
                    type="button"
                    onClick={() => onOpenDrawer(d)}
                    className="text-xs font-medium px-2.5 py-1.5 rounded-xl text-slate-600 dark:text-zinc-300 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer shrink-0"
                  >
                    管理详情
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
