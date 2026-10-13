import { useEffect, useMemo, useRef, useState } from "react";
import type { NovelCodexApi, ThreadDetail } from "./types";

export function useConversationReader(api: NovelCodexApi | undefined, projectId: string, threadId: string | null, authorKey: string, enabled: boolean) {
  const owner = useMemo(() => ({}), [api, projectId, threadId, authorKey, enabled]);
  const currentOwner = useRef(owner); currentOwner.current = owner;
  const request = useRef<object | null>(null);
  const [reading, setReading] = useState<{ owner: object; projectId: string; threadId: string; detail: ThreadDetail | null; loading: boolean; error: string | null } | null>(null);
  async function read() {
    if (!api || !projectId || !threadId || !enabled) return;
    const ticket = {}; request.current = ticket;
    const current = () => currentOwner.current === owner && request.current === ticket;
    setReading({ owner, projectId, threadId, detail: null, loading: true, error: null });
    try {
      const detail = await api.agent.readConversation(projectId, threadId);
      if (!current()) return;
      if (detail.thread.id !== threadId || detail.thread.projectId !== projectId) throw new Error("对话不属于当前作品");
      setReading({ owner, projectId, threadId, detail, loading: false, error: null });
    } catch (error) { if (current()) setReading({ owner, projectId, threadId, detail: null, loading: false, error: error instanceof Error ? error.message : "读取对话失败" }); }
  }
  useEffect(() => { currentOwner.current = owner; void read(); return () => { request.current = null; if (currentOwner.current === owner) currentOwner.current = {}; }; }, [owner]);
  return { reading: reading?.owner === owner ? reading : null, refresh: read };
}
