import { useRef, useState, type SetStateAction } from "react";
import type { DiscussionExcerpt, DiscussionSource } from "./types";
import { discussionExcerptsError } from "../shared/discussion-excerpts.mjs";
import { MAX_AUTHOR_INTENT_BYTES, MAX_AUTHOR_INTENT_CHARACTERS } from "../shared/author-input-budget.mjs";

type Draft = { value: string; source: DiscussionSource | null; excerpts: DiscussionExcerpt[] };
const empty = (): Draft => ({ value: "", source: null, excerpts: [] });

// Text and its explicitly selected source travel together across project navigation.
// Capacity never silently evicts an author's draft.
export function useComposerDrafts(projectId: string, threadId: string | null = null, authorKey = "") {
  const scope = JSON.stringify([authorKey, projectId, threadId]);
  const storageKey = `novel-codex:conversation-draft:${scope}`;
  const read = (): Draft => {
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) ?? "null");
      if (!saved || typeof saved.value !== "string" || !Array.isArray(saved.excerpts)
        || saved.value.length > MAX_AUTHOR_INTENT_CHARACTERS || new TextEncoder().encode(saved.value).byteLength > MAX_AUTHOR_INTENT_BYTES
        || saved.excerpts.some((item: DiscussionExcerpt) => item.projectId !== projectId) || saved.excerpts.length && discussionExcerptsError(saved.excerpts)
        || saved.source && saved.source.focus?.reference?.projectId !== projectId) return empty();
      return saved;
    } catch { return empty(); }
  };
  const editVersion = useRef(0);
  const drafts = useRef(new Map<string, Draft>());
  const [input, setInput] = useState({ projectId, scope, ...read() });
  const [error, setError] = useState<string | null>(null);
  const visible = input.scope === scope ? input : drafts.current.get(scope) ?? read();
  const current = useRef({ projectId, scope, ...visible });
  current.current = { ...visible, projectId, scope };
  function save() {
    const { value, source, excerpts } = current.current;
    const draft = { value, source, excerpts };
    if (value || source || excerpts.length) drafts.current.set(scope, draft);
    else drafts.current.delete(scope);
    if (!projectId || !authorKey) return null;
    try {
      if (value || source || excerpts.length) localStorage.setItem(storageKey, JSON.stringify(draft));
      else localStorage.removeItem(storageKey);
      return null;
    } catch { return "本机对话草稿保存失败，请复制内容后重试；当前输入仍保留。"; }
  }

  function setValue(next: SetStateAction<string>) {
    if (current.current.scope !== scope) return;
    const value = typeof next === "function" ? next(current.current.value) : next;
    editVersion.current++;
    current.current = { ...current.current, value };
    setInput(current.current);
    setError(save());
  }
  function setSource(source: DiscussionSource | null) {
    if (current.current.scope !== scope || source && source.focus.reference.projectId !== projectId) return;
    editVersion.current++;
    current.current = { ...current.current, source };
    setInput(current.current);
    setError(save());
  }
  function preserve() {
    const { projectId, value, source, excerpts } = current.current;
    const message = value.length > MAX_AUTHOR_INTENT_CHARACTERS || new TextEncoder().encode(value).byteLength > MAX_AUTHOR_INTENT_BYTES
      ? "对话草稿过长，请先整理或复制保存，再切换作品。原文仍保留在输入框中。"
      : (value || source || excerpts.length) && drafts.current.size > 40
        ? "已有 40 个对话保留草稿，请先发送或清空其中一份，再切换。当前输入仍保留。"
        : null;
    setError(message);
    if (message) return false;
    const saveError = save(); setError(saveError);
    return !saveError;
  }
  function clear() {
    editVersion.current++;
    drafts.current.clear();
    current.current = { projectId, scope, ...empty() };
    setInput(current.current);
    setError(save());
  }
  function bindNewConversation(id: string) {
    // The first root may be opened by Send. Transfer only its still-unsent draft,
    // never a sent message or another conversation's input.
    if (threadId !== null || current.current.scope !== scope) return;
    const targetScope = JSON.stringify([authorKey, projectId, id]);
    const targetKey = `novel-codex:conversation-draft:${targetScope}`;
    const { value, source, excerpts } = current.current;
    const draft = { value, source, excerpts };
    drafts.current.set(targetScope, draft);
    try {
      if (value || source || excerpts.length) localStorage.setItem(targetKey, JSON.stringify(draft));
      localStorage.removeItem(storageKey);
      drafts.current.delete(scope);
    } catch { setError("本机对话草稿保存失败，输入仍保留在当前对话，请复制内容后重试。"); }
  }
  function addExcerpt(excerpt: DiscussionExcerpt) {
    if (current.current.scope !== scope || excerpt.projectId !== projectId) return false;
    const excerpts = [...current.current.excerpts, excerpt];
    const message = discussionExcerptsError(excerpts);
    setError(message);
    if (message) return false;
    editVersion.current++;
    current.current = { ...current.current, excerpts };
    setInput(current.current);
    setError(save());
    return true;
  }
  function removeExcerpt(id: string) {
    if (current.current.scope !== scope) return;
    editVersion.current++;
    current.current = { ...current.current, excerpts: current.current.excerpts.filter(item => item.id !== id) };
    setInput(current.current); setError(save());
  }
  function clearExcerpts() {
    if (current.current.scope !== scope) return;
    editVersion.current++;
    current.current = { ...current.current, excerpts: [] };
    setInput(current.current);
    setError(save());
  }
  return { version: () => `${current.current.scope}:${editVersion.current}`, value: visible.value, source: visible.source, excerpts: visible.excerpts,
    setValue, setSource, addExcerpt, removeExcerpt, clearExcerpts, preserve, clear, bindNewConversation, error };
}
