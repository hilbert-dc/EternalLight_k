import { useEffect, useRef, useState } from "react";
import type { ConditionEvaluationJob, ConditionEvaluationTarget, ConditionExplanationPage, NovelCodexApi } from "./types";
import "./condition-evaluation.css";

type Props = { api: NovelCodexApi; projectId: string; target: ConditionEvaluationTarget; initialJobId?: string };
const statuses = { queued: "等待检查", running: "正在检查", paused: "已暂停", completed: "检查完成", cancelled: "已取消", stale: "输入已变化", failed: "检查失败" };
const results = { supported: "来源支持", refuted: "来源反驳", both: "来源冲突", unknown: "尚不能判定" };

export function ConditionEvaluationPanel(props: Props) {
  return <Panel key={JSON.stringify([props.projectId, props.target.snapshotId, props.target.sourceBatchId, props.target.assertionIndex])} {...props} />;
}

function Panel(props: Props) {
  const [open, setOpen] = useState(false);
  const [jobId, setJobId] = useState(props.initialJobId);
  return <span className="condition-evaluation">
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "收起条件检查" : "完整条件检查"}</button>
    {open && <Controls {...props} initialJobId={jobId} onJob={setJobId} />}
  </span>;
}

function Controls({ api, projectId, target, initialJobId, onJob }: Props & { onJob: (id: string) => void }) {
  const [id, setId] = useState(initialJobId);
  const [job, setJob] = useState<ConditionEvaluationJob | null>(null);
  const [page, setPage] = useState<ConditionExplanationPage | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const pending = useRef(false);
  const current = useRef<ConditionEvaluationJob | null>(null);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  async function run<T>(request: () => Promise<T>, apply: (value: T) => void) {
    if (pending.current) return;
    pending.current = true; setBusy(true);
    try {
      const value = await request();
      if (alive.current) { apply(value); setError(""); }
    } catch (reason) {
      if (alive.current) {
        setError(reason instanceof Error ? reason.message : "条件检查请求失败");
        current.current = null; setJob(null); setPage(null);
      }
    } finally {
      pending.current = false;
      if (alive.current) setBusy(false);
    }
  }
  function accept(next: ConditionEvaluationJob) {
    if (next.projectId !== projectId || next.target.snapshotId !== target.snapshotId || next.target.sourceBatchId !== target.sourceBatchId || next.target.assertionIndex !== target.assertionIndex) throw new Error("条件检查范围不匹配");
    if (current.current?.version !== next.version || current.current?.bindingDigest !== next.bindingDigest || next.status !== "completed") setPage(null);
    current.current = next; setJob(next); setId(next.id); onJob(next.id);
  }
  useEffect(() => {
    if (!id) return;
    const refresh = () => void run(() => api.runtime.getConditionEvaluation(projectId, id), accept);
    refresh();
    const timer = window.setInterval(() => { if (!current.current || !["failed", "cancelled", "stale"].includes(current.current.status)) refresh(); }, 2000);
    return () => window.clearInterval(timer);
  }, [api, projectId, id]);
  const control = (command: "pause" | "resume" | "cancel") => {
    if (job) void run(() => api.runtime.controlConditionEvaluation(projectId, job.id, command, job.version), accept);
  };
  const explain = (cursor: string | null) => {
    if (job?.status === "completed") void run(() => api.runtime.getConditionExplanations(projectId, job.id, cursor), setPage);
  };
  return <span className="condition-evaluation-body" aria-busy={busy}>
    <small role="status">{job ? `${statuses[job.status]} · 已检查 ${job.scannedSources} 个来源批次` : id ? error ? "检查状态不可用" : "正在读取检查状态" : "尚未启动完整检查"}</small>
    <span className="condition-evaluation-actions">
      {(!id || job && ["failed", "cancelled", "stale"].includes(job.status)) && <button type="button" disabled={busy} onClick={() => void run(() => api.runtime.startConditionEvaluation(projectId, target), accept)}>开始完整检查</button>}
      {job && ["queued", "running"].includes(job.status) && <button type="button" disabled={busy} onClick={() => control("pause")}>暂停检查</button>}
      {job?.status === "paused" && <button type="button" disabled={busy} onClick={() => control("resume")}>恢复检查</button>}
      {job && ["queued", "running", "paused", "failed", "stale"].includes(job.status) && <button type="button" disabled={busy} onClick={() => control("cancel")}>取消检查</button>}
      {id && <button type="button" disabled={busy} onClick={() => void run(() => api.runtime.getConditionEvaluation(projectId, id), accept)}>刷新检查</button>}
    </span>
    {error && <small role="alert">{error}</small>}
    {!job && !id && <small>需要已采用规则及有效的知识索引校验。检查不会确认事实或改写正文。</small>}
    {job?.status === "stale" && <small>来源或规则已变化，旧结果不可用；重新检查将绑定当前输入。</small>}
    {job?.status === "failed" && <small>执行未完成，保留上次进度；不会自动重试。</small>}
    {job?.status === "completed" && job.result && <>
      <small>本次结果：{results[job.result.result]}。{job.result.coverage?.complete ? "计算范围完整" : "仍有未解释项"}；不认证世界真相。</small>
      <button type="button" disabled={busy} onClick={() => explain(null)}>{page ? "回到解释第一页" : "查看条件解释"}</button>
      {page && <span role="list" aria-label="条件比较解释">{page.data.map((row, index) => <span role="listitem" className="condition-explanation" key={index}>
        <small>条件 {row.leafIndex + 1} · {row.subjectId} · {results[row.assessment.result]} · 操作数 {JSON.stringify(row.operand)}</small>
        <small>支持：{row.assessment.support ? `${row.assessment.support.sourceBatchId} #${row.assessment.support.assertionIndex + 1}` : "无代表来源"}</small>
        <small>反对：{row.assessment.refute ? `${row.assessment.refute.sourceBatchId} #${row.assessment.refute.assertionIndex + 1}` : "无代表来源"}；未决 {row.assessment.unresolvedChecks}</small>
        {(["support", "refute"] as const).map(side => {
          const excerpt = row.excerpts[side];
          return excerpt && <span className="condition-source-excerpt" key={side}>
            <small>{side === "support" ? "支持主张的原文" : "反对主张的原文"}{excerpt.truncated ? "（节选）" : ""}：{excerpt.quote}</small>
            <small>来源 {excerpt.sourceObjectId} · 修订 {excerpt.sourceRevisionId} · 字符 [{excerpt.start}, {excerpt.end})</small>
          </span>;
        })}
      </span>)}{!page.data.length && <small>没有可列出的比较项，请结合未决检查阅读结果。</small>}</span>}
      {page?.nextCursor && <button type="button" disabled={busy} onClick={() => explain(page.nextCursor)}>下一页解释</button>}
    </>}
    <small>仅列出各比较的代表证据。关闭详情不取消后台检查。</small>
  </span>;
}
