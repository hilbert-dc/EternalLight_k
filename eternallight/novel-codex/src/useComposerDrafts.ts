import { useEffect, useMemo, useRef, useState, type SetStateAction } from "react";
import type { DiscussionExcerpt, DiscussionSource } from "./types";
import { validateDiscussionExcerptShape } from "../shared/discussion-excerpt-shape.mjs";
let excerptValidator: typeof import("../shared/discussion-excerpts.mjs").discussionExcerptsError | undefined;
const loadValidator = async () => { excerptValidator ??= (await import("../shared/discussion-excerpts.mjs")).discussionExcerptsError; return excerptValidator; };
import { MAX_AUTHOR_INTENT_BYTES, MAX_AUTHOR_INTENT_CHARACTERS } from "../shared/author-intent-validation.mjs";

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
        || saved.excerpts.some((item: DiscussionExcerpt) => item.projectId !== projectId)
        || saved.source && saved.source.focus?.reference?.projectId !== projectId) return empty();
      if (saved.excerpts.length) validateDiscussionExcerptShape(saved.excerpts);
      return saved;
    } catch { return empty(); }
  };
  const owner = useMemo(() => ({}), [scope]);
  const currentOwner = useRef(owner); currentOwner.current = owner;
  useEffect(() => { currentOwner.current = owner; return () => { if (currentOwner.current === owner) currentOwner.current = {}; }; }, [owner]);
  const [validatorLoaded, setValidatorLoaded] = useState(Boolean(excerptValidator));
  const editVersion = useRef(0);
  const revisions = useRef(new Map<string, number>());
  const bump = () => { editVersion.current++; revisions.current.set(scope, (revisions.current.get(scope) ?? 0) + 1); };
  const drafts = useRef(new Map<string, Draft>());
  const [input, setInput] = useState({ projectId, scope, ...read() });
  const [error, setError] = useState<string | null>(null);
  const visible = input.scope === scope ? input : drafts.current.get(scope) ?? read();
  const current = useRef({ projectId, scope, ...visible });
  current.current = { ...visible, projectId, scope };
  const excerptError = useMemo(() => visible.excerpts.length && excerptValidator ? excerptValidator(visible.excerpts) : null, [visible.excerpts, validatorLoaded]);
  const excerptsReady = !visible.excerpts.length || Boolean(excerptValidator && !excerptError);
  useEffect(() => {
    if (!visible.excerpts.length) return;
    if (excerptValidator) { setValidatorLoaded(true); return; }
    let active = true;
    void loadValidator().then(() => { if (active) setValidatorLoaded(true); }).catch(() => { if (active) setError("引用校验暂不可用，原文仍保留，请重新打开对话后重试。"); });
    return () => { active = false; };
  }, [scope, visible.excerpts.length]);
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
    bump();
    current.current = { ...current.current, value };
    setInput(current.current);
    setError(save());
  }
  function setSource(source: DiscussionSource | null) {
    if (current.current.scope !== scope || source && source.focus.reference.projectId !== projectId) return;
    bump();
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
    bump();
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
  async function addExcerpt(excerpt: DiscussionExcerpt) {
    if (current.current.scope !== scope || excerpt.projectId !== projectId) return false;
    const originalExcerpts = JSON.stringify(current.current.excerpts);
    let validate: Awaited<ReturnType<typeof loadValidator>>;
    try { validate = await loadValidator(); } catch { if (currentOwner.current === owner) setError("引用校验暂不可用，请重试；原文没有被截断。"); return false; }
    if (currentOwner.current !== owner || current.current.scope !== scope || JSON.stringify(current.current.excerpts) !== originalExcerpts) return false;
    setValidatorLoaded(true);
    const excerpts = [...current.current.excerpts, excerpt];
    const message = validate(excerpts);
    setError(message);
    if (message) return false;
    bump();
    current.current = { ...current.current, excerpts };
    setInput(current.current);
    setError(save());
    return true;
  }
  function removeExcerpt(id: string) {
    if (current.current.scope !== scope) return;
    bump();
    current.current = { ...current.current, excerpts: current.current.excerpts.filter(item => item.id !== id) };
    setInput(current.current); setError(save());
  }
  function clearExcerpts() {
    if (current.current.scope !== scope) return;
    bump();
    current.current = { ...current.current, excerpts: [] };
    setInput(current.current);
    setError(save());
  }
  function acceptance() {
    const revision = revisions.current.get(scope) ?? 0;
    const original = JSON.stringify({value: visible.value, source: visible.source, excerpts: visible.excerpts});
    return () => {
      if ((revisions.current.get(scope) ?? 0) !== revision) return;
      const value = current.current.scope === scope ? current.current : drafts.current.get(scope);
      if (!value || JSON.stringify({value:value.value,source:value.source,excerpts:value.excerpts}) !== original) return;
      try { localStorage.removeItem(storageKey); } catch { setError("消息已接收，但本机草稿清理失败。"); return; }
      drafts.current.delete(scope); revisions.current.set(scope, revision + 1);
      if (current.current.scope === scope) {
        editVersion.current++; current.current = {projectId, scope, ...empty()}; setInput(current.current);
      }
    };
  }
  return { acceptance, version: () => `${current.current.scope}:${editVersion.current}`, value: visible.value, source: visible.source, excerpts: visible.excerpts,
    excerptsReady, setValue, setSource, addExcerpt, removeExcerpt, clearExcerpts, preserve, clear, bindNewConversation, error: error ?? excerptError };
}
