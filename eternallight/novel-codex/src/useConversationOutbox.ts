import { useCallback, useEffect, useRef, useState } from "react";
import type { ConversationFocus, ConversationMessage, MessageTrigger, NovelCodexApi } from "./types";

// Submitted messages live in Native. This hook contains only a disposable view.
export function useConversationOutbox(api: NovelCodexApi | undefined, projectId: string, threadId: string | null) {
  const scope = JSON.stringify([projectId, threadId]);
  const current = useRef(scope); current.current = scope;
  const [snapshot, setSnapshot] = useState<{ scope: string; items: ConversationMessage[] }>({ scope, items: [] });
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(new Set<string>());
  const unconfirmedKey = `novel-codex:message-submission:${scope}`;

  const refresh = useCallback(async () => {
    if (!api || !projectId || !threadId) return;
    try {
      const items = await api.agent.listMessages(projectId, threadId);
      const unconfirmed = localStorage.getItem(unconfirmedKey);
      if (unconfirmed) {
        const pending = JSON.parse(unconfirmed);
        // A delivered item no longer appears in the queue. Read its durable
        // receipt by the original ID; never resend it while recovering the UI.
        const receipt = items.find(item => item.id === pending.messageId)
          ?? await api.agent.readMessage(projectId, threadId, pending.messageId).catch(() => null);
        if (receipt && receipt.id === pending.messageId && receipt.projectId === projectId && receipt.threadId === threadId
          && localStorage.getItem(unconfirmedKey) === unconfirmed) localStorage.removeItem(unconfirmedKey);
      }
      if (current.current === scope) { setSnapshot({ scope, items }); setError(null); }
    } catch (error) {
      if (current.current === scope) setError(`待发送消息读取失败：${error instanceof Error ? error.message : "请重试"}`);
    }
  }, [api, projectId, threadId, scope, unconfirmedKey]);
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 2000);
    return () => clearInterval(timer);
  }, [refresh]);
  async function add(text: string, focus: ConversationFocus | null, trigger: MessageTrigger) {
    if (!api || !threadId || pending.current.has(scope)) return false;
    pending.current.add(scope);
    try {
      const body = { input: { text, focus, connectionId: null }, trigger };
      const prior = JSON.parse(localStorage.getItem(unconfirmedKey) ?? "null");
      if (prior && JSON.stringify(prior.body) !== JSON.stringify(body)) throw new Error("上一条提交结果尚未确认，请先核对原消息");
      const submission = prior ?? { messageId: crypto.randomUUID(), body };
      localStorage.setItem(unconfirmedKey, JSON.stringify(submission));
      await api.agent.submitMessage(projectId, threadId, { messageId: submission.messageId, ...body });
      localStorage.removeItem(unconfirmedKey);
      await refresh(); return true;
    } catch (error) {
      if (current.current === scope) setError(`消息尚未确认接收：${error instanceof Error ? error.message : "请核对"}`);
      return false;
    } finally { pending.current.delete(scope); }
  }
  const items = snapshot.scope === scope ? snapshot.items : [];
  async function change(id: string, action: "cancel" | "continue" | "move" | "edit" | "reconcile", values: Record<string, unknown> = {}) {
    const item = items.find(item => item.id === id);
    if (!api || !threadId || !item || pending.current.has(scope)) return false;
    pending.current.add(scope);
    try { await api.agent.changeMessage(projectId, threadId, id, { action, expectedRevision: item.revision, ...values }); await refresh(); return true; }
    catch (error) { if (current.current === scope) setError(error instanceof Error ? error.message : "消息操作未确认，请刷新后核对"); return false; }
    finally { pending.current.delete(scope); }
  }
  return { items, error, add, refresh,
    remove: (id: string) => change(id, "cancel"),
    reconcile: (id: string) => change(id, "reconcile"),
    continue: (id: string) => change(id, "continue"),
    move: (id: string, direction: number) => change(id, "move", { direction }),
    edit: (id: string, text: string, expectedRevision: number) => {
      const item = items.find(x => x.id === id);
      if (!item || item.revision !== expectedRevision) {
        setError("消息已变化；编辑草稿已保留，请核对最新消息后重新编辑。");
        return Promise.resolve(false);
      }
      return change(id, "edit", { input: { ...item.input, text } });
    },
  };
}
