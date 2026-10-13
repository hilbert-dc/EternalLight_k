import type { Dispatch, RefObject, SetStateAction } from "react";
import { ChapterPublicationDialog } from "./ChapterPublicationDialog";
import { formatFactValue, selectionAwareReviewChecksPass } from "./creative-product-logic";
import type { DirectionDecisionValue, PlanningTarget, WorkspaceView } from './creative-workspace-types';
import { publicationObjectLabel } from "./publication-object-label";
import { DirectionDecisionEditor, epistemicStateLabel, factKindLabel, manualDecisionKey } from "./PublicationReviewFields";
import { StatePropertyDetails } from "./StatePropertyDetails";
import { StoryEventDetails } from "./StoryEventDetails";
import type { CreativeArtifactKind, CreativeReview, CreativeWorkspace, NovelCodexApi, ProvisionalPlanningImpact, RuntimeState } from "./types";
import { type ChapterPublicationTarget } from "./useChapterPublication";

type Input = {
  view: WorkspaceView;
  currentReview: CreativeReview | undefined;
  publicationOpen: boolean;
  publicationTarget: ChapterPublicationTarget | null;
  editorIdentity: string;
  busy: boolean;
  agentRunning: boolean;
  closePublication: () => void;
  pendingChapterAccept: RefObject<{ scope: string; reviewId: string; revisionId: string; } | null>;
  noConfirmedChanges: boolean;
  setNoConfirmedChanges: Dispatch<SetStateAction<boolean>>;
  publication: { preparing: boolean; prepare: ({ bodyOnlyConfirmed, expectedSource }?: { bodyOnlyConfirmed?: boolean; expectedSource?: { revisionId: string; batchId: string; }; }) => Promise<void>; };
  recoverPublication: () => Promise<void>;
  reviewRef: RefObject<HTMLElement | null>;
  viewKind: CreativeArtifactKind | "draft";
  confirmedFacts: CreativeWorkspace["factProposals"];
  workspaceChapter: CreativeWorkspace["chapters"][number] | undefined;
  reviewFacts: CreativeWorkspace["factProposals"];
  undecidedReviewCount: number;
  selectedReviewFacts: CreativeWorkspace["factProposals"];
  bulkReviewAllowed: boolean;
  decideUndecidedReviewFacts: (decision: "confirmed" | "rejected") => void;
  draftInteractionLocked: boolean;
  markReviewDirty: () => void;
  selectedSceneGroups: string[];
  provisionalImpacts: ProvisionalPlanningImpact[];
  planningTargets: PlanningTarget[];
  factDependencyTargets: Record<string, string[]>;
  factDecisions: Record<string, "rejected" | "confirmed">;
  publicationEntities: unknown[];
  state: RuntimeState;
  api: NovelCodexApi | undefined;
  projectId: string;
  setFactDecisions: Dispatch<SetStateAction<Record<string, "rejected" | "confirmed">>>;
  directionChangeDecisions: Record<string, DirectionDecisionValue>;
  setDirectionChangeDecisions: Dispatch<SetStateAction<Record<string, DirectionDecisionValue>>>;
  setFactDependencyTargets: Dispatch<SetStateAction<Record<string, string[]>>>;
  manualDirectionDecisions: Record<string, DirectionDecisionValue>;
  setManualDirectionDecisions: Dispatch<SetStateAction<Record<string, DirectionDecisionValue>>>;
  reviewProgress: string;
  canAccept: boolean | undefined;
  acceptCurrent: () => Promise<void>;
  returnsDraft: boolean;
};
export function PublicationReview({ view, currentReview, publicationOpen, publicationTarget, editorIdentity, busy, agentRunning, closePublication, pendingChapterAccept, noConfirmedChanges, setNoConfirmedChanges, publication, recoverPublication, reviewRef, viewKind, confirmedFacts, workspaceChapter, reviewFacts, undecidedReviewCount, selectedReviewFacts, bulkReviewAllowed, decideUndecidedReviewFacts, draftInteractionLocked, markReviewDirty, selectedSceneGroups, provisionalImpacts, planningTargets, factDependencyTargets, factDecisions, publicationEntities, state, api, projectId, setFactDecisions, directionChangeDecisions, setDirectionChangeDecisions, setFactDependencyTargets, manualDirectionDecisions, setManualDirectionDecisions, reviewProgress, canAccept, acceptCurrent, returnsDraft }: Input) {
 return <>{(view !== "chapters" && currentReview || view === "chapters" && publicationOpen && publicationTarget?.scope === editorIdentity) && <ChapterPublicationDialog inline={view !== "chapters"} busy={busy || agentRunning} onClose={closePublication}>
        {view === "chapters" && publicationTarget?.knowledge === "manual" && <p className="creative-alert warning">本版本尚未完成故事设定分析；不会使用之前版本的分析结果。</p>}
        {view === "chapters" && !currentReview && !pendingChapterAccept.current && <><label className="no-fact-decision"><input type="checkbox" disabled={busy} checked={noConfirmedChanges} onChange={event => setNoConfirmedChanges(event.target.checked)} /><span>我已检查，本次仅发布正文，不新增故事设定</span></label><button className="primary-button" type="button" disabled={busy || !noConfirmedChanges} onClick={() => void publication.prepare({ bodyOnlyConfirmed: true })}>继续发布前检查</button></>}
        {view === "chapters" && pendingChapterAccept.current?.scope === editorIdentity && <button type="button" className="primary-button" disabled={busy} onClick={() => void recoverPublication()}>重新读取发布结果</button>}
        {currentReview && <section className="review-panel" ref={reviewRef} tabIndex={-1}>
        {view !== "chapters" && <div><span className="creative-kicker">审阅</span><h3>采用前检查</h3></div>}
        {currentReview.checks.map((check) => {
          const selectionAware = viewKind === "draft" && !check.passed && (check.id === "fact-evidence" || check.id === "fact-evidence-anchor");
          const passed = selectionAware
            ? selectionAwareReviewChecksPass([check], confirmedFacts, workspaceChapter?.candidate?.sceneGroups ?? [])
            : check.passed;
          const pending = check.id === "semantic-review" && !passed;
          return <div className={`review-check ${pending ? "pending" : passed ? "passed" : "failed"}`} key={check.id}><span>{pending ? "…" : passed ? "✓" : "!"}</span>{view === "chapters" && check.id === "current-candidate" ? "内容版本与本次发布一致" : check.message}{selectionAware ? "；当前只检查已确认的故事设定" : ""}</div>;
        })}
        {viewKind === "draft" && <p>本次发布整章，包括标题、场景顺序和删除的内容。</p>}
        {view === "chapters" && reviewFacts.length > 0 && <section className="publication-bulk-decisions" aria-label="批量处理未决定建议">
          <p role="status" aria-live="polite" aria-label="建议决定进展">还有 {undecidedReviewCount} 项未决定，{selectedReviewFacts.length - undecidedReviewCount} 项已有决定。</p>
          {undecidedReviewCount > 0 && <div><button type="button" className="secondary-button" disabled={!bulkReviewAllowed} onClick={() => decideUndecidedReviewFacts("confirmed")}>确认未决定的 {undecidedReviewCount} 项</button><button type="button" className="secondary-button" disabled={!bulkReviewAllowed} onClick={() => decideUndecidedReviewFacts("rejected")}>拒绝未决定的 {undecidedReviewCount} 项</button></div>}
          <small>仅处理本次检查中未决定的建议，保留已有单项决定；发布后才更新故事设定。</small>
        </section>}
        {viewKind === "draft" && reviewFacts.length === 0 && <label className="no-fact-decision"><input type="checkbox" disabled={draftInteractionLocked} checked={noConfirmedChanges} onChange={(event) => { setNoConfirmedChanges(event.target.checked); markReviewDirty(); }} /><span><strong>{publicationTarget?.knowledge === "manual" ? "我已检查，本次仅发布正文，不新增故事设定" : "本章没有要确认的故事设定变化"}</strong></span></label>}
        {reviewFacts.map((fact) => {
          const sceneSelected = selectedSceneGroups.includes(fact.sceneGroupId);
          const factImpacts = provisionalImpacts.filter((impact) => impact.factProposalId === fact.id);
          const automaticTargetKeys = new Set(factImpacts.map((impact) => `${impact.targetArtifactKind}:${impact.targetNodeId}`));
          const supplementaryTargets = planningTargets.filter((target) => !automaticTargetKeys.has(`${target.kind}:${target.nodeId}`));
          const selectedManualTargets = planningTargets.filter((target) => (factDependencyTargets[fact.id] ?? []).includes(target.key));
          const confirmed = sceneSelected && factDecisions[fact.id] === "confirmed";
          return <article className={`fact-review-card ${sceneSelected ? "" : "disabled"}`} key={fact.id}>
            <header><div><small>{fact.source === "agent" ? "分析建议" : "作者记录"} · {factKindLabel(fact.factKind)}</small><strong>{fact.predicate}</strong><small>主体：{publicationObjectLabel(fact.subjectId, publicationEntities, state.canon)}{!fact.storyEvent && ` · 值：${formatFactValue(fact.value)}`}</small><StoryEventDetails event={fact.storyEvent} api={api} projectId={projectId} />{fact.stateProperty ? <StatePropertyDetails property={fact.stateProperty} /> : fact.storyEvent ? null : <small>认知状态：{epistemicStateLabel(fact.epistemicState)} · 持有者：{publicationObjectLabel(fact.epistemicOwnerId, publicationEntities, state.canon)}</small>}<p>{fact.evidence}</p><small>证据来自本章正文</small><details><summary>查看来源标识</summary><small>主体：{fact.subjectId || "无"} · 持有者：{fact.epistemicOwnerId || "无"}</small></details></div><select disabled={!sceneSelected || draftInteractionLocked} aria-label={`${fact.predicate} 的确认结论`} value={sceneSelected ? factDecisions[fact.id] ?? "" : "rejected"} onChange={(event) => { setFactDecisions((current) => ({ ...current, [fact.id]: event.target.value as "confirmed" | "rejected" })); markReviewDirty(); }}><option value="">请选择</option><option value="confirmed">确认变化</option><option value="rejected">不采纳</option></select></header>
            {factImpacts.length > 0 && <div className="provisional-impact-list"><strong>采用前影响预览</strong>{factImpacts.map((impact) => { const target = planningTargets.find((item) => item.kind === impact.targetArtifactKind && item.nodeId === impact.targetNodeId); const pathLabel = target?.label ?? impact.targetLabel; return <div className={`provisional-impact ${impact.severity}`} key={impact.id}><div><span>{impact.targetArtifactKind === "outline" ? "大纲" : "章纲"} · {pathLabel}</span>{target?.protected && <small>原安排：{target.currentArrangement}</small>}<small>正文变化：{impact.predicate}；证据：{impact.evidence || "未提供"}</small><small>影响范围：{pathLabel} · {impact.severity === "locked-conflict" ? "受保护，必须先决定" : "将创建待处理规划反馈"} · 自动匹配依赖</small></div>{confirmed && impact.severity === "locked-conflict" && <DirectionDecisionEditor disabled={draftInteractionLocked} value={directionChangeDecisions[impact.id]} onChange={(value) => { setDirectionChangeDecisions((current) => ({ ...current, [impact.id]: value })); markReviewDirty(); }} />}</div>; })}</div>}
            {supplementaryTargets.length > 0 && <fieldset className="manual-fact-link" disabled={!confirmed || draftInteractionLocked}><legend>补充关联遗漏的正式规划（可多选）</legend>{supplementaryTargets.map((target) => <label key={target.key}><input type="checkbox" checked={(factDependencyTargets[fact.id] ?? []).includes(target.key)} onChange={(event) => { setFactDependencyTargets((current) => ({ ...current, [fact.id]: event.target.checked ? [...new Set([...(current[fact.id] ?? []), target.key])] : (current[fact.id] ?? []).filter((key) => key !== target.key) })); markReviewDirty(); }} /><span>{target.kind === "outline" ? "大纲" : "章纲"} · {target.label} · {target.protectionLabel}</span></label>)}</fieldset>}
            {confirmed && selectedManualTargets.filter((target) => target.protected).map((target) => <div className="provisional-impact locked-conflict" key={target.key}><div><span>{target.label}</span><small>原安排：{target.currentArrangement}</small><small>正文变化：{fact.predicate}；证据：{fact.evidence || "未提供"}</small><small>影响范围：{target.label} · {target.protectionLabel}，必须先决定</small></div><DirectionDecisionEditor disabled={draftInteractionLocked} value={manualDirectionDecisions[manualDecisionKey(fact.id, target.key)]} onChange={(value) => { setManualDirectionDecisions((current) => ({ ...current, [manualDecisionKey(fact.id, target.key)]: value })); markReviewDirty(); }} /></div>)}
          </article>;
        })}
        <p role="status" aria-live="polite" aria-label={view === "chapters" ? "发布检查进展" : "审阅进展"}>{reviewProgress}</p>
        <button className="primary-button" disabled={busy || draftInteractionLocked || !canAccept || pendingChapterAccept.current?.scope === editorIdentity} onClick={() => void acceptCurrent()}>{view === "chapters" ? returnsDraft ? "返回编辑" : "确认发布章节" : "确认采用这份规划"}</button>
      </section>}</ChapterPublicationDialog>}</>;
}
