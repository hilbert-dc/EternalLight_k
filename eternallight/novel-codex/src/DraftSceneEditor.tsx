import { useRef, type RefObject } from "react";
import type { DraftRevision } from "./types";
import { newDraftScene } from "./creative-product-logic";

type Scene = DraftRevision["sceneGroups"][number];

export function DraftSceneEditor({ scenes, lockedIds, disabled, firstEditorRef, onChange }: {
  scenes: Scene[];
  lockedIds: string[];
  disabled: boolean;
  firstEditorRef: RefObject<HTMLTextAreaElement | null>;
  onChange: (scenes: Scene[]) => void;
}) {
  const editors = useRef(new Map<string, HTMLTextAreaElement>());
  const focusScene = (id: string | undefined) => requestAnimationFrame(() => {
    if (id) editors.current.get(id)?.focus();
  });
  const isLocked = (scene: Scene) => lockedIds.includes(scene.id);
  const move = (index: number, destination: number) => {
    const next = [...scenes];
    [next[index], next[destination]] = [next[destination], next[index]];
    onChange(next);
  };
  return <fieldset className="draft-editor-wrap" data-chapter-draft-anchor disabled={disabled}>
    <legend className="sr-only">按场景编辑</legend>
    {scenes.map((scene, index) => <section key={scene.id} data-scene-id={scene.id}>
      <label><strong>场景 {index + 1}{isLocked(scene) ? " · 已锁定" : ""}<small>{scene.body.length} 字</small></strong>
        <textarea ref={(element) => { if (element) editors.current.set(scene.id, element); else editors.current.delete(scene.id); if (index === 0) firstEditorRef.current = element; }} className="draft-editor" aria-label={`场景 ${index + 1} 正文`} value={scene.body} disabled={disabled || isLocked(scene)} onChange={(event) => onChange(scenes.map((item) => item.id === scene.id ? { ...item, body: event.target.value } : item))} placeholder="从一个场景开始……" />
      </label>
      <div className="draft-scene-actions">
        <button type="button" disabled={isLocked(scene) || index === 0 || isLocked(scenes[index - 1])} onClick={() => move(index, index - 1)}>上移场景 {index + 1}</button>
        <button type="button" disabled={isLocked(scene) || index === scenes.length - 1 || isLocked(scenes[index + 1])} onClick={() => move(index, index + 1)}>下移场景 {index + 1}</button>
        <button type="button" disabled={scenes.slice(index).some(isLocked)} onClick={() => { onChange(scenes.filter((item) => item.id !== scene.id)); focusScene(scenes[index - 1]?.id ?? scenes[index + 1]?.id); }}>删除场景 {index + 1}</button>
      </div>
    </section>)}
    <button type="button" className="draft-add-scene" onClick={() => { const scene = newDraftScene(); onChange([...scenes, scene]); focusScene(scene.id); }}>＋ 添加场景</button>
  </fieldset>;
}
