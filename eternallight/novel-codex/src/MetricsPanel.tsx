import { useEffect, useState } from "react";
import type { LocalMetricsSummary, NovelCodexApi } from "./types";

const stages = { brief: "创作方向", outline: "全书大纲", plan: "近章规划", draft: "正文" };
const pendingKey = "novel-codex:pending-metric-action";
type PendingAction = { stage: keyof typeof stages; key: string };
function readPending(): PendingAction | null {
  try {
    const value = JSON.parse(localStorage.getItem(pendingKey) ?? "null");
    return value && Object.hasOwn(stages, value.stage) && /^[a-f0-9-]{36}$/.test(value.key) ? value : null;
  } catch { return null; }
}
const events: Record<string, string> = { saved: "候选保存", changedSave: "内容变更保存", staleResave: "旧基线候选续存", adopted: "采用", abandoned: "手动记录放弃" };
const labels: Record<string, string> = {
  "feedback.autoMatch": "反馈自动命中", "feedback.manualLink": "反馈手动补链", "feedback.kept": "反馈：保留安排", "feedback.updated": "反馈：修改安排", "feedback.falsePositive": "反馈：误报",
  "localOnly.created": "独立章节创建", "localOnly.associated": "独立章节关联规划", "localOnly.adopted": "独立正文采用",
  "generation.completed": "任务完成", "generation.failed": "任务失败", "generation.cancelled": "任务取消",
};
function label(key: string) {
  const [stage, event] = key.split(".");
  return labels[key] ?? `${stages[stage as keyof typeof stages]} · ${events[event]}`;
}

export function MetricsPanel({ api }: { api: NovelCodexApi }) {
  const [summary, setSummary] = useState<LocalMetricsSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<PendingAction | null>(readPending);
  const [stage, setStage] = useState<keyof typeof stages>("brief");
  const refresh = async () => {
    setBusy(true); setMessage("");
    try { setSummary(await api.runtime.getLocalMetrics()); }
    catch { setMessage("无法读取本地统计，请重试。"); }
    finally { setBusy(false); }
  };
  useEffect(() => { void refresh(); }, [api]);
  const abandon = async () => {
    setBusy(true); setMessage("");
    try {
      const action = pending ?? { stage, key: crypto.randomUUID() };
      localStorage.setItem(pendingKey, JSON.stringify(action)); setPending(action);
      setSummary(await api.runtime.recordStageAbandonment(action.stage, action.key));
      localStorage.removeItem(pendingKey); setPending(null);
      setMessage("已记录一次放弃；作品和候选内容未变更。");
    }
    catch { setMessage("记录结果尚未确认。重试会复用本次凭证，避免重复计数。"); }
    finally { setBusy(false); }
  };
  const exportSummary = () => {
    if (!summary) return;
    const url = URL.createObjectURL(new Blob([JSON.stringify(summary, null, 2)], { type: "application/json" }));
    const anchor = document.createElement("a"); anchor.href = url; anchor.download = "小说工坊-本地创作统计.json"; anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    setMessage("已发起匿名汇总下载。");
  };
  return <section className="inspector-stack" aria-label="本地创作统计">
    <h3>本地创作统计</h3>
    <small className="inspector-note">仅保存在本机，汇总所有作品；不联网发送，不含正文、秘密、名称、ID 或原始时间戳。撤销不抹去已发生的操作次数。</small>
    <small className="inspector-note">本机另保留最近 1,000 次手动操作的随机凭证用于去重，不关联作品或人物，也不导出。</small>
    <small className="inspector-note">内容变更保存次数是返工代理，不能衡量实质重写或文学质量。旧基线候选续存不等同于复制按钮点击。</small>
    <small className="inspector-note">采用历时从所采用候选保存起算；反馈历时从产生到处理，任务历时从创建到终止。均为墙钟时间（含等待和离开），不是专注创作时间。</small>
    <div className="inspector-actions"><button disabled={busy} onClick={() => void refresh()}>刷新统计</button><button disabled={busy || !summary} onClick={exportSummary}>导出匿名汇总</button></div>
    <div role="status" aria-live="polite">{busy ? "读取或保存统计中…" : message}</div>
    {summary && <dl>{summary.metrics.map((row) => <div key={row.metric}><dt>{label(row.metric)}</dt><dd>{row.count} 次{row.metric.endsWith(".adopted") && !row.metric.startsWith("localOnly.") || row.metric.startsWith("generation.") || ["feedback.kept", "feedback.updated", "feedback.falsePositive"].includes(row.metric) ? ` · 累计 ${row.totalSeconds} 秒` : ""}</dd></div>)}</dl>}
    <label className="settings-field"><span>这次放弃的阶段</span><select disabled={busy || Boolean(pending)} value={pending?.stage ?? stage} onChange={(event) => setStage(event.target.value as keyof typeof stages)}>{Object.entries(stages).map(([value, text]) => <option key={value} value={value}>{text}</option>)}</select></label>
    <small className="inspector-note">只在你确认放弃这次尝试时记录；切页、退出或取消任务不会被自动当成放弃。</small>
    {pending && <small>有一项待确认记录，请重试完成后再记录下一次。</small>}
    <button disabled={busy} onClick={() => void abandon()}>{pending ? "重试确认放弃记录" : "记录一次放弃"}</button>
  </section>;
}
