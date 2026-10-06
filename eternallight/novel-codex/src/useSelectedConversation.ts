import { useRef, useState } from "react";

export function useSelectedConversation(author: string, projectId: string) {
  const scope = JSON.stringify([author, projectId]);
  const key = `novel-codex:selected-conversation:${scope}`;
  function read() { try { return localStorage.getItem(key); } catch { return null; } }
  const [selection, setSelection] = useState({ scope, id: read() });
  const id = selection.scope === scope ? selection.id : read();
  const current = useRef({ scope, id }); current.current = { scope, id };
  return { id, current,
    select(next: string | null) {
      current.current = { scope, id: next }; setSelection(current.current);
      try { if (next) localStorage.setItem(key, next); else localStorage.removeItem(key); } catch { /* Selection still works in memory; draft storage reports failures. */ }
    },
  };
}
