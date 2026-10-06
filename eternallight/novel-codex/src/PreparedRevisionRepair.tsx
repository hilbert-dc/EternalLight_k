import type { RevisionRepairPreparation } from "./types";

export function PreparedRevisionRepair({ preparation, checking, error, onCancel }: {
  preparation: RevisionRepairPreparation; checking: boolean; error: string | null; onCancel: () => void;
}) {
  return <section className="revision-outcome-decision" aria-label="已准备改稿讨论" aria-busy={checking}>
    <strong>讨论下一次改稿 · 已准备，尚未发送</strong>
    <p>新讨论独立确认预算，写作需候选审批。</p>
    <small>章节 {preparation.chapterId} · 写作尝试 {preparation.source.writerTaskId} · 判断 {preparation.source.assessmentRevisionId}
      {` · 决定 ${preparation.source.decisionId} · 正文 ${preparation.draftRevisionId} · 知识批次 ${preparation.knowledgeBatchId ?? "未使用（直接对照正文）"}`}</small>
    {preparation.note && <p>决策备注（仅供参考，可自行写入或调整下方消息）：{preparation.note}</p>}
    {checking && <p role="status">正在核对改稿讨论来源…</p>}
    {error && <p role="alert">{error}</p>}
    <button className="secondary-button" type="button" onClick={onCancel}>取消改稿讨论来源</button>
  </section>;
}

export function FailedRevisionRepair({ preparation, onStartOrdinaryDiscussion }: {
  preparation: RevisionRepairPreparation; onStartOrdinaryDiscussion: () => void;
}) {
  return <section className="revision-outcome-decision" aria-label="本轮改稿讨论来源">
    <strong>本轮改稿讨论未完成</strong>
    <p>请从任务恢复，或重新选择修改结果并发送。重新讨论需独立确认预算。</p>
    <small>写作尝试 {preparation.source.writerTaskId} · 判断 {preparation.source.assessmentRevisionId}
      {` · 决定 ${preparation.source.decisionId} · 正文 ${preparation.draftRevisionId} · 知识批次 ${preparation.knowledgeBatchId ?? "未使用（直接对照正文）"}`}</small>
    <p>“重试对话”不能将本轮改稿请求作为普通聊天发送。</p>
    <button className="secondary-button" type="button" onClick={onStartOrdinaryDiscussion}>取消改稿来源，开始普通讨论</button>
  </section>;
}
