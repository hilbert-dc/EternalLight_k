import type { ChapterPlanPayload, CreativeArtifactKind, OutlineNode } from "./types";
import { OutlineReadingTree } from "./OutlineReadingTree";

export function PlanningRevisionPreview({ kind, payload, outlineNodeId = null }: { kind: CreativeArtifactKind; payload: Record<string, unknown> | null; outlineNodeId?: string | null }) {
  if (!payload) return <p role="status">这份修订尚未提供规划内容。</p>;
  if (kind === "outline") {
    const nodes = (payload.nodes ?? []) as OutlineNode[];
    const matches = outlineNodeId ? nodes.filter(node => (node.id ?? node.clientKey) === outlineNodeId) : [];
    if (outlineNodeId && (matches.length !== 1 || matches[0].kind !== "volume"))
      return <p role="status">这份已保存内容不包含可唯一定位的本卷；未替换为其他卷或全书内容。</p>;
    return <OutlineReadingTree nodes={nodes} focusNodeId={outlineNodeId} revisionId="comparison" blocked dirty={false} />;
  }
  if (kind === "brief") return <dl className="planning-version-content">{[
    ["premise", "故事构想"], ["promise", "读者期待"], ["stakes", "核心代价"], ["endingDirection", "结局方向"],
  ].map(([key, label]) => <div key={key}><dt>{label}</dt><dd>{String(payload[key] || "尚未填写")}</dd></div>)}</dl>;
  return <ol className="planning-version-content">{((payload.chapters ?? []) as ChapterPlanPayload[]).map((chapter, index) => <li key={index}>
    <h3>第 {chapter.chapterNumber} 章 · {chapter.publicTitle || chapter.title || "标题待定"}{chapter.locked ? " · 已锁定" : ""}</h3>
    <dl><div><dt>本章目标</dt><dd>{chapter.goal || "尚未说明"}</dd></div>
      {chapter.conflict && <div><dt>核心冲突</dt><dd>{chapter.conflict}</dd></div>}
      {chapter.storyTime != null && <div><dt>故事时间</dt><dd>{chapter.storyTime}</dd></div>}
      {Boolean(chapter.publicCommitments?.length) && <div><dt>必须兑现</dt><dd>{chapter.publicCommitments!.join("；")}</dd></div>}
      {Boolean(chapter.allowances?.length) && <div><dt>可以自由发挥</dt><dd>{chapter.allowances!.join("；")}</dd></div>}
      {chapter.writerGuidance && <div><dt>写作备注</dt><dd>{chapter.writerGuidance}</dd></div>}
    </dl>
    {(chapter.scenes ?? []).length > 0 && <details><summary>场景与节拍 · {(chapter.scenes ?? []).length}</summary><ol>{(chapter.scenes ?? []).map((scene, sceneIndex) => <li key={sceneIndex}>
      <h4>{scene.title || `场景 ${sceneIndex + 1}`}{scene.locked ? " · 已锁定" : ""}</h4><p>{scene.goal}</p>{scene.conflict && <p>核心冲突：{scene.conflict}</p>}
      {scene.expectedEffects.length > 0 && <dl>{scene.expectedEffects.map(effect => <div key={effect.id}><dt>预期变化</dt><dd>{effect.description}</dd></div>)}</dl>}
      <ol>{scene.beats.map((beat, beatIndex) => <li key={beatIndex}>{beat.description}{beat.locked ? " · 已锁定" : ""}</li>)}</ol>
    </li>)}</ol></details>}
  </li>)}</ol>;
}
