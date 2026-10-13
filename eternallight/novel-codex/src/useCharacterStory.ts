import { useEffect, useMemo, useRef, useState } from "react";
import type { CharacterKnowledge, CreativeWorkspace, IdentityCatalog, NovelCodexApi } from "./types";

type Owner = { api?: NovelCodexApi; projectId: string; refreshToken: number; retry: number };
type CatalogRead = { owner: Owner; workspace: CreativeWorkspace | null; catalog: IdentityCatalog | null; loading: boolean; error: string | null };
type KnowledgeOwner = { owner: Owner; entityId: string | null; workspace: CreativeWorkspace | null; catalog: IdentityCatalog | null };
type KnowledgeRead = { owner: KnowledgeOwner; knowledge: CharacterKnowledge | null; loading: boolean; error: string | null };
const changed = () => new Error("人物资料读取期间作品已变化，请重新读取。");
const sameWatermark = (left: CreativeWorkspace, right: CreativeWorkspace) => left.project.id === right.project.id
  && left.revision === right.revision && left.headSnapshotId === right.headSnapshotId && left.sourceDigest === right.sourceDigest;
const sameProject = (left: Owner, right: Owner) => left.api === right.api && left.projectId === right.projectId;
const message = (reason: unknown) => reason instanceof Error ? reason.message : "人物资料暂时无法读取";
const currentSource = (workspace: CreativeWorkspace, objectId: string, revisionId: string, batchId?: string) => {
  const chapters = workspace.chapters.filter(chapter => objectId === `draft-${chapter.id}`);
  // Workspace only projects the first 100 modeling heads; absence cannot negate
  // the Author query's native, current-source validation.
  const modeled = workspace.modeling.filter(source => source.objectId === objectId);
  return chapters.length === 1 && (chapters[0].candidate ?? chapters[0].accepted)?.id === revisionId
    && (modeled.length === 0 || modeled.length === 1 && modeled[0].sourceRevisionId === revisionId
      && modeled[0].status === "ready" && (!batchId || modeled[0].batchId === batchId));
};

/** Reads current saved-source character claims at one verified watermark; never creates or confirms knowledge. */
export function useCharacterStory({ api, projectId, refreshToken, selectedEntityId }: {
  api?: NovelCodexApi; projectId: string; refreshToken: number; selectedEntityId: string | null;
}) {
  const [retryCount, setRetryCount] = useState(0);
  const [read, setRead] = useState<CatalogRead | null>(null);
  const [knowledgeRead, setKnowledgeRead] = useState<KnowledgeRead | null>(null);
  const owner = useMemo(() => ({ api, projectId, refreshToken, retry: retryCount }), [api, projectId, refreshToken, retryCount]);
  const active = useRef<Owner | null>(owner); active.current = owner;
  useEffect(() => {
    active.current = owner;
    if (!api || !projectId) return;
    let live = true;
    const isCurrent = () => live && active.current === owner;
    setRead(previous => ({ owner, loading: true, error: null,
      workspace: previous && sameProject(previous.owner, owner) ? previous.workspace : null,
      catalog: previous && sameProject(previous.owner, owner) ? previous.catalog : null }));
    void (async () => {
      const workspace = await api.runtime.getCreativeWorkspace(projectId);
      if (!isCurrent()) return;
      if (workspace.project.id !== projectId || !workspace.headSnapshotId) throw changed();
      const catalog = await api.runtime.getIdentityCatalog(projectId);
      if (!isCurrent()) return;
      if (catalog.entries.some(entry => entry.claims.some(claim => !currentSource(workspace, claim.sourceObjectId, claim.sourceRevisionId)))) throw changed();
      const fresh = await api.runtime.getCreativeWorkspace(projectId);
      if (!isCurrent()) return;
      if (!sameWatermark(workspace, fresh)) throw changed();
      setRead({ owner, workspace, catalog, loading: false, error: null });
    })().catch(reason => {
      if (isCurrent()) setRead(previous => ({ owner, loading: false, error: message(reason),
        workspace: previous?.owner === owner ? previous.workspace : null,
        catalog: previous?.owner === owner ? previous.catalog : null }));
    });
    return () => { live = false; if (active.current === owner) active.current = null; };
  }, [owner]);
  const visible = read && sameProject(read.owner, owner) ? read : null;
  const current = visible?.owner === owner && !visible.loading && !visible.error;
  const workspace = visible?.workspace ?? null;
  const catalog = visible?.catalog ?? null;
  const selectedValid = Boolean(selectedEntityId && catalog?.entries.some(entry => entry.kind === "character" && entry.canonicalId === selectedEntityId));
  const knowledgeOwner = useMemo(() => ({ owner, entityId: selectedEntityId, workspace, catalog }),
    [owner, selectedEntityId, workspace, catalog]);
  const activeKnowledge = useRef<KnowledgeOwner | null>(knowledgeOwner); activeKnowledge.current = knowledgeOwner;
  useEffect(() => {
    activeKnowledge.current = knowledgeOwner;
    if (!current || !api || !workspace || !catalog || !selectedEntityId || !selectedValid) return;
    let live = true;
    const isCurrent = () => live && active.current === owner && activeKnowledge.current === knowledgeOwner;
    setKnowledgeRead(previous => ({ owner: knowledgeOwner, loading: true, error: null,
      knowledge: previous && sameProject(previous.owner.owner, owner) && previous.owner.entityId === selectedEntityId
        && previous.owner.workspace?.sourceDigest === workspace.sourceDigest
        && previous.knowledge?.identityRevisionId === catalog.identityRevisionId ? previous.knowledge : null }));
    void (async () => {
      const knowledge = await api.runtime.getCharacterKnowledge(projectId, selectedEntityId);
      if (!isCurrent()) return;
      if (knowledge.projectId !== projectId || knowledge.snapshotId !== null || knowledge.perspective !== "author"
        || knowledge.identityRevisionId !== catalog.identityRevisionId || knowledge.selectors.entityId !== selectedEntityId
        || knowledge.selectors.predicate !== null) throw changed();
      const seen = new Set<string>();
      for (const entry of knowledge.entries) {
        const key = `${entry.modelingBatchId}:${entry.assertionIndex}`;
        const chapters = workspace.chapters.filter(chapter => entry.sourceObjectId === `draft-${chapter.id}`);
        if (!entry.sourceRevisionId || !entry.modelingBatchId || !Number.isSafeInteger(entry.assertionIndex) || entry.assertionIndex < 0
          || seen.has(key) || !currentSource(workspace, entry.sourceObjectId, entry.sourceRevisionId, entry.modelingBatchId)
          || entry.sourceChapter !== null && entry.sourceChapter !== chapters[0].number) throw changed();
        seen.add(key);
      }
      const fresh = await api.runtime.getCreativeWorkspace(projectId);
      if (!isCurrent()) return;
      if (!sameWatermark(workspace, fresh)) throw changed();
      setKnowledgeRead({ owner: knowledgeOwner, knowledge, loading: false, error: null });
    })().catch(reason => {
      if (isCurrent()) setKnowledgeRead(previous => ({ owner: knowledgeOwner, loading: false, error: message(reason),
        knowledge: previous?.owner === knowledgeOwner ? previous.knowledge : null }));
    });
    return () => { live = false; if (activeKnowledge.current === knowledgeOwner) activeKnowledge.current = null; };
  }, [knowledgeOwner, current, selectedValid]);
  const visibleKnowledge = !selectedValid ? null : knowledgeRead && sameProject(knowledgeRead.owner.owner, owner)
    && knowledgeRead.owner.entityId === selectedEntityId && knowledgeRead.owner.workspace?.sourceDigest === workspace?.sourceDigest
    && knowledgeRead.knowledge?.identityRevisionId === catalog?.identityRevisionId
    ? knowledgeRead : knowledgeRead?.owner === knowledgeOwner ? knowledgeRead : null;
  return { workspace, catalog, knowledge: visibleKnowledge?.knowledge ?? null, current,
    loading: Boolean(api && projectId && (visible?.owner !== owner || visible.loading)), error: visible?.owner === owner ? visible.error : null,
    knowledgeCurrent: current && visibleKnowledge?.owner === knowledgeOwner && !visibleKnowledge.loading && !visibleKnowledge.error,
    knowledgeLoading: Boolean(current && selectedValid && (visibleKnowledge?.owner !== knowledgeOwner || visibleKnowledge.loading)),
    knowledgeError: visibleKnowledge?.owner === knowledgeOwner ? visibleKnowledge.error : null,
    retry: () => setRetryCount(value => value + 1), retryKnowledge: () => setRetryCount(value => value + 1) };
}
