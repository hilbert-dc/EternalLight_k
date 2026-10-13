import type { CreativeWorkspace } from "./types";
import { summarizeWorkOverview } from "./work-overview";

type Props = {
  workspace: CreativeWorkspace | null;
  onContinue: (chapterId: string | null) => void;
  onDiscuss?: () => void;
  blocked: boolean;
};

export function WorkOverview({ workspace, onContinue, onDiscuss, blocked }: Props) {
  const progress = workspace ? summarizeWorkOverview(workspace) : null;
  const canContinue = Boolean(workspace) && !blocked;
  const continueLabel = !progress?.continueChapterNumber ? "开始写作"
    : progress.writtenChapters ? `继续第 ${progress.continueChapterNumber} 章` : `写第 ${progress.continueChapterNumber} 章`;
  return <section className="work-overview" aria-label="作品创作进展">
    <dl className="work-overview-stats">
      <div><dt>已写章节</dt><dd>{progress?.writtenChapters.toLocaleString("zh-CN") ?? "—"}</dd></div>
      <div><dt>当前正文</dt><dd>{progress ? `${progress.characters.toLocaleString("zh-CN")} 字符` : "—"}</dd></div>
      <div><dt>已发布章节</dt><dd>{progress?.publishedChapters.toLocaleString("zh-CN") ?? "—"}</dd></div>
    </dl>
    <p className="work-overview-note">{progress ? "按当前已保存正文统计，包含标点，不含空白和场景分隔线。" : "正在读取作品进展…"}</p>
    {progress?.lastEditedAt && <p className="work-overview-edited">最近编辑：<time dateTime={progress.lastEditedAt}>{new Date(progress.lastEditedAt).toLocaleString("zh-CN")}</time></p>}
    <div className="work-overview-actions">
      <button type="button" className="primary-button" disabled={!canContinue} onClick={() => onContinue(progress?.continueChapterId ?? null)}>{continueLabel}</button>
      {onDiscuss && <button type="button" className="secondary-button" disabled={!canContinue} onClick={onDiscuss}>讨论作品方向</button>}
    </div>
  </section>;
}
