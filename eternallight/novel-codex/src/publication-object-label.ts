import type { StoryCanon } from "./types";

export function publicationObjectLabel(id: string | null, entities: unknown[], canon: StoryCanon): string {
  if (!id) return "无";
  const entity = entities.find(value => value && typeof value === "object" && (value as Record<string, unknown>).id === id);
  if (entity && typeof (entity as Record<string, unknown>).displayName === "string") {
    const name = ((entity as Record<string, unknown>).displayName as string).trim();
    if (name) return name;
  }
  return [...canon.characters, ...canon.locations].find(value => value.id === id)?.name || "未命名对象";
}
