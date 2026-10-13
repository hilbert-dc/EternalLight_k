import type { DraftRevision } from "./types";

type Scene = DraftRevision["sceneGroups"][number];
export type ChapterDraft = { title: string; sceneGroups: Scene[] };
const conflict = Symbol("conflict");
const mergeField = <T>(base: T, local: T, remote: T): T | typeof conflict =>
  local === remote ? local : base === local ? remote : base === remote ? local : conflict;
const key = (scene: Scene) => scene.sceneId ?? scene.id;
function keys(scenes: Scene[]): string[] | null {
  const values = scenes.map(key);
  return values.every(value => typeof value === "string" && value.length > 0)
    && new Set(values).size === values.length && new Set(scenes.map(scene => scene.id)).size === scenes.length ? values : null;
}

// Scene order is an atomic structural choice. Independent content edits can
// follow a single changed order; competing orders require explicit review.
export function mergeChapterDraft(base: ChapterDraft, local: ChapterDraft, remote: ChapterDraft): ChapterDraft | null {
  return reconcileChapter(base, local, remote);
}

export function resolveChapterDraft(base: ChapterDraft, local: ChapterDraft, remote: ChapterDraft, preference: "local" | "remote"): ChapterDraft | null {
  return reconcileChapter(base, local, remote, preference);
}

function reconcileChapter(base: ChapterDraft, local: ChapterDraft, remote: ChapterDraft, preference?: "local" | "remote"): ChapterDraft | null {
  const field = <T>(b: T, l: T, r: T) => {
    const result = mergeField(b, l, r);
    return result === conflict && preference ? preference === "local" ? l : r : result;
  };
  const identities = [base, local, remote].map(value => keys(value.sceneGroups));
  if (identities.some(value => !value)) return null;
  let order = field(...identities.map(value => JSON.stringify(value)) as [string, string, string]);
  const title = field(base.title, local.title, remote.title);
  if (order === conflict || title === conflict) return null;
  const [bs, ls, rs] = [base, local, remote].map(value => new Map(value.sceneGroups.map(scene => [key(scene), scene])));
  const unchanged = (a: Scene, b: Scene) => a.body === b.body && Boolean(a.locked) === Boolean(b.locked);
  // A deletion cannot erase another side's edits or a protected scene.
  for (const [id, b] of bs) {
    const l = ls.get(id), r = rs.get(id);
    if (!l || !r) {
      if (b.locked || l?.locked || r?.locked) return null;
      if ((l && !unchanged(b, l)) || (r && !unchanged(b, r))) {
        if (!preference) return null;
        order = JSON.stringify(identities[preference === "local" ? 1 : 2]);
      }
    }
  }
  const sceneGroups: Scene[] = [];
  for (const id of JSON.parse(order) as string[]) {
    const b = bs.get(id), l = ls.get(id), r = rs.get(id);
    if (!b) {
      if (l && r && !unchanged(l, r) && !preference) return null;
      const added = l && r && preference === "local" ? l : r ?? l;
      if (!added) return null;
      sceneGroups.push({ ...added });
      continue;
    }
    if (!l || !r) {
      const retained = preference === "local" ? l : preference === "remote" ? r : undefined;
      if (!retained) return null;
      sceneGroups.push({ ...retained });
      continue;
    }
    if ((b.locked || r.locked) && l.body !== b.body) return null;
    if (l.locked && r.body !== b.body) return null;
    const body = field(b.body, l.body, r.body);
    const locked = field(Boolean(b.locked), Boolean(l.locked), Boolean(r.locked));
    if (body === conflict || locked === conflict) return null;
    sceneGroups.push({ ...r, body, locked });
  }
  // Revision-local IDs can collide even when stable scene identities do not.
  if (!keys(sceneGroups)) return null;
  return structuredClone({ title, sceneGroups });
}
