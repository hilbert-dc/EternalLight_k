import { useEffect, useRef, useState } from "react";
import type { GenerationReplanPreview, GenerationRunSummary, NovelCodexApi, ReplanStage } from "./types";
import "./generation-replan.css";

type Props = { api: NovelCodexApi; projectId: string; runId: string; disabled: boolean; onConfirmed: (run: GenerationRunSummary) => void };
const choices: Record<ReplanStage, { label: string; detail: string; preparation: number }> = {
  brief: { label: "从创作简报开始", detail: "重建简报、大纲和近章规划；已有正文保留。", preparation: 2 },
  outline: { label: "从全书大纲开始", detail: "保留当前简报，重建大纲和近章规划；已有正文保留。", preparation: 1 },
  rollingPlan: { label: "只重建近章规划", detail: "保留当前简报、大纲和已有正文。", preparation: 0 },
  currentChapter: { label: "基于当前章重写", detail: "以作者修改后的当前稿为依据重写本章，再重新建模和评估；前文与当前规划绑定保留。", preparation: -1 },
};

export function GenerationReplanPanel(props: Props) {
  return <Controls key={`${props.projectId}/${props.runId}`} {...props} />;
}

function Controls({ api, projectId, runId, disabled, onConfirmed }: Props) {
  const [preview, setPreview] = useState<GenerationReplanPreview | null>(null);
  const [stage, setStage] = useState<ReplanStage>("outline");
  const [instruction, setInstruction] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  async function request(action: () => Promise<void>) {
    if (pending.current || disabled) return;
    pending.current = true; setBusy(true); setError("");
    try { await action(); }
    catch (reason) {
      if (alive.current) { setPreview(null); setError(`${reason instanceof Error ? reason.message : "请求失败"}。请重新预览当前状态后再确认。`); }
    } finally { pending.current = false; if (alive.current) setBusy(false); }
  }
  function load() {
    void request(async () => {
      setPreview(null);
      const value = await api.runtime.previewGenerationReplan(projectId, runId);
      if (!alive.current) return;
      if (value.run.projectId !== projectId || value.run.id !== runId) throw new Error("重规划范围不匹配");
      setPreview(value);
      setStage(value.availableFrom.includes("outline") ? "outline" : value.availableFrom[0]);
    });
  }
  const run = preview?.run;
  const eligible = run && ["paused", "blocked"].includes(run.status)
    && (stage === "currentChapter" ? ["draft", "extract", "commitments", "assess", "check"].includes(run.stage)
      : ["brief", "outline", "rollingPlan", "materializeChapter"].includes(run.stage) && !run.currentChapterId)
    && (!run.activeTaskId || ["completed", "failed", "cancelled"].includes(preview.activeAttemptStatus ?? ""));
  const remainingAttempts = run ? run.maxTaskAttempts - run.usedTaskAttempts : 0;
  const remainingRequests = run ? run.providerBudget.maxRequests - run.providerBudget.usedRequests : 0;
  const remainingTokens = run ? run.providerBudget.maxTokens - run.providerBudget.usedTokens - run.providerBudget.unknownTokens : 0;
  const required = run ? (run.targetChapters - run.completedChapters) * 5 + choices[stage].preparation : 0;
  const providerBudgetAvailable = Boolean(run && !run.providerBudget.overrun && remainingRequests >= required && remainingTokens > 0);
  const canConfirm = eligible && preview?.availableFrom.includes(stage) && remainingAttempts >= required && providerBudgetAvailable
    && run.deadlineAt > Date.now() / 1000 && instruction.trim() && Array.from(instruction.trim()).length <= 1000;
  return <section className="generation-replan" aria-label="调整后重新规划" aria-busy={busy}>
    <strong>按当前作品重新规划</strong>
    <p>选择需要重建的范围，补充这一轮的创作要求。</p>
    <button type="button" className="secondary-button" disabled={disabled || busy} onClick={load}>{busy ? "处理中…" : preview ? "重新预览" : "预览重规划"}</button>
    {error && <p role="alert">{error}</p>}
    {preview && run && <>
      <label>重建范围<select aria-label="重建范围" value={stage} disabled={disabled || busy} onChange={event => setStage(event.target.value as ReplanStage)}>
        {preview.availableFrom.map(value => <option key={value} value={value}>{choices[value].label}</option>)}
      </select></label>
      <p>{choices[stage].detail}</p>
      <p>剩余 {remainingAttempts} 次任务尝试，本范围至少需要 {required} 次。已完成 {run.completedChapters} 章，目标共 {run.targetChapters} 章。</p>
      <p>供应商预算：剩余 {remainingRequests} 次请求、{remainingTokens.toLocaleString()} token；已使用 {run.providerBudget.usedTokens.toLocaleString()} token，未知用量 {run.providerBudget.unknownTokens.toLocaleString()} token。</p>
      <small>原截止时间：{new Date(run.deadlineAt * 1000).toLocaleString()}。预算和截止时间保持不变。</small>
      {!eligible && <p role="alert">请先核对当前任务或未完成章节；此状态暂不能重规划。</p>}
      {remainingAttempts < required && <p role="alert">剩余任务尝试次数不足，请缩小本次工作范围或另行安排创作。</p>}
      {!providerBudgetAvailable && <p role="alert">供应商请求或 token 预算不足，不能继续生成；重规划不会增加原预算。</p>}
      <label>本轮补充要求<textarea aria-label="重规划补充要求" value={instruction} disabled={disabled || busy} maxLength={1000} onChange={event => setInstruction(event.target.value)} /></label>
      <small>原创作要求继续有效。确认后可点击“继续创作”推进；新内容先保存为候选。</small>
      <button type="button" className="primary-button" disabled={disabled || busy || !canConfirm} onClick={() => void request(async () => {
        const result = await api.runtime.replanGeneration(projectId, runId, {
          expectedVersion: run.version, expectedProjectRevision: preview.projectRevision,
          expectedSourceDigest: preview.sourceDigest, restartFrom: stage, instruction: instruction.trim(),
        });
        if (alive.current) { setPreview(null); onConfirmed(result); }
      })}>确认重规划</button>
    </>}
  </section>;
}
