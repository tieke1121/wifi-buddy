import { useState } from "react";
import { Check, Laptop, Radar, ShieldCheck, Smartphone, Sparkles, Tv, Wifi } from "lucide-react";
import type { DeviceView, NetworkOverview } from "../types";

interface OnboardingModalProps {
  overview: NetworkOverview | null;
  devices: DeviceView[];
  onSaveAlias: (mac: string, alias: string) => Promise<void>;
  onFinish: () => Promise<void>;
}

const QUICK_NAMES = ["我的手机", "家人的手机", "客厅电视", "办公电脑"];

export function OnboardingModal({
  overview,
  devices,
  onSaveAlias,
  onFinish,
}: OnboardingModalProps) {
  const [step, setStep] = useState<1 | 2>(1);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [savedMacs, setSavedMacs] = useState<Record<string, boolean>>({});
  const [finishing, setFinishing] = useState(false);

  const candidates = devices
    .filter((d) => !d.is_gateway)
    .slice(0, 4);

  async function handleQuickSave(d: DeviceView, alias: string) {
    const trimmed = alias.trim();
    if (!trimmed) return;
    setDrafts((prev) => ({ ...prev, [d.mac]: trimmed }));
    await onSaveAlias(d.mac, trimmed);
    setSavedMacs((prev) => ({ ...prev, [d.mac]: true }));
  }

  async function handleComplete() {
    setFinishing(true);
    try {
      for (const d of candidates) {
        const val = drafts[d.mac]?.trim();
        if (val && !savedMacs[d.mac]) {
          await onSaveAlias(d.mac, val);
        }
      }
      await onFinish();
    } finally {
      setFinishing(false);
    }
  }

  return (
    <div className="fixed inset-0 bg-slate-950/55 backdrop-blur-sm flex items-center justify-center p-4 z-50">
      <div className="bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-3xl w-full max-w-lg overflow-hidden shadow-2xl">
        {/* Header */}
        <div className="px-6 pt-6 pb-4 bg-gradient-to-br from-blue-50 via-indigo-50/60 to-white dark:from-blue-950/30 dark:via-zinc-900 dark:to-zinc-900 border-b border-slate-100 dark:border-zinc-800">
          <div className="flex items-center justify-between">
            <div className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-blue-600 dark:text-blue-400 bg-blue-100/80 dark:bg-blue-500/15 px-2.5 py-1 rounded-full">
              <Sparkles className="w-3.5 h-3.5" />
              <span>30 秒快速上手 · {step}/2</span>
            </div>
            <button
              onClick={handleComplete}
              disabled={finishing}
              className="text-xs text-slate-400 hover:text-slate-600 dark:hover:text-zinc-200 cursor-pointer"
            >
              跳过引导
            </button>
          </div>

          <h2 className="text-lg font-bold mt-3 tracking-tight">
            {step === 1 ? "欢迎使用 Wi-Fi 小管家" : "给常用设备起个好记的名字"}
          </h2>
          <p className="text-xs text-slate-500 dark:text-zinc-400 mt-1 leading-relaxed">
            {step === 1
              ? "不用懂 IP 和 MAC，一眼看清谁在连你家的网、扫码分享 Wi-Fi 密码、卡顿时一键查出原因。"
              : "先认领 1~2 台家里熟悉的设备，以后有陌生设备连上来时，我们就会立刻标黄提醒你。"}
          </p>
        </div>

        {/* Body */}
        <div className="p-6 space-y-4">
          {step === 1 ? (
            <>
              <div className="rounded-2xl bg-slate-50 dark:bg-zinc-950 border border-slate-200/70 dark:border-zinc-800 p-4 space-y-2.5">
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-500 dark:text-zinc-400 flex items-center gap-1.5">
                    <Wifi className="w-4 h-4 text-blue-500" />
                    当前连接网络
                  </span>
                  <span className="font-semibold">
                    {overview?.ssid || overview?.preferred_ssids[0] || "已连接局域网"}
                  </span>
                </div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-500 dark:text-zinc-400 flex items-center gap-1.5">
                    <Radar className="w-4 h-4 text-indigo-500" />
                    已识别路由器
                  </span>
                  <span className="font-semibold">
                    {overview?.router.brand ?? "家用路由器"}
                  </span>
                </div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-slate-500 dark:text-zinc-400 flex items-center gap-1.5">
                    <ShieldCheck className="w-4 h-4 text-emerald-500" />
                    本机数据隐私
                  </span>
                  <span className="text-emerald-600 dark:text-emerald-400 font-medium">
                    100% 本地保存，不上传云端
                  </span>
                </div>
              </div>

              {overview?.ssid_redacted && (
                <div className="text-xs rounded-xl bg-amber-50 dark:bg-amber-500/10 border border-amber-200/70 dark:border-amber-500/20 text-amber-800 dark:text-amber-300 p-3 leading-relaxed">
                  小提示：macOS 开启“定位权限”后才能显示当前 Wi-Fi 名称；如果不授权也能正常扫描全网设备，只是分享密码时需要手动选一下 Wi-Fi 名。
                </div>
              )}

              <div className="flex justify-end pt-2">
                <button
                  onClick={() => setStep(2)}
                  className="text-xs font-semibold px-5 py-2.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 shadow-sm shadow-blue-600/20 transition cursor-pointer"
                >
                  下一步：认领设备 ({candidates.length} 台)
                </button>
              </div>
            </>
          ) : (
            <>
              {candidates.length === 0 ? (
                <div className="py-8 text-center text-xs text-slate-500">
                  正在扫描局域网设备，你可以先进入主界面查看结果。
                </div>
              ) : (
                <div className="space-y-2.5 max-h-72 overflow-y-auto pr-1">
                  {candidates.map((d) => (
                    <div
                      key={d.mac}
                      className="rounded-2xl border border-slate-200/80 dark:border-zinc-800 p-3 space-y-2"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex items-center gap-2 min-w-0">
                          <span className="w-7 h-7 rounded-lg bg-slate-100 dark:bg-zinc-800 flex items-center justify-center text-slate-600 dark:text-zinc-300 shrink-0">
                            {d.device_type === "phone" ? (
                              <Smartphone className="w-3.5 h-3.5" />
                            ) : d.device_type === "laptop" ? (
                              <Laptop className="w-3.5 h-3.5" />
                            ) : (
                              <Tv className="w-3.5 h-3.5" />
                            )}
                          </span>
                          <div className="min-w-0">
                            <div className="text-xs font-semibold truncate flex items-center gap-1.5">
                              <span>{d.display_name}</span>
                              {d.is_self && (
                                <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300">
                                  本机
                                </span>
                              )}
                            </div>
                            <div className="text-[11px] text-slate-400 truncate">
                              {d.vendor}
                            </div>
                          </div>
                        </div>

                        {savedMacs[d.mac] && (
                          <span className="inline-flex items-center gap-1 text-[11px] text-emerald-600 dark:text-emerald-400 font-medium shrink-0">
                            <Check className="w-3.5 h-3.5" />
                            已保存
                          </span>
                        )}
                      </div>

                      <div className="flex items-center gap-1.5 flex-wrap">
                        <input
                          value={drafts[d.mac] ?? ""}
                          onChange={(e) =>
                            setDrafts((prev) => ({ ...prev, [d.mac]: e.target.value }))
                          }
                          placeholder="输入备注，如：我的电脑"
                          className="flex-1 min-w-36 text-xs px-2.5 py-1.5 rounded-lg border border-slate-200 dark:border-zinc-700 bg-slate-50 dark:bg-zinc-950 focus:outline-none focus:border-blue-500"
                        />
                        {QUICK_NAMES.slice(0, 2).map((q) => (
                          <button
                            key={q}
                            type="button"
                            onClick={() => handleQuickSave(d, q)}
                            className="text-[11px] px-2 py-1.5 rounded-lg bg-slate-100 hover:bg-blue-50 hover:text-blue-600 dark:bg-zinc-800 dark:hover:bg-blue-500/15 transition cursor-pointer"
                          >
                            {q}
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div className="flex items-center justify-between pt-2">
                <button
                  onClick={() => setStep(1)}
                  className="text-xs px-3.5 py-2 rounded-xl border border-slate-200 dark:border-zinc-700 hover:bg-slate-100 dark:hover:bg-zinc-800 transition cursor-pointer"
                >
                  上一步
                </button>
                <button
                  onClick={handleComplete}
                  disabled={finishing}
                  className="text-xs font-semibold px-5 py-2.5 rounded-xl bg-blue-600 text-white hover:bg-blue-700 shadow-sm shadow-blue-600/20 transition cursor-pointer"
                >
                  {finishing ? "正在进入..." : "完成设置，进入主面板"}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
