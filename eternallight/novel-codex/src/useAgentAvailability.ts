import { useEffect, useRef } from "react";
import type { NovelCodexApi } from "./types";
type Status = Awaited<ReturnType<NovelCodexApi["agent"]["status"]>>;

// Availability is a local evidence read. It never verifies a provider or submits a turn.
export function useAgentAvailability({ api, blocked, provider, model, canApply, onStatus }: {
  api: NovelCodexApi | undefined; blocked: boolean; provider?: string; model?: string;
  canApply: () => boolean; onStatus: (status: Status) => void;
}) {
  const lifetime = useRef({ api, blocked, provider, model });
  if (lifetime.current.api !== api || lifetime.current.blocked !== blocked
    || lifetime.current.provider !== provider || lifetime.current.model !== model) lifetime.current = { api, blocked, provider, model };
  const owner = lifetime.current;
  const latest = useRef({ canApply, onStatus });
  latest.current = { canApply, onStatus };
  useEffect(() => {
    if (!api || blocked) return;
    let cancelled = false, pending = false;
    const refresh = async () => {
      if (pending || cancelled || lifetime.current !== owner || !latest.current.canApply()) return;
      pending = true;
      try {
        const status = await api.agent.status();
        if (!cancelled && lifetime.current === owner && latest.current.canApply() && status.provider === provider && status.model === model) latest.current.onStatus(status);
      } catch { /* A failed status read cannot invent readiness or invalidate an active run. */ }
      finally { pending = false; }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 15_000);
    window.addEventListener("focus", refresh);
    return () => { cancelled = true; window.clearInterval(timer); window.removeEventListener("focus", refresh); };
  }, [api, blocked, provider, model]);
}
