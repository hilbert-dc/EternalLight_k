import { useEffect, useId, useRef } from "react";
import type { HypothesisResult, PlanningReviewRole, SavedAnalysis, StateProperty } from "./types";
import "./saved-analysis.css";

const roles: Record<PlanningReviewRole, string> = { structure: "结构与因果", character: "人物与主题", experience: "阅读体验" };
const checks = { checked: "已检查", unknown: "无法判断", notChecked: "未检查" };
const states = { known: "已知", suspected: "怀疑", false: "误信" };
const runStates = { running: "正在评审", completed: "评审已完成", partial: "评审部分完成", cancelled: "评审已停止" };
const findingLabels: Record<HypothesisResult["simulation"]["findings"][number]["type"], string> = { polarityConflict: "相反主张可能冲突", epistemicDisagreement: "人物认知存在分歧", stateValueConflict: "同一时段的属性值存在冲突", stateSchemaDisagreement: "属性解释规则存在分歧" };
const valueText = (value: unknown) => typeof value === "string" ? value : value === true ? "是" : value === false ? "否" : value === null ? "未指定" : typeof value === "number" ? String(value) : "结构化值";
const stateValueText = (value: string | number | boolean | { entityId: string }) => typeof value === "object" ? "关联对象（见来源定位）" : valueText(value);
function schemaText(schema: StateProperty["schema"]) {
  const value = schema.valueType;
  const kind = value.type === "entity" ? `关联${{ character: "人物", organization: "组织", location: "地点", item: "物品", proposition: "命题", event: "事件" }[value.kind]}` : { boolean: "是或否", integer: "整数", text: "文本" }[value.type];
  return `${schema.cardinality === "single" ? "同一时刻只取一个值" : "同一时刻可有多个值"} · ${kind}`;
}

export function SavedAnalysisDialog({ analysis, onClose }: { analysis: SavedAnalysis; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const id = useId();
  const identity = analysis.kind === "planningReview" ? `${analysis.run.projectId}:${analysis.run.id}:${analysis.reportId}` : `${analysis.result.projectId}:${analysis.result.id}`;
  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement as HTMLElement | null;
    element?.showModal();
    close.current?.focus({ preventScroll: true });
    return () => { element?.close(); if (previous?.isConnected && previous !== document.body) previous.focus({ preventScroll: true }); };
  }, []);
  useEffect(() => { close.current?.focus({ preventScroll: true }); }, [identity]);
  return <dialog ref={dialog} className="saved-analysis-dialog" aria-labelledby={`${id}-title`} aria-describedby={`${id}-notice`}
    data-discussion-document="" data-discussion-project={analysis.kind === "planningReview" ? analysis.run.projectId : analysis.result.projectId}
    data-discussion-view={analysis.kind === "planningReview" ? analysis.run.artifactKind === "outline" ? "outline" : "plans" : "world"}
    data-discussion-label={analysis.kind === "planningReview" ? "规划深审报告" : "假设分析结果"}
    data-discussion-locator={analysis.kind === "planningReview" ? analysis.reportId : analysis.result.id}
    onClose={onClose} onCancel={event => { event.preventDefault(); onClose(); }}>
    <header><h2 id={`${id}-title`}>{analysis.kind === "planningReview" ? "已保存的规划深审" : "已保存的假设分析"}</h2><button ref={close} type="button" aria-label="关闭分析结果" onClick={onClose}>×</button></header>
    <p id={`${id}-notice`} className="saved-analysis-notice">只读已保存结果；不会改写正文、确认故事设定或采用规划。</p>
    <div key={identity}>{analysis.kind === "planningReview" ? <PlanningReport analysis={analysis} /> : <HypothesisReport analysis={analysis} />}</div>
  </dialog>;
}

function PlanningReport({ analysis }: { analysis: Extract<SavedAnalysis, { kind: "planningReview" }> }) {
  const { run, reportId } = analysis;
  const matching = run.reports.filter(report => report.id === reportId);
  const report = matching.length === 1 && matching[0].runId === run.id && run.roles.includes(matching[0].role) ? matching[0] : null;
  if (!report) return <p role="alert">这份报告未能唯一对应本次评审；不能用其他报告替代。请关闭后重新读取任务结果。</p>;
  const expected = run.checksByRole[report.role] ?? [];
  const missing = expected.filter(check => !report.report.coverage.some(item => item.checkId === check));
  const checked = expected.filter(check => report.report.coverage.some(item => item.checkId === check && item.status === "checked"));
  const incomplete = run.roles.filter(role => {
    const items = run.reports.filter(item => item.role === role && item.runId === run.id);
    return items.length !== 1 || items[0].status !== "complete" || (run.checksByRole[role] ?? []).some(check => !items[0].report.coverage.some(item => item.checkId === check && item.status === "checked"));
  });
  return <>
    <p className="saved-analysis-scope">原{run.artifactKind === "outline" ? run.targetNodeId ? "指定卷大纲" : "全书大纲" : "章纲"} · 作品版本 v{run.inputRevision} · {roles[report.role]}</p>
    <p>{runStates[run.status]} · 已收到 {run.reports.length}/{run.roles.length} 席报告</p>
    <p>以下意见对应当时的规划修订，不代表已检查当前稿。阅读体验意见基于规划材料，不代表真实读者试读。</p>
    <p>作者关注点：{run.authorIntent}</p>
    {(run.status !== "completed" || run.coverageStatus !== "complete" || incomplete.length > 0 || run.synthesis?.incompleteRoles.length) && <p className="saved-analysis-warning" role="status">本轮尚未完成全部检查；保留已交付意见，不把未覆盖范围判作没有问题。</p>}
    {incomplete.length > 0 && <p>缺少完整结果：{incomplete.map(role => roles[role]).join("、")}</p>}
    <section><h3>{roles[report.role]} · {report.status === "complete" ? "报告已交付" : "报告部分完成"}</h3><p>{report.report.summary}</p></section>
    {report.report.strengths.length > 0 && <section><h3>值得保留</h3>{report.report.strengths.map((item, index) => <article key={index}><p>{item.reason}</p>{item.quote && <blockquote>{item.quote}</blockquote>}</article>)}</section>}
    <section><h3>可参考的建议</h3>{report.report.findings.length === 0 ? <p>本席没有提出具体问题；这不等于全部检查完成或文学质量得到保证。</p> : report.report.findings.map((item, index) => <article key={index}><h4>{item.observation}</h4>{item.quote && <blockquote>{item.quote}</blockquote>}<p>影响：{item.impact}</p>{item.suggestion && <p>建议：{item.suggestion}</p>}</article>)}</section>
    {run.synthesis?.disagreements.length ? <section><h3>本轮仍有分歧</h3><ul>{run.synthesis.disagreements.map((item, index) => <li key={index}>{item}</li>)}</ul><p>取舍由你决定，原案仍然保留。</p></section> : null}
    <section><h3>检查覆盖</h3><p>本席约定 {expected.length} 项检查，已检查 {checked.length} 项。无法判断与未检查不计作通过。</p>{missing.length > 0 && <p>尚缺 {missing.length} 项约定检查的结果。</p>}
      {report.report.coverage.length === 0 ? <p>没有返回逐项覆盖，不能判断完整性。</p> : <ul>{report.report.coverage.map((item, index) => <li key={index}><strong>{checks[item.status]}</strong>{item.reason && <p>{item.reason}</p>}<details><summary>检查项定位</summary><p>{item.checkId}</p></details></li>)}</ul>}</section>
    <details className="saved-analysis-source"><summary>来源与记录</summary><dl><div><dt>评审记录</dt><dd>{run.id}</dd></div><div><dt>席位报告</dt><dd>{report.id}</dd></div><div><dt>原规划修订</dt><dd>{run.revisionId}</dd></div>{run.targetNodeId && <div><dt>原卷节点</dt><dd>{run.targetNodeId}</dd></div>}<div><dt>材料摘要</dt><dd>{run.sourcePacketHash}</dd></div></dl></details>
  </>;
}

function HypothesisReport({ analysis }: { analysis: Extract<SavedAnalysis, { kind: "hypothesis" }> }) {
  const result = analysis.result;
  const assumed = result.simulation.entries.filter(entry => entry.confirmationState === "assumed");
  return <>
    <p className="saved-analysis-scope">固定历史快照 · 独立假设记录</p><p>本次假设：{result.intent}</p>
    <p>这是反事实分析，未改变故事。仅覆盖原快照收录的正文与知识；未收录的章节、后来的修改与知识不在分析范围。没有发现冲突也不代表没有影响。</p>
    <section><h3>假设条件与临时排除</h3>{result.assumptions.map((item, index) => <article key={index}>
      {item.kind === "assumeClaim" ? <><p>假设条件 {index + 1}：{item.predicate} 为 {valueText(item.value)}{item.storyTime !== null && ` · 故事时刻 ${item.storyTime}`}</p><small>假设条件，不是正文事实。</small><details><summary>对象定位</summary><p>{item.subjectId}</p></details></>
        : <><p>临时排除条件 {index + 1}：本次模拟不使用这条正文主张；原记录未删除。</p><details><summary>被排除主张定位</summary><dl><div><dt>来源修订</dt><dd>{item.sourceRevisionId}</dd></div><div><dt>知识批次</dt><dd>{item.modelingBatchId}</dd></div><div><dt>主张序号</dt><dd>{item.assertionIndex}</dd></div></dl></details></>}
    </article>)}</section>
    <p>原快照有 {result.baseline.entryCount} 条主张；模拟保留 {result.simulation.entries.length - assumed.length} 条原主张，加入 {assumed.length} 条假设。</p>
    <section><h3>分析发现</h3>{result.simulation.findings.length === 0 ? <p>当前模拟未检出明确冲突；未知或未覆盖后果仍需判断。</p> : result.simulation.findings.map((item, index) => <article key={index}>
      <h4>{findingLabels[item.type]} · {item.predicate}</h4>
      {item.type === "epistemicDisagreement" && <p>认知状态：{states[item.leftState as keyof typeof states] ?? item.leftState} / {states[item.rightState as keyof typeof states] ?? item.rightState}</p>}
      {(item.type === "stateValueConflict" || item.type === "stateSchemaDisagreement") && <><p>属性值：{stateValueText(item.leftValue)} / {stateValueText(item.rightValue)}</p><p>左侧规则：{schemaText(item.leftSchema)}</p><p>右侧规则：{schemaText(item.rightSchema)}</p><small>{item.schemaAuthority === "authorConfirmed" ? "依据作者已确认的规则" : "依据模型声明的属性解释"}，不自动判断哪条主张应保留。</small></>}
      {[{ quote: item.leftEvidence?.quote, revision: item.leftSourceRevisionId }, { quote: item.rightEvidence?.quote, revision: item.rightSourceRevisionId }].map((source, side) => <div key={side}>{source.quote ? <blockquote>{source.quote}</blockquote> : <p>{source.revision ? "此项未提供原文引用。" : "本次假设，没有正文原句。"}</p>}</div>)}
      <details><summary>发现来源定位</summary><dl><div><dt>对象</dt><dd>{item.subjectId}</dd></div>{item.leftSourceRevisionId && <div><dt>左侧正文修订</dt><dd>{item.leftSourceRevisionId}</dd></div>}{item.rightSourceRevisionId && <div><dt>右侧正文修订</dt><dd>{item.rightSourceRevisionId}</dd></div>}{item.type === "epistemicDisagreement" && <div><dt>认知人物</dt><dd>{item.ownerId}</dd></div>}{(item.type === "stateValueConflict" || item.type === "stateSchemaDisagreement") && <>{item.propertyRulesRevisionId && <div><dt>属性规则修订</dt><dd>{item.propertyRulesRevisionId}</dd></div>}{typeof item.leftValue === "object" && <div><dt>左侧关联对象</dt><dd>{item.leftValue.entityId}</dd></div>}{typeof item.rightValue === "object" && <div><dt>右侧关联对象</dt><dd>{item.rightValue.entityId}</dd></div>}</>}</dl></details>
    </article>)}</section>
    <section><h3>分析范围</h3><p>{result.simulation.coverage.projectionComplete ? "当前快照的有限投影已完成；不代表全书影响已检查。" : "当前投影未完整覆盖；可能有未显示的冲突或无法判断的关系。"}</p><p>这份结果不提供全局影响保证。</p></section>
    <details className="saved-analysis-source"><summary>来源与记录</summary><dl><div><dt>假设记录</dt><dd>{result.id}</dd></div><div><dt>原快照</dt><dd>{result.baseSnapshotId}</dd></div></dl></details>
  </>;
}
