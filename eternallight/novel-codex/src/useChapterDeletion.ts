import { useEffect, useMemo, useRef, useState } from "react";
import { savedChapterTitle } from "./chapter-title";
import type { ChapterDeletionReceipt, NovelCodexApi, RuntimeState } from "./types";

type Attempt = {
  owner: object; chapterId: string; title: string; number: number;
  launcher: HTMLElement | null;
  input: { chapterId: string; expectedRevision: number; idempotencyKey: string } | null;
  phase: "prepare" | "confirm" | "check"; busy: boolean; error: string | null;
  receipt: ChapterDeletionReceipt | null;
};

export function useChapterDeletion({ api, projectId, enabled, isBlocked, onApplied }: {
  api: NovelCodexApi | undefined; projectId: string; enabled: boolean;
  isBlocked: () => boolean; onApplied: (next: RuntimeState, chapterId: string) => void;
}) {
  const owner = useMemo(() => ({}), [api, projectId, enabled]);
  const current = useRef<object | null>(owner); current.current = owner;
  const callbacks = useRef({ isBlocked, onApplied }); callbacks.current = { isBlocked, onApplied };
  const [state, setState] = useState<Attempt | null>(null);
  const attempt = useRef<Attempt | null>(null);
  const flight = useRef<object | null>(null);
  useEffect(() => {
    current.current = owner;
    return () => { if (current.current === owner) { current.current = null; attempt.current = null; flight.current = null; } };
  }, [owner]);
  const owns = (value: Attempt) => current.current === value.owner && attempt.current === value;
  const put = (value: Attempt | null) => { attempt.current = value; setState(value); };
  function restoreFocus(value: Attempt) {
    requestAnimationFrame(() => {
      if (current.current !== value.owner || document.querySelector("dialog[open]")) return;
      const target = value.launcher?.isConnected ? value.launcher
        : document.querySelector<HTMLElement>('[data-directory-chapter-id][aria-current="page"], [aria-label="作品目录操作"]');
      target?.focus({ preventScroll: true });
    });
  }

  async function prepare(value: Attempt) {
    if (!api || !owns(value) || flight.current || callbacks.current.isBlocked()) return;
    const ticket = {}; flight.current = ticket;
    put({ ...value, busy: true, error: null }); const reading = attempt.current!;
    try {
      const workspace = await api.runtime.getCreativeWorkspace(projectId);
      if (!owns(reading)) return;
      if (workspace.project.id !== projectId) throw new Error("章节不属于当前作品。");
      const chapter = workspace.chapters.find(item => item.id === reading.chapterId);
      if (!chapter) throw new Error("本章已不在作品中，请重新读取作品目录。");
      put({ ...reading, title: savedChapterTitle(chapter), number: chapter.number,
        phase: "confirm", busy: false,
        input: { chapterId: chapter.id, expectedRevision: workspace.revision, idempotencyKey: crypto.randomUUID() } });
    } catch (reason) {
      if (owns(reading)) put({ ...reading, busy: false, error: reason instanceof Error ? reason.message : "读取章节失败。" });
    } finally { if (flight.current === ticket) flight.current = null; }
  }

  function open(chapterId: string) {
    if (!api || !enabled || callbacks.current.isBlocked() || (flight.current && attempt.current?.owner === owner)) return;
    if (attempt.current?.owner !== owner) flight.current = null;
    if (attempt.current?.owner === owner && attempt.current.phase === "check") return;
    const value: Attempt = { owner, chapterId, title: "", number: 0, launcher: document.activeElement as HTMLElement | null,
      input: null, phase: "prepare", busy: false, error: null, receipt: null };
    put(value); void prepare(value);
  }

  async function check(value: Attempt) {
    if (!api || !owns(value)) return;
    const [next, workspace, catalog] = await Promise.all([
      api.runtime.getState(projectId), api.runtime.getCreativeWorkspace(projectId), api.runtime.getWorkCatalog(projectId),
    ]);
    if (!owns(value)) return;
    if (next.project.id !== projectId || workspace.project.id !== projectId) throw new Error("删除结果不属于当前作品。");
    const chapter = workspace.chapters.find(item => item.id === value.chapterId);
    if (chapter || next.novelEdition.chapters.some(item => item.id === value.chapterId) || catalog.placements.some(item => item.chapterId === value.chapterId)) {
      if (!value.receipt && chapter) {
        put({ ...value, phase: "confirm", busy: false, title: savedChapterTitle(chapter), number: chapter.number,
          input: { chapterId: chapter.id, expectedRevision: workspace.revision, idempotencyKey: crypto.randomUUID() },
          error: "本章仍在作品中，尚未删除。请核对后重新确认。" });
        return;
      }
      throw new Error("删除结果尚未同步，请重新核对；不会重复删除。");
    }
    if (value.receipt && (workspace.revision < value.receipt.resultRevision || catalog.revision < value.receipt.catalogRevision)) {
      throw new Error("删除结果尚未同步，请重新核对；不会重复删除。");
    }
    callbacks.current.onApplied(next, value.chapterId);
    put(null); restoreFocus(value);
  }

  async function confirm() {
    const value = attempt.current;
    if (!api || !value || !owns(value) || value.phase !== "confirm" || !value.input || flight.current || callbacks.current.isBlocked()) return;
    const ticket = {}; flight.current = ticket;
    put({ ...value, phase: "check", busy: true, error: null }); let checking = attempt.current!;
    try {
      const receipt = await api.runtime.deleteChapter(projectId, value.input);
      if (!owns(checking)) return;
      if (receipt.operation !== "delete-chapter" || receipt.projectId !== projectId || receipt.chapterId !== value.chapterId
        || receipt.baseRevision !== value.input.expectedRevision || receipt.resultRevision !== receipt.baseRevision + 1
        || !Number.isSafeInteger(receipt.catalogRevision) || receipt.catalogRevision < 0) throw new Error("删除回执与本章不一致。");
      put({ ...checking, receipt }); checking = attempt.current!;
      await check(checking);
    } catch (reason) {
      if (owns(checking)) put({ ...checking, busy: false, error: `${reason instanceof Error ? reason.message : "删除结果尚未确认。"} 请核对删除结果；不会自动重复删除。` });
    } finally { if (flight.current === ticket) flight.current = null; }
  }

  async function reconcile() {
    const value = attempt.current;
    if (!value || !owns(value) || flight.current) return;
    if (value.phase === "prepare") { await prepare(value); return; }
    if (value.phase !== "check") return;
    const ticket = {}; flight.current = ticket;
    put({ ...value, busy: true, error: null }); const reading = attempt.current!;
    try { await check(reading); }
    catch (reason) { if (owns(reading)) put({ ...reading, busy: false, error: reason instanceof Error ? reason.message : "核对失败，请稍后重新读取。" }); }
    finally { if (flight.current === ticket) flight.current = null; }
  }

  function close() {
    const value = attempt.current;
    if (value?.owner !== owner || value.phase === "check" || value.busy) return;
    put(null); restoreFocus(value);
  }
  return { state: state?.owner === owner ? state : null, open, confirm, reconcile, close, blocked: callbacks.current.isBlocked(),
    active: state?.owner === owner, busy: state?.owner === owner && state.busy };
}
