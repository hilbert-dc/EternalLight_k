import type { CreativeWorkspace } from "./types";

type Scenes = NonNullable<CreativeWorkspace["chapters"][number]["candidate"]>["sceneGroups"];

// Compare the editable chapter, including identity/order and protection, to its loaded baseline.
export function chapterEditorContent(title: string, scenes: Scenes, locked: string[]): string {
  return JSON.stringify({ title, scenes: scenes.map(scene => ({ id: scene.id,
    sceneId: scene.sceneId ?? null, body: scene.body, locked: locked.includes(scene.id) })) });
}
