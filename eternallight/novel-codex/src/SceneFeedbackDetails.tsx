import type { CommitmentAssessment, SceneAssessmentSummary } from "./types";

const outcomeLabels: Record<CommitmentAssessment["outcome"], string> = {
  advanced: "有所推进", partiallyFulfilled: "部分兑现", fulfilled: "兑现候选",
  contradicted: "与承诺矛盾", unchanged: "尚未推进", insufficientEvidence: "证据不足",
};
const stateLabels: Record<CommitmentAssessment["referenceState"], string> = {
  current: "原规划引用仍有效", sourceChanged: "来源正文已改变",
  revisionSuperseded: "作者已调整承诺版本", authorEstablished: "提案已由作者另行建立",
};

export function SceneFeedbackDetails({ assessment }: { assessment: SceneAssessmentSummary }) {
  return <ul>{assessment.assessment.assessments
    .filter((scene) => scene.verdict !== "realized" || scene.recommendation !== "continue")
    .map((scene) => <li key={scene.sceneId}><b>{scene.sceneId}</b> · {scene.rationale}
      {scene.effectAssessments.flatMap((effect) => effect.commitmentAssessments.map((item) => {
        const id = item.reference.type === "commitment" ? item.reference.commitmentId : item.reference.proposalId;
        return <details key={`${effect.effectId}-${id}`}>
          <summary>承诺判断 · {id} · {outcomeLabels[item.outcome]}（候选分析）</summary>
          <p>{item.rationale}</p><small>{stateLabels[item.referenceState]}</small>
          <small>原引用：{item.reference.type === "commitment" ? item.reference.revisionId : item.reference.reportRevisionId}</small>
          <small>{item.actualEventRefs.length ? `事件证据：${item.actualEventRefs.map((event) => `${event.modelingBatchId} #${event.assertionIndex}`).join("；")}` : "没有可定位的兑现事件证据"}</small>
        </details>;
      }))}
    </li>)}</ul>;
}
