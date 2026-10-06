import { useEffect, useState } from "react";
import type { CreativeWorkspace, NovelCodexApi, WriterContextPreview } from "./types";
import { WriterContextDetails } from "./WriterContextDetails";

type Binding = NonNullable<CreativeWorkspace["chapterPlanBindings"]>[number];
type PreviewRead = { identity: string; status: "loading" } | { identity: string; status: "error"; message: string } | { identity: string; status: "current"; value: WriterContextPreview };

export function useWriterPreview({ api, projectId, revision, chapterNumber, sceneOrder, binding, identity }: {
  api?: NovelCodexApi; projectId: string; revision: number | null; chapterNumber: number | null;
  sceneOrder: number; binding: Binding | undefined; identity: string | null;
}) {
  const [read, setRead] = useState<PreviewRead | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    if (!api || identity === null || revision === null || chapterNumber === null) return;
    let cancelled = false;
    const pov = binding?.writerProjection.povCharacterId ?? null;
    const storyTime = binding?.writerProjection.storyTime ?? null;
    setRead({ identity, status: "loading" });
    void api.runtime.getWriterPreview(projectId, revision, chapterNumber, sceneOrder, pov, storyTime).then(preview => {
      if (preview.purpose !== "writer" || preview.taskId !== null || preview.target.projectId !== projectId
        || preview.target.projectRevision !== revision || preview.target.position.readingChapter !== chapterNumber
        || preview.target.position.sceneOrder !== sceneOrder || preview.target.position.povCharacterId !== pov
        || preview.target.position.storyTime !== storyTime) throw new Error("创作范围预览与当前作品或章节不匹配");
      const plan = preview.plannedEffects[0]?.value;
      if (binding ? plan?.planRevisionId !== binding.plan_revision_id || plan.chapters[0]?.chapterPlanId !== binding.chapter_plan_id : Boolean(plan)) throw new Error("创作范围预览与当前章纲绑定不匹配");
      if (!cancelled) setRead({ identity, status: "current", value: preview });
    }).catch(reason => {
      if (!cancelled) setRead({ identity, status: "error", message: reason instanceof Error ? reason.message : "创作依据读取失败" });
    });
    return () => { cancelled = true; };
  }, [api, identity, retry]);
  return { read: identity === null ? null : read?.identity === identity ? read : { identity, status: "loading" } as PreviewRead,
    retry: () => setRetry(value => value + 1) };
}

export function WriterPreviewPanel({ read, retry, dirty, activeSceneOrder, scope, onScopeChange }: {
  read: PreviewRead | null; retry: () => void; dirty: boolean; activeSceneOrder: number | null;
  scope: "chapter" | "scene"; onScopeChange: (scope: "chapter" | "scene") => void;
}) {
  return <section className="writer-preview-state" aria-label="创作范围预览状态">
    <div className="writer-preview-range"><span>当前编辑位置：{activeSceneOrder === null ? "尚未选择场景" : `场景 ${activeSceneOrder}`}</span>
      <button type="button" className="text-button" aria-pressed={scope === "chapter"} onClick={() => onScopeChange("chapter")}>全章预览</button>
      <button type="button" className="text-button" disabled={activeSceneOrder === null} aria-pressed={scope === "scene"} onClick={() => onScopeChange("scene")}>当前场景预览</button>
    </div>
    <p>请求范围：{scope === "chapter" ? "完整本章" : `截至场景 ${activeSceneOrder ?? "未选择"}`}。这是作者侧预览，不是任务已经冻结的执行依据。</p>
    {!read || read.status === "loading" ? <p role="status">正在读取当前范围预览…</p>
      : read.status === "error" ? <p role="alert">预览读取失败：{read.message} <button type="button" className="text-button" onClick={retry}>重试读取预览</button></p>
        : <><p role="status">{dirty ? "预览已过期：工作稿有未保存修改，当前展示仅对应已保存来源。" : "预览已更新 · 当前已保存来源"}</p><WriterContextDetails preview={read.value} /></>}
  </section>;
}
