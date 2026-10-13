import { useId, useState, type FormEvent } from "react";
import type { PlanningReviewArtifactKind, PlanningReviewFinding, PlanningReviewReport, PlanningReviewRole, PlanningReviewRun, PlanningReviewStartRequest } from "./types";
import "./analysis-entry.css";

export type { PlanningReviewStartRequest } from "./types";

export interface PlanningReviewPanelProps {
  runs: PlanningReviewRun[];
  artifactKind: PlanningReviewArtifactKind;
  revisionId: string | null;
  outlineNodes?: { id?: string; kind: "book" | "volume" | "unit"; title: string }[];
  busy: boolean;
  agentAvailable: boolean;
  onStart: (request: PlanningReviewStartRequest) => void | Promise<void>;
  onCancel: (runId: string) => void | Promise<void>;
  onDiscuss?: (prompt: string) => void;
}

const roleLabels: Record<PlanningReviewRole, string> = {
  structure: "结构与因果",
  character: "人物与主题",
  experience: "阅读体验",
};
const runLabels: Record<PlanningReviewRun["status"], string> = {
  running: "正在评审",
  completed: "评审已完成",
  partial: "评审部分完成",
  cancelled: "评审已停止",
};
const checkLabels: Record<PlanningReviewReport["report"]["coverage"][number]["status"], string> = {
  checked: "已检查",
  unknown: "无法判断",
  notChecked: "未检查",
};

export function PlanningReviewPanel({ runs, artifactKind, revisionId, outlineNodes = [], busy, agentAvailable, onStart, onCancel, onDiscuss }: PlanningReviewPanelProps) {
  const id = useId();
  const [intent, setIntent] = useState("");
  const [scope, setScope] = useState("");
  const [selectedRoles, setSelectedRoles] = useState<PlanningReviewRole[]>(["structure"]);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const volumes = outlineNodes.filter((node): node is typeof node & { id: string } => node.kind === "volume" && Boolean(node.id));
  const latest = runs.filter((run) => run.artifactKind === artifactKind).sort((left, right) => right.createdAt.localeCompare(left.createdAt))[0];
  const running = latest?.status === "running";
  const stale = Boolean(latest && latest.revisionId !== revisionId);
  const scopeMissing = artifactKind === "outline" && Boolean(scope) && !volumes.some((node) => node.id === scope);
  const disabled = busy || starting || running;
  const canStart = !disabled && agentAvailable && Boolean(revisionId) && Boolean(intent.trim()) && !scopeMissing && selectedRoles.length > 0;
  const missingRoles = latest?.roles.filter((role) => {
    const report = latest.reports.find((item) => item.role === role);
    return !report || report.status !== "complete" || (latest.checksByRole[role] ?? []).some((checkId) => !report.report.coverage.some((check) => check.checkId === checkId && check.status === "checked"));
  }) ?? [];
  const incompleteRoles = [...new Set([...missingRoles, ...(latest?.synthesis?.incompleteRoles ?? [])])];
  const returnedFindings = latest?.reports.flatMap((report) => report.report.findings.map((finding) => ({ role: report.role, finding }))) ?? [];
  const priorityFindings = latest?.synthesis
    ? latest.synthesis.priorities.map((priority) => ({ role: priority.role, finding: latest.reports.find((report) => report.role === priority.role && report.id === priority.reportId)?.report.findings[priority.findingIndex] }))
    : returnedFindings;
  const unresolvedPriorities = priorityFindings.some((item) => !item.finding);
  const strengths = latest?.reports.flatMap((report) => report.report.strengths) ?? [];
  const expectedCheckCount = latest?.roles.reduce((count, role) => count + (latest.checksByRole[role]?.length ?? 0), 0) ?? 0;
  const checkedCount = latest?.roles.reduce((count, role) => {
    const report = latest.reports.find((item) => item.role === role);
    return count + (latest.checksByRole[role] ?? []).filter((checkId) => report?.report.coverage.some((check) => check.checkId === checkId && check.status === "checked")).length;
  }, 0) ?? 0;

  async function start(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canStart || !revisionId) return;
    setStarting(true);
    setError(null);
    try {
      await onStart({
        artifactKind,
        revisionId,
        targetNodeId: artifactKind === "outline" && scope ? scope : null,
        authorIntent: intent.trim(),
        roles: selectedRoles,
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "评审未能启动，关注点仍保留在输入框中。");
    } finally {
      setStarting(false);
    }
  }

  async function cancel() {
    if (!latest || !running || cancelling) return;
    setCancelling(true);
    setError(null);
    try {
      await onCancel(latest.id);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "尚未确认评审已停止，请查看任务状态后重试停止。");
    } finally {
      setCancelling(false);
    }
  }

  return <section className="planning-review-panel" aria-label="规划讨论与评审">
    {onDiscuss && <div><button type="button" className="secondary-button" disabled={busy || starting || running || !revisionId} onClick={() => {
      if (busy || starting || running || !revisionId) return;
      onDiscuss(`请读取当前已保存的${artifactKind === "outline" ? "大纲" : "章纲"}，根据当前安排选择值得检查的问题和必要视角，给出有材料依据的建议；可以直接讨论，必要时再进行独立规划检查。先保留原案，不修改或采用规划。`);
    }}>在对话中讨论这份规划</button><p>说明你关心的问题，由 AI 选择合适的检查方式；也可以在下方指定视角。</p></div>}
    <details><summary>指定规划检查视角</summary>
    <header className="planning-review-heading">
      <div><h3 className="sr-only" id={`${id}-title`}>规划深审</h3><p id={`${id}-description`}>从不同视角审阅安排，保留有价值的设计和分歧。只生成意见，不修改或采用规划；无需先写正文，也不必先确定结局。</p></div>
      <span className="planning-review-badge">只读评审</span>
    </header>
    <form className="planning-review-form" onSubmit={(event) => void start(event)} aria-describedby={`${id}-description`}>
      <label className="planning-review-scope"><span>审阅范围</span>{artifactKind === "outline"
        ? <select value={scope} disabled={disabled} onChange={(event) => setScope(event.target.value)} aria-invalid={scopeMissing || undefined} aria-describedby={scopeMissing ? `${id}-scope-error` : undefined}>
          <option value="">全书大纲</option>
          {scopeMissing && <option value={scope}>原选卷已不在当前版本</option>}
          {volumes.map((node) => <option key={node.id} value={node.id}>{node.title || "未命名卷"}</option>)}
        </select>
        : <span className="planning-review-scope-value">当前章纲</span>}
      </label>
      {scopeMissing && <p id={`${id}-scope-error`} className="field-error" role="alert">当前版本没有这卷，请重新选择范围。</p>}
      <label className="planning-review-intent"><span>本次关注点</span><textarea value={intent} onChange={(event) => setIntent(event.target.value)} disabled={disabled} required rows={3} placeholder="例如：保留结局，看看第二卷的人物行动和前后承接是否成立。" /></label>
      <fieldset className="planning-review-depth" disabled={disabled}>
        <legend>检查视角 · 按需选择</legend>
        {(Object.keys(roleLabels) as PlanningReviewRole[]).map(role => <label key={role}>
          <input type="checkbox" checked={selectedRoles.includes(role)} onChange={event => setSelectedRoles(current => event.target.checked ? [...current, role] : current.filter(item => item !== role))} />
          <span><strong>{roleLabels[role]}</strong><small>{{ structure: "检查事件推进和前后承接", character: "检查人物动机和选择", experience: "检查节奏、悬念和阅读感受" }[role]}</small></span>
        </label>)}
        {selectedRoles.length === 0 && <p role="status">选择一个或多个视角，也可以直接在对话中说明关注点。</p>}
      </fieldset>
      <div className="planning-review-start">
        <button type="submit" className="secondary-button" disabled={!canStart}>{starting ? "正在启动…" : "开始检查"}</button>
        <small>{!revisionId ? "先保存这份规划，即可开始只读评审。" : !agentAvailable ? "配置可用模型后即可评审；关注点可以先写。" : "只执行选中的视角，在原任务额度内交付意见。"}</small>
      </div>
    </form>
    </details>
    {error && <p className="creative-alert error" role="alert">{error}</p>}
    {latest && <section className="planning-review-result" aria-labelledby={`${id}-result-title`}>
      <header className="planning-review-heading">
        <div><h4 id={`${id}-result-title`}>最近一次评审</h4><p role="status" aria-live="polite">{runLabels[latest.status]} · 已收到 {latest.reports.length}/{latest.roles.length} 个视角的报告</p></div>
        {running && <button type="button" className="secondary-button" disabled={cancelling} onClick={() => void cancel()}>{cancelling ? "正在停止…" : "停止本次评审"}</button>}
      </header>
      <p className="planning-review-request">关注点：{latest.authorIntent}</p>
      <p className="planning-review-note">范围：{latest.artifactKind === "plan" ? "章纲" : latest.targetNodeId ? "指定卷" : "全书大纲"}。阅读体验意见基于规划材料，不代表真实读者试读。</p>
      {stale && <p className="creative-alert warning" role="status">当前规划版本已变化，以下意见仍对应原版本，不代表已检查新稿。原报告继续保留。</p>}
      {latest.status !== "running" && (latest.status !== "completed" || incompleteRoles.length > 0) && <p className="creative-alert warning">本次未完成全部检查。已保存的意见仍可参考，未完成不等于作品存在问题。</p>}
      {incompleteRoles.length > 0 && <p className="planning-review-note">{running ? "尚未完成" : "缺少完整结果"}：{incompleteRoles.map((role) => roleLabels[role]).join("、")}。</p>}
      <p className="planning-review-note">本次约定 {expectedCheckCount} 项检查，已检查 {checkedCount} 项。无法判断、未检查和未返回的项目不计为已检查。</p>
      <details className="planning-review-reading"><summary>查看深审结果</summary>
      {strengths.length > 0 && <div className="planning-review-strengths"><h4>值得保留</h4><ul>{strengths.slice(0, 3).map((strength, index) => <li key={index}><p>{strength.reason}</p><blockquote>{strength.quote}</blockquote></li>)}</ul>{strengths.length > 3 && <small>其余保留项见各视角的完整报告。</small>}</div>}
      {priorityFindings.length > 0 && <div className="planning-review-priorities"><h4>{latest.synthesis ? "优先关注" : "已返回意见 · 尚未综合"}</h4>{priorityFindings.slice(0, 3).map(({ role, finding }, index) => finding && <FindingCard key={`${role}-${index}`} finding={finding} role={role} />)}<small>最多展示三条，全部意见及检查依据见下方报告。</small></div>}
      {unresolvedPriorities && <p className="creative-alert warning">部分综合意见未能定位到原报告，不能据此视为检查完成。</p>}
      {latest.synthesis && latest.synthesis.disagreements.length > 0 && <div className="planning-review-disagreements"><h4>仍有分歧</h4><ul>{latest.synthesis.disagreements.map((disagreement, index) => <li key={index}>{disagreement}</li>)}</ul><p>这些取舍由你决定，不影响保留原案或继续创作。</p></div>}
      {latest.status === "completed" && latest.reports.length > 0 && latest.reports.every((report) => report.report.findings.length === 0) && <p className="planning-review-note">已检查的视角没有提出具体问题；这不等于文学质量已经得到保证。</p>}
      <div className="planning-review-reports">{latest.roles.map((role) => {
        const report = latest.reports.find((item) => item.role === role);
        const missingChecks = (latest.checksByRole[role] ?? []).filter((checkId) => !report?.report.coverage.some((check) => check.checkId === checkId));
        return <details key={role}>
          <summary>{roleLabels[role]} · {report ? report.status === "complete" ? "报告已交付" : "报告部分完成" : running ? "等待结果" : "未交付报告"}</summary>
          {report ? <div className="planning-review-report-body">
            <p>{report.report.summary}</p>
            {report.report.strengths.length > 0 && <div><h5>值得保留</h5><ul>{report.report.strengths.map((strength, index) => <li key={index}><p>{strength.reason}</p><blockquote>{strength.quote}</blockquote></li>)}</ul></div>}
            {report.report.findings.map((finding, index) => <FindingCard key={index} role={role} finding={finding} />)}
            <div className="planning-review-coverage"><h5>检查覆盖与依据</h5>{missingChecks.length > 0 && <p>尚缺 {missingChecks.length} 项约定检查的结果。</p>}{report.report.coverage.length === 0 ? <p>没有返回逐项覆盖，不能判断已检查完整范围。</p> : <ul>{report.report.coverage.map((check) => <li key={check.checkId}><strong>{checkLabels[check.status]}</strong>{check.reason && <p>{check.reason}</p>}<details><summary>检查项定位</summary><p>{check.checkId}</p></details></li>)}</ul>}</div>
          </div> : <p>尚无可展示的持久化报告。</p>}
        </details>;
      })}</div>
      <details className="planning-review-source"><summary>本次来源记录</summary><p>规划修订：{latest.revisionId}</p><p>作品版本：{latest.inputRevision}</p><p>材料摘要：{latest.sourcePacketHash}</p>{latest.targetNodeId && <p>卷节点：{latest.targetNodeId}</p>}<p>创建时间：<time dateTime={latest.createdAt}>{latest.createdAt}</time></p></details>
      <p className="planning-review-note">评审意见不会阻止采用。你可以保留原案、稍后再看，或继续编辑。</p>
      </details>
    </section>}
  </section>;
}

function FindingCard({ finding, role }: { finding: PlanningReviewFinding; role: PlanningReviewRole }) {
  return <article className="planning-review-finding">
    <small>{roleLabels[role]}</small><p className="planning-review-observation">{finding.observation}</p>
    {finding.quote && <blockquote>{finding.quote}</blockquote>}
    <p><strong>影响：</strong>{finding.impact}</p>
    {finding.suggestion && <p><strong>建议：</strong>{finding.suggestion}</p>}
  </article>;
}
