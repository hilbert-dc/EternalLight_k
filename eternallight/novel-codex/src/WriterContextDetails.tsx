import type { WriterContextPreview } from "./types";

export function WriterContextDetails({ preview }: { preview: WriterContextPreview | null }) {
  if (!preview) return <details className="writer-runtime-preview"><summary>创作范围预览</summary><p>正在读取当前版本的创作范围；尚不能据此确认执行依据。</p></details>;
  const position = preview.target.position;
  const plan = preview.plannedEffects[0]?.value.chapters[0];
  const references = preview.relevantText.filter(source => source.content.chapterNumber < position.readingChapter);
  const target = preview.relevantText.find(source => source.content.chapterNumber === position.readingChapter);
  return <details className="writer-runtime-preview"><summary>创作范围预览 · 第 {position.readingChapter} 章</summary>
    <p>这是当前作者侧范围预览，执行时以任务冻结的依据为准；覆盖完整不代表故事理解或文学质量通过。</p>
    <div><strong>{plan?.publicTitle || "无公开标题"}</strong><small>作品 v{preview.target.projectRevision}</small></div>
    <p>{plan?.writerGuidance || "当前范围没有公开写作指引。"}</p>
    <small>公开承诺：{plan?.publicCommitments?.join("；") || "无"} · 视角：{position.povCharacterId || "未指定"} · 故事时间：{position.storyTime ?? "未指定"}</small>
    {target && <small>当前目标稿：第 {target.content.chapterNumber} 章 · {target.authority === "accepted" ? "正式稿" : target.authority === "working" ? "作者选定工作稿" : "候选稿"}{target.workingSelectionRevision != null ? "（已选为后续依据）" : ""}</small>}
    {references.length ? <details><summary>参考前文 · {references.length} 项</summary>{references.map(source => <small key={source.sourceRevisionId}>第 {source.content.chapterNumber} 章 · {source.authority === "accepted" ? "正式稿" : source.authority === "working" ? "作者选定工作稿" : "候选稿"}{source.workingSelectionRevision != null ? "（已选为后续依据）" : ""}</small>)}</details> : <small>当前范围未包含前文正文</small>}
    <small>覆盖：{preview.coverage.complete ? "已覆盖请求范围" : "尚有未覆盖范围"} · 语义完整性尚未认证</small>
    <small>上下文 {preview.budgetsUsed.byteSize} / {preview.budgetsUsed.byteLimit} 字节 · 已带入事实 {preview.coverage.scannedScope.factsIncluded} 条</small>
    {preview.coverage.missingScope.length > 0 && <small>未覆盖项：{preview.coverage.missingScope.join("、")}</small>}
    {preview.coverage.truncationReasons.length > 0 && <small>裁剪原因：{preview.coverage.truncationReasons.join("、")}</small>}
    <details><summary>查看预览来源</summary>{preview.plannedEffects[0] && <small>绑定章纲修订：{preview.plannedEffects[0].value.planRevisionId}</small>}{target && <small>当前目标稿修订：{target.sourceRevisionId}</small>}<small>快照：{preview.snapshotId}</small><small>摘要：{preview.contentHash}</small></details>
  </details>;
}
