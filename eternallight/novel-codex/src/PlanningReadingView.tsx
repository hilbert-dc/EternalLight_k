import type { ChapterPlanPayload, CreativeWorkspace, OutlineNode } from "./types";
import { OutlineReadingTree } from "./OutlineReadingTree";
import { outlineLayerCopy } from "./outline-layer-copy";

import "./planning-reading.css";

type Props = {
  view: "outline" | "plans";
  artifact: CreativeWorkspace["artifacts"]["outline"] | null;
  dirty: boolean;
  outlineNodeId?: string | null;
  onOpenVersions?: () => void;
  onOpenOutlineNode?: (id: string) => void;
  prepareDirtyDiscussion?: boolean;
  blocked: boolean;
  onDiscuss?: (prompt: string) => void;
};

export function PlanningReadingView({ view, artifact, outlineNodeId = null, onOpenOutlineNode, onOpenVersions, dirty, prepareDirtyDiscussion = false, blocked, onDiscuss }: Props) {
  const saved = artifact?.candidate ?? artifact?.accepted;
  const working = artifact?.working;
  const name = view === "outline" ? outlineNodeId ? "本卷卷纲" : "全书大纲" : "近章规划";
  const nodes = view === "outline" && Array.isArray(saved?.payload.nodes) ? saved.payload.nodes as OutlineNode[] : [];
  const chapters = view === "plans" && Array.isArray(saved?.payload.chapters) ? saved.payload.chapters as ChapterPlanPayload[] : [];
  const hasContent = view === "outline" ? nodes.length > 0 : chapters.length > 0;
  const nodeScope = outlineNodeId ? `大纲修订 ${saved?.id}，卷节点 ${outlineNodeId}。请只讨论这个节点及其下级，不改其他卷。` : "";
  const storyScope = view === "outline" ? outlineLayerCopy[outlineNodeId ? "volume" : "book"].discussion : "";
  const discussionDisabled = blocked || dirty && !prepareDirtyDiscussion;
  const discussionPrompt = saved || dirty
    ? `请讨论当前已保存的${name}可以怎样调整，先核对来源并说明建议；先不要改写或采用。`
    : `请和我一起起草${name}。已有明确目标时据此规划，缺少关键目标时先问我；不要自动采用。`;
  const checkPrompt = `请只读检查当前已保存的${name}，说明有依据的问题、可保留的安排和未覆盖的范围。不要修改或采用规划。`;

  return <section className="planning-reading" aria-label={`${name}摘要`} data-discussion-revision={saved?.id ?? ""}>
    {view === "outline" && <p className="planning-reading-hint">{outlineNodeId ? "看这一卷怎样推动主线，以及结束时人物与局势有什么变化。" : "看整部故事怎样开始、推进并走向结局；各卷的阶段事件在卷纲中展开。"}</p>}
    <div className="planning-reading-status" role="status">
      <strong>{saved ? saved.id === artifact?.accepted?.id ? "已采用的规划" : "已保存的规划建议 · 尚未采用" : "尚未开始规划"}</strong>
      {artifact?.locked && <span>已锁定</span>}
    </div>
    {working && working.id !== saved?.id && <p className="planning-reading-basis">后续写作仍参考另一份已保存规划。{onOpenVersions && <button type="button" className="text-button" onClick={onOpenVersions}>查看依据</button>}</p>}
    {dirty && <p className="planning-reading-hint">此处展示上次保存的内容；手动编辑中有未保存修改。{prepareDirtyDiscussion ? "发送时先保存规划，成功后再讨论。" : "请先保存，再按已保存内容讨论。"}</p>}
    {onDiscuss && <div className="planning-reading-actions">
      <button type="button" className="primary-button" disabled={discussionDisabled} onClick={() => onDiscuss(nodeScope + storyScope + discussionPrompt)}>{saved || dirty ? "讨论调整" : "开始规划"}</button>
      <button type="button" className="text-button" disabled={blocked || dirty || !saved} onClick={() => onDiscuss(nodeScope + storyScope + checkPrompt)}>只读检查</button>
      {view === "plans" && <button type="button" className="secondary-button" disabled={discussionDisabled || !saved && !dirty} onClick={() => onDiscuss("请根据当前已保存的近章规划讨论后续写作安排。先核对规划、已有正文与来源，说明哪些章节已经可以写、哪些还需我决定；不要自动采用规划或创建章节。")}>讨论后续写作</button>}
    </div>}
    {!hasContent && <p className="planning-reading-empty">{saved ? "这份已保存规划还没有可展示的安排。" : view === "outline" ? "可以先聊故事怎样展开，再形成大纲；不必先填完所有字段。" : "可以从接下来的几章要发生什么开始讨论。"}也可以展开手动编辑。</p>}
    {view === "outline" && !outlineNodeId && nodes.length > 0 && !nodes.some(node => node.kind === "book") && <p className="planning-reading-hint" role="status">尚未建立全书主线与故事脉络，已有卷纲仍保留。可以在对话中把整部故事串起来。</p>}
    {view === "outline" && dirty && <p className="planning-reading-hint">讨论具体段落前，请先保存手动修改，确保讨论的是当前内容。</p>}
    {view === "outline" && nodes.length > 0 && saved && <OutlineReadingTree key={saved.id} nodes={nodes} focusNodeId={outlineNodeId} onOpenNode={onOpenOutlineNode} revisionId={saved.id} blocked={blocked} dirty={dirty} onDiscuss={onDiscuss} />}
    {view === "plans" && chapters.length > 0 && <ol className="planning-reading-list">{[...chapters].sort((left, right) => left.chapterNumber - right.chapterNumber).map((chapter, index) => <li key={chapter.chapterPlanId ?? chapter.clientKey ?? index}>
      <article className="planning-reading-card" data-discussion-locator={chapter.chapterPlanId ?? chapter.clientKey} data-discussion-label={`第 ${chapter.chapterNumber} 章 · ${chapter.publicTitle || chapter.title || "章纲"}`}>
        <header><span>第 {chapter.chapterNumber} 章{chapter.locked ? " · 已锁定" : ""}</span><h2>{chapter.publicTitle || chapter.title || "标题待定"}</h2></header>
        <dl><div><dt>本章目标</dt><dd>{chapter.goal || "尚未说明"}</dd></div>
          {chapter.conflict && <div><dt>核心冲突</dt><dd>{chapter.conflict}</dd></div>}
          {chapter.writerGuidance && <div><dt>给正文的指引</dt><dd>{chapter.writerGuidance}</dd></div>}
        </dl>
        {Boolean(chapter.scenes?.length) && <details><summary>场景与节拍 · {chapter.scenes?.length} 个场景</summary><ul>{chapter.scenes?.map((scene, sceneIndex) => <li key={scene.id}>
          <strong>{scene.title || `场景 ${sceneIndex + 1}`}{scene.locked ? " · 已锁定" : ""}</strong><p>{scene.goal}</p>
          {scene.beats.length > 0 && <ol>{scene.beats.map(beat => <li key={beat.id}>{beat.description}{beat.locked ? " · 已锁定" : ""}</li>)}</ol>}
        </li>)}</ul></details>}
      </article>
    </li>)}</ol>}
  </section>;
}
