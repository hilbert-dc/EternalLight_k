import type { CreativeWorkspace, OutlineNode, ChapterPlanPayload } from "./types";
import "./global-story-design.css";

type Artifact = CreativeWorkspace["artifacts"]["outline"];
function basis(artifact: Artifact) { return artifact.working ?? artifact.accepted ?? artifact.candidate; }
function status(artifact: Artifact) {
  const revision = basis(artifact);
  return !revision ? "尚未保存" : revision.id === artifact.accepted?.id ? "已采用" : revision.id === artifact.working?.id ? "当前写作依据 · 尚未采用" : "已保存建议 · 尚未采用";
}

export function GlobalStoryDesign({ workspace, blocked, dirty, onOpen }: {
  workspace: CreativeWorkspace; blocked: boolean; dirty: boolean;
  onOpen: (kind: "outline" | "plan") => void;
}) {
  const outline = basis(workspace.artifacts.outline), plan = basis(workspace.artifacts.plan);
  const nodes = (outline?.payload?.nodes ?? []) as OutlineNode[];
  const chapters = (plan?.payload?.chapters ?? []) as ChapterPlanPayload[];
  const global = nodes.filter(node => node.kind === "book" || node.kind === "volume");
  const disabled = blocked || dirty;
  const revisionStatus = (artifact: Artifact) => <p className="global-design-source">{status(artifact)}{artifact.locked && " · 已锁定"}{artifact.candidate && artifact.candidate.id !== basis(artifact)?.id && "；另有未作为当前依据的新建议"}</p>;
  return <section className="global-story-design" aria-label="全局故事设计">
    {workspace.planLineageStatus === "stale" && <p role="status">大纲来源已变化，请在近章规划中核对和更新。</p>}
    <details><summary>全局故事设计</summary>
    {dirty && <p role="status">此处是已保存设计。请先保存当前修改，再切换规划。</p>}
    <div className="global-design-grid">
      <article data-discussion-revision={outline?.id ?? ""}><h3>全书主线与阶段</h3>{revisionStatus(workspace.artifacts.outline)}
        {global.length ? <ol>{global.slice(0, 8).map((node, i) => <li key={node.id ?? node.clientKey ?? i}><strong>{node.title}</strong><p>{node.purpose}</p>{node.summary && <p>{node.summary}</p>}</li>)}</ol> : <p>尚未记录全书主线或阶段结构。可先形成足以指导下一步的安排。</p>}
        {global.length > 8 && <p>共 {global.length} 个阶段，此处展示前 8 个。</p>}
        <button type="button" className="text-button" disabled={disabled} onClick={() => onOpen("outline")}>查看和编辑全书设计</button>
      </article>
      <article data-discussion-revision={plan?.id ?? ""}><h3>近期创作安排</h3>{revisionStatus(workspace.artifacts.plan)}
        {chapters.length ? <ol>{[...chapters].sort((a,b) => a.chapterNumber-b.chapterNumber).slice(0, 6).map((chapter,i) => <li key={chapter.chapterPlanId ?? chapter.clientKey ?? i}><strong>第 {chapter.chapterNumber} 章 · {chapter.publicTitle || chapter.title || "标题待定"}</strong><p>{chapter.goal || "本章目标尚未说明"}</p>{chapter.conflict && <p>冲突：{chapter.conflict}</p>}</li>)}</ol> : <p>尚未保存近期章节安排。</p>}
        {chapters.length > 6 && <p>共 {chapters.length} 章，此处展示前 6 章。</p>}
        <button type="button" className="text-button" disabled={disabled} onClick={() => onOpen("plan")}>查看和编辑近章规划</button>
      </article>
    </div>
    </details>
  </section>;
}
