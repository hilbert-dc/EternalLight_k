import { useEffect, useMemo, useRef, useState } from "react";
import type { CreativeWorkspace, HistoricalKnowledgeState, HypothesisAssumption, HypothesisResult, ImpactAnalysisResult, NovelCodexApi } from "./types";
import "./analysis-entry.css";

interface Props {
  api: NovelCodexApi | null;
  projectId: string;
  workspace: CreativeWorkspace | null;
  chapterId: string;
  dirty: boolean;
  refreshToken: number;
  onDiscuss?: (prompt: string) => void;
  agentRunning?: boolean;
}

function chapterLabel(workspace: CreativeWorkspace | null, objectId: string): string {
  const chapter = workspace?.chapters.find((item) => `draft-${item.id}` === objectId);
  return chapter ? `第 ${chapter.number} 章 · ${chapter.title}` : objectId;
}

function batchStatusLabel(status: string): string {
  return ({ available: "已发布", missingPublishedBatch: "尚无已发布批次", pendingPublication: "仍待发布", ambiguousPublishedBatches: "存在多个批次，需先确定来源" } as Record<string, string>)[status] ?? status;
}

function attitudeLabel(state: string): string {
  return ({ known: "已知", suspected: "怀疑", false: "误信" } as Record<string, string>)[state] ?? state;
}

function suppressionKey(entry: HistoricalKnowledgeState["entries"][number]): string {
  return JSON.stringify([entry.modelingBatchId, entry.sourceRevisionId, entry.assertionIndex]);
}

export function ImpactAnalysisPanel({ api, projectId, workspace, chapterId, dirty, refreshToken, onDiscuss, agentRunning = false }: Props) {
  const chapter = workspace?.chapters.find((item) => item.id === chapterId);
  const revision = chapter?.candidate ?? chapter?.accepted;
  const [entityId, setEntityId] = useState("");
  const [compareRevisions, setCompareRevisions] = useState(true);
  const [report, setReport] = useState<ImpactAnalysisResult | null>(null);
  const [historicalKnowledge, setHistoricalKnowledge] = useState<HistoricalKnowledgeState | null>(null);
  const [knowledgeError, setKnowledgeError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hypothesisPredicate, setHypothesisPredicate] = useState("");
  const [hypothesisValue, setHypothesisValue] = useState("false");
  const [hypothesisStoryTime, setHypothesisStoryTime] = useState("");
  const [hypothesis, setHypothesis] = useState<HypothesisResult | null>(null);
  const [hypothesisBusy, setHypothesisBusy] = useState(false);
  const [hypothesisError, setHypothesisError] = useState<string | null>(null);
  const [suppressedKeys, setSuppressedKeys] = useState<string[]>([]);
  const requestSerial = useRef(0);
  const entities = useMemo(() => {
    const choices = new Map<string, string>();
    for (const batch of workspace?.modeling ?? []) {
      if (batch.status !== "ready") continue;
      for (const raw of batch.entities) {
        if (!raw || typeof raw !== "object") continue;
        const entity = raw as { id?: unknown; displayName?: unknown };
        if (typeof entity.id === "string" && entity.id.length <= 160) {
          choices.set(entity.id, typeof entity.displayName === "string" ? entity.displayName : entity.id);
        }
      }
    }
    const sorted = [...choices].sort((left, right) => left[1].localeCompare(right[1], "zh-CN"));
    return { items: sorted.slice(0, 100), truncated: sorted.length > 100 };
  }, [workspace?.modeling]);
  const suppressibleEntries = useMemo(() => {
    if (!historicalKnowledge || !report || !hypothesisPredicate.trim()) return [];
    return historicalKnowledge.entries.filter((entry) =>
      entry.canonicalSubjectId === report.canonicalEntityId
      && entry.assertion.kind === "assertion"
      && entry.assertion.predicate === hypothesisPredicate.trim()
      && typeof entry.assertion.value === "boolean"
      && typeof entry.modelingBatchId === "string"
      && typeof entry.sourceRevisionId === "string"
      && Number.isSafeInteger(entry.assertionIndex)
    );
  }, [historicalKnowledge, report, hypothesisPredicate]);
  useEffect(() => {
    requestSerial.current += 1;
    setReport(null);
    setHistoricalKnowledge(null);
    setKnowledgeError(null);
    setError(null);
    setLoading(false);
    setHypothesis(null);
    setHypothesisError(null);
    setSuppressedKeys([]);
  }, [projectId, chapterId, revision?.id, entityId, compareRevisions, dirty, refreshToken]);

  async function analyze() {
    if (!api || !revision || dirty || !entityId.trim() || loading) return;
    const serial = ++requestSerial.current;
    setLoading(true);
    setError(null);
    setReport(null);
    setHistoricalKnowledge(null);
    setKnowledgeError(null);
    setSuppressedKeys([]);
    try {
      const result = await api.runtime.getImpactAnalysis(projectId, {
        entityId: entityId.trim(),
        fromRevisionId: compareRevisions ? revision.parentRevisionId : null,
        toRevisionId: compareRevisions && revision.parentRevisionId ? revision.id : null,
      });
      if (requestSerial.current !== serial) return;
      setReport(result);
      try {
        const knowledge = await api.runtime.getHistoricalKnowledgeState(projectId, result.baseSnapshotId);
        if (requestSerial.current === serial) setHistoricalKnowledge(knowledge);
      } catch (reason) {
        if (requestSerial.current === serial) setKnowledgeError(reason instanceof Error ? reason.message : "历史来源读取失败");
      }
    } catch (reason) {
      if (requestSerial.current === serial) setError(reason instanceof Error ? reason.message : "影响分析失败");
    } finally {
      if (requestSerial.current === serial) setLoading(false);
    }
  }

  async function checkHypothesis() {
    if (!api || !report || !historicalKnowledge || dirty || !entityId.trim() || !hypothesisPredicate.trim() || hypothesisBusy) return;
    const serial = ++requestSerial.current;
    setHypothesisBusy(true);
    setHypothesisError(null);
    setHypothesis(null);
    try {
      const selected: HypothesisAssumption[] = suppressibleEntries
        .filter((entry) => suppressedKeys.includes(suppressionKey(entry)))
        .map((entry) => ({ kind: "suppressClaim", modelingBatchId: entry.modelingBatchId, sourceRevisionId: entry.sourceRevisionId, assertionIndex: entry.assertionIndex }));
      const created = await api.runtime.createHypothesis(projectId, {
        baseSnapshotId: report.baseSnapshotId,
        intent: `假设 ${entityId.trim()} 的 ${hypothesisPredicate.trim()} 为 ${hypothesisValue === "true" ? "是" : "否"}`,
        assumptions: [...selected, { kind: "assumeClaim", subjectId: entityId.trim(), predicate: hypothesisPredicate.trim(), value: hypothesisValue === "true", storyTime: hypothesisStoryTime.trim() ? Number(hypothesisStoryTime) : null }],
        idempotencyKey: crypto.randomUUID(),
      });
      const result = await api.runtime.getHypothesis(projectId, created.id);
      if (requestSerial.current === serial) setHypothesis(result);
    } catch (reason) {
      if (requestSerial.current === serial) setHypothesisError(reason instanceof Error ? reason.message : "假设检查失败");
    } finally {
      if (requestSerial.current === serial) setHypothesisBusy(false);
    }
  }

  return <section className="impact-analysis-panel" aria-label="故事改动影响">
    {onDiscuss && <div className="impact-analysis-controls"><button type="button" className="secondary-button" disabled={!revision || dirty || agentRunning} onClick={() => {
      if (!revision || dirty || agentRunning || workspace?.project.id !== projectId) return;
      onDiscuss(`请先核对第 ${chapter?.number} 章《${revision.title}》当前已保存正文，再检查本次修改可能影响的人物、关系与后文。若有上一版，请对照变化；没有上一版请说明。列出正文依据和未覆盖范围，只分析，不修改或确认设定。`);
    }}>在对话中检查本章改动影响</button><small>只准备讨论；发送后按正文证据检查，不自动改写。</small></div>}
    {dirty && <p className="impact-analysis-warning">请先保存当前编辑；分析只读取已保存的修订。</p>}
    <details><summary>手动检查与假设模拟</summary>
    <p>按实体查看有证据的关系路径与正文提及。这是只读候选，不会自动修改章节或设定。</p>
    <div className="impact-analysis-controls">
      <label><span>人物、事件或地点</span><input aria-label="影响分析实体" list={`impact-entities-${chapterId}`} value={entityId} maxLength={160} onChange={(event) => setEntityId(event.target.value)} placeholder="选择或输入实体 ID" /></label>
      <datalist id={`impact-entities-${chapterId}`}>{entities.items.map(([id, name]) => <option key={id} value={id} label={name} />)}</datalist>
      <label className="impact-analysis-check"><input type="checkbox" checked={compareRevisions} disabled={!revision?.parentRevisionId} onChange={(event) => setCompareRevisions(event.target.checked)} />对照本章上一版</label>
      <button type="button" className="secondary-button" disabled={!api || !revision || dirty || !entityId.trim() || loading} onClick={() => void analyze()}>{loading ? "检查中…" : "检查影响"}</button>
    </div>
    {entities.truncated && <p>实体建议仅显示前 100 项；仍可输入完整 ID 检查其他实体。</p>}
    {error && <p role="alert" className="impact-analysis-warning">{error}</p>}
    {report && <div className="impact-analysis-results" aria-label="影响分析结果">
      <p className="impact-analysis-warning">未证明全书影响已找全；关系和字面命中仅表示需要复核，不表示必须改写。分析基于快照 {report.baseSnapshotId}。</p>
      <p>依赖传播规则：{report.dependencyRuleVersion} · 仅沿显式依赖复核</p>
      <div className="impact-analysis-coverage">
        <span>关系路径：{report.coverage.explicitGraphComplete ? "当前范围已遍历" : "范围或索引不完整"}</span>
        <span>字面召回：{report.coverage.literalSearch.literalNameSearchComplete ? "已检索已知名称" : "名称或索引覆盖不完整"}</span>
        {report.coverage.graphCoverage.pendingSourceCount ? <span>待建模来源 {report.coverage.graphCoverage.pendingSourceCount} 个</span> : null}
      </div>
      {report.revisionDelta && <section>
        <h4>本章修订</h4>
        <p>{report.revisionDelta.bodyChanged ? "正文有变化" : "正文未变化"} · 场景变化 {report.revisionDelta.sceneChanges.length} 项</p>
        {report.revisionDelta.modelCandidateDelta.status === "unavailable"
          ? <p>建模主张暂不可比较：上一版{batchStatusLabel(report.revisionDelta.modelCandidateDelta.coverage.from)}，当前版{batchStatusLabel(report.revisionDelta.modelCandidateDelta.coverage.to)}。不能据此认定事实被删除。</p>
          : <><p>建模候选变化 {report.revisionDelta.modelCandidateDelta.coverage.totalCandidateChanges ?? 0} 项{report.revisionDelta.modelCandidateDelta.coverage.truncated ? "（仅展示部分）" : ""}；证据重锚定 {report.revisionDelta.modelCandidateDelta.coverage.evidenceReanchoredClaimCount ?? 0} 项。</p>
            <ol>{report.revisionDelta.modelCandidateDelta.candidateChanges.map((change, index) => <li key={`${change.modelingBatchId}-${index}`}><strong>{change.change === "newlyExtracted" ? "新抽取" : "不再抽取"}</strong> {change.claim.subjectId ?? "未知主体"} · {change.claim.predicate ?? "未知主张"} · {JSON.stringify(change.claim.value)}<blockquote>{change.evidence.quote}</blockquote><small>来源 {change.sourceRevisionId}</small></li>)}</ol>
          </>}
      </section>}
      <section><h4>关系路径 · {report.findings.length}</h4>
        {report.findings.length === 0 ? <p>当前显式关系图中没有找到路径；这不等于没有影响。</p> : <ol>{report.findings.map((finding, index) => <li key={`${finding.affectedEntityId}-${index}`}><strong>{finding.affectedEntityId}</strong> · {finding.hopCount} 跳{finding.evidencePath.map((edge, step) => <div key={`${edge.sourceRevisionId}-${step}`} className="impact-analysis-evidence"><span>{edge.fromId} → {edge.toId} · {edge.relationType}</span><blockquote>{edge.evidence.quote}</blockquote><small>{chapterLabel(workspace, edge.sourceObjectId)} · {edge.sourceRevisionId}</small></div>)}</li>)}</ol>}
      </section>
      <section><h4>正文提及 · {report.literalTextCandidates.length}</h4>
        {report.literalTextCandidates.length === 0 ? <p>未召回已知名称的字面提及；代词及隐含语义仍未覆盖。</p> : <ol>{report.literalTextCandidates.map((hit, index) => <li key={`${hit.sourceRevisionId}-${hit.sourcePath}-${index}`}><strong>{chapterLabel(workspace, hit.sourceObjectId)}</strong> · {hit.sourcePath}<blockquote>{hit.evidence.quote}</blockquote><small>{hit.sourceRevisionId}</small></li>)}</ol>}
      </section>
      <section>
        <h4>固定快照假设模拟</h4>
        <p>这是独立的手动模拟，普通对话分析不会自动执行此模拟。在固定快照上选择要临时排除的证据，再加入真假假设；其他独立来源仍保留。不生成剧情、不修改正文或正式设定，只有故事时间一致时才能判断时间冲突。</p>
        <div className="impact-analysis-controls">
          <label><span>属性 ID</span><input aria-label="假设属性" value={hypothesisPredicate} maxLength={160} onChange={(event) => { setHypothesisPredicate(event.target.value); setSuppressedKeys([]); setHypothesis(null); }} placeholder="例如 alive" /></label>
          <label><span>假设值</span><select aria-label="假设值" value={hypothesisValue} onChange={(event) => { setHypothesisValue(event.target.value); setHypothesis(null); }}><option value="true">是</option><option value="false">否</option></select></label>
          <label><span>故事时间（可选）</span><input aria-label="假设故事时间" type="number" step="1" value={hypothesisStoryTime} onChange={(event) => { setHypothesisStoryTime(event.target.value); setHypothesis(null); }} placeholder="与来源时间一致" /></label>
          <button type="button" className="secondary-button" disabled={!api || !historicalKnowledge || dirty || !hypothesisPredicate.trim() || hypothesisBusy || (hypothesisStoryTime.trim() !== "" && !Number.isSafeInteger(Number(hypothesisStoryTime)))} onClick={() => void checkHypothesis()}>{hypothesisBusy ? "检查中…" : "检查假设"}</button>
        </div>
        {knowledgeError && <p role="alert" className="impact-analysis-warning">无法读取快照来源：{knowledgeError}</p>}
        {historicalKnowledge && !historicalKnowledge.coverage.complete && <p className="impact-analysis-warning">快照知识投影不完整，以下仅列已读取来源；未列出的证据仍可能支持原主张。</p>}
        {historicalKnowledge && hypothesisPredicate.trim() && suppressibleEntries.length === 0 && <p>当前投影中没有此实体和属性的可替换真假证据；只能追加假设，不能推断原主张不存在。</p>}
        {suppressibleEntries.length > 0 && <div className="impact-analysis-sources" aria-label="可替换的来源证据">
          <p>选中要在这次模拟中排除的来源；不选则只追加假设。</p>
          {suppressibleEntries.slice(0, 30).map((entry) => {
            const key = suppressionKey(entry);
            return <label key={key}><input type="checkbox" checked={suppressedKeys.includes(key)} disabled={!suppressedKeys.includes(key) && suppressedKeys.length >= 24} onChange={(event) => { setSuppressedKeys((current) => event.target.checked ? [...current, key] : current.filter((item) => item !== key)); setHypothesis(null); }} />
              <span>{entry.assertion.value ? "是" : "否"} · {entry.assertion.evidence?.quote ?? "无摘录"} <small>{chapterLabel(workspace, entry.sourceObjectId)} · {entry.sourceRevisionId} · {entry.confirmationState === "confirmed" ? "已确认" : "候选"}</small></span>
            </label>;
          })}
          {suppressibleEntries.length > 30 && <p>仅展示前 30 条来源；其余证据仍保留在模拟中。</p>}
        </div>}
        {hypothesisError && <p role="alert" className="impact-analysis-warning">{hypothesisError}</p>}
        {hypothesis && <div aria-label="假设检查结果">
          <p>历史基线 {hypothesis.baseline.entryCount} 条主张；模拟后 {hypothesis.simulation.entries.length} 条。{hypothesis.simulation.coverage.projectionComplete ? "当前投影完整" : "当前投影不完整"}，无法据此认定全书影响已找全。</p>
          {hypothesis.simulation.findings.length === 0 ? <p>未检测到候选证据间的真假冲突或认知分歧；这不代表假设成立。</p> : <ol>{hypothesis.simulation.findings.map((finding, index) => <li key={`${finding.subjectId}-${finding.predicate}-${index}`}>
            {finding.type === "epistemicDisagreement" ? <>
              {finding.ownerId}：认知证据存在分歧（{attitudeLabel(finding.leftState)} / {attitudeLabel(finding.rightState)}）
              <small>命题 {finding.propositionRef.propositionId} · 来源批次 {finding.propositionRef.batchId}；不自动判定哪方正确</small>
            </> : <>{finding.subjectId} · {finding.predicate}：与已记录主张冲突</>}
            <div className="impact-analysis-evidence"><blockquote>{finding.leftEvidence?.quote ?? "假设（无正文证据）"}</blockquote><small>{finding.leftSourceRevisionId ?? "模拟假设"}</small></div>
            <div className="impact-analysis-evidence"><blockquote>{finding.rightEvidence?.quote ?? "假设（无正文证据）"}</blockquote><small>{finding.rightSourceRevisionId ?? "模拟假设"}</small></div>
          </li>)}</ol>}
        </div>}
      </section>
    </div>}
    </details>
  </section>;
}
