import type { EventClaimReference, EventReferenceTiming, StoryEventInterpretation, NovelCodexApi } from "./types";
import { ConditionEvaluationPanel } from "./ConditionEvaluationPanel";

const roles = { actor: "行动者", recipient: "接收者", affected: "受影响者", location: "地点", instrument: "工具", item: "物品", witness: "见证者", beneficiary: "受益者" };
const timingLabels: Record<EventReferenceTiming, string> = {
  applicableBeforeEvent: "区间在事件前适用", inactiveBeforeEvent: "区间在事件前不适用", startsAtEvent: "起点同刻，顺序未定",
  beforeEvent: "起点早于事件", atEvent: "起点与事件同刻", afterEvent: "起点晚于事件",
  unknownTime: "时间未知", unsupportedConditionKind: "条件类型尚未解释", unsupportedEffectKind: "效果类型尚未解释",
};
const conditionLabels = { supported: "来源支持", refuted: "来源反驳", both: "来源冲突", unknown: "尚不能判定" };

export function StoryEventDetails({ event, api, projectId }: { event?: StoryEventInterpretation; api?: NovelCodexApi; projectId?: string }) {
  if (!event) return null;
  const label = ({ sourceBatchId, assertionIndex }: EventClaimReference) => `${sourceBatchId === null ? "本批次" : `批次 ${sourceBatchId}`} #${assertionIndex + 1}`;
  const refs = (references: EventClaimReference[]) => references.length ? references.map(label).join("、") : "尚未解释";
  return <span className="state-property-details">
    <small>事件解释 · {event.eventType} · {event.eventId}</small>
    <small>{event.participants.map(({ role, entityId }) => `${roles[role]}：${entityId}`).join("；")}</small>
    <small>世界时间：{event.storyTime === null ? "开始未知" : `从 ${event.storyTime}`}；{event.untilExclusive === null ? "结束未定" : `到 ${event.untilExclusive} 前`}</small>
    <small>前提主张：{refs(event.preconditions)}；效果主张：{refs(event.effects)}</small>
    <small>前提时序：{event.temporalAssessment.preconditions.length ? event.temporalAssessment.preconditions.map(({ referenceIndex, timing }) => `${event.preconditions[referenceIndex] ? label(event.preconditions[referenceIndex]) : "引用缺失"} ${timingLabels[timing]}`).join("；") : "尚未解释"}</small>
    <small>效果时序：{event.temporalAssessment.effects.length ? event.temporalAssessment.effects.map(({ referenceIndex, timing }) => `${event.effects[referenceIndex] ? label(event.effects[referenceIndex]) : "引用缺失"} ${timingLabels[timing]}`).join("；") : "尚未解释"}</small>
    <small>仅解释来源绑定的时序，不证明前提真实或因果成立。</small>
    <small>审阅记录中的规则条件：{event.conditionAssessment.status === "noApplicableRule" ? "尚无正式采用规则" : conditionLabels[event.conditionAssessment.result]}；不认证世界真相，不自动确认来源。</small>
    {event.conditionAssessment.coverage && <small>该次解释的证据覆盖：{event.conditionAssessment.coverage.complete ? "解释范围完整" : "存在未覆盖或未解释项"}；{event.conditionAssessment.coverage.sources.sourceProofId ? "已校验索引筛选" : "直接来源校验"}，读取 {event.conditionAssessment.coverage.sources.scannedSources}/{event.conditionAssessment.coverage.sources.relevantSources} 个待检查批次，范围共 {event.conditionAssessment.coverage.sources.totalSources} 批，待建模 {event.conditionAssessment.coverage.sources.pendingSources}；{event.conditionAssessment.coverage.evidenceTruncated ? "引用展示超限" : `未决检查 ${event.conditionAssessment.coverage.unresolvedChecks}`}。</small>}
    {event.conditionAssessment.sourceClaims?.length ? <small>条件证据：{refs(event.conditionAssessment.sourceClaims)}</small> : null}
    <small>编号对应标注来源的主张；确认事件不自动确认关联主张，不执行世界状态变化。</small>
    {api && projectId && <ConditionEvaluationPanel api={api} projectId={projectId} target={event.evaluationTarget} initialJobId={event.conditionAssessment.explanationJobId ?? event.conditionAssessment.evaluationJob?.id} />}
  </span>;
}
