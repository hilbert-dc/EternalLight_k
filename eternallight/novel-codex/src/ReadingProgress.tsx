import type { AuthorReadingProgress } from "./types";

const traversalLabels = {
  reading: "原文阅读进度",
  acknowledgedEnd: "当前阅读范围已遍历",
  budgetExhausted: "阅读检查点预算已用完",
};

export function ReadingProgress({ progress }: { progress: AuthorReadingProgress | null }) {
  if (progress === null) return null;
  return <div className="work-reading-progress" data-source-state={progress.status}>
    {progress.status === "current" ? <>
      <strong>{traversalLabels[progress.traversal]}</strong>
      <span>已确认 {progress.pages.toLocaleString()} 页原文 · {progress.acknowledgedCharacters.toLocaleString()} 字符</span>
      <p>{progress.traversal === "acknowledgedEnd" ? "仅表示当前阅读范围已到末尾，不代表全书或内容理解完成。"
        : progress.traversal === "budgetExhausted" ? "当前步骤不能再保存阅读检查点；继续任务不会重置此预算。"
        : "记录已确认的原文页面，不表示任务已完成。"}理解完整性尚未验证。</p>
    </> : <>
      <strong>{progress.status === "stale" ? "原文来源已变化" : "阅读记录无法校验"}</strong>
      <p>{progress.status === "stale" ? "旧阅读进度不再有效，请按当前内容重新发起任务。" : "不显示无法验证的进度；任务控制仍按当前状态处理。"}理解完整性尚未验证。</p>
    </>}
  </div>;
}
