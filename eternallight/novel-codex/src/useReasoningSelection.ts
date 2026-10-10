import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi } from "./types";

export function useReasoningSelection({ api, provider, model, onChange }: {
  api: NovelCodexApi | undefined; provider?: string | null; model?: string | null; onChange: (effort: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ provider: string; model: string; message: string } | null>(null);
  const current = useRef({ provider, model, onChange });
  current.current = { provider, model, onChange };
  const pending = useRef(false);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  async function change(effort: string) {
    if (!api || !provider || !model || pending.current) return;
    const isCurrent = () => mounted.current && current.current.provider === provider && current.current.model === model;
    pending.current = true; setBusy(true); setFailure(null);
    try {
      const result = await api.agent.setReasoningEffort(provider, model, effort);
      if (isCurrent()) current.current.onChange(result.reasoningEffort);
    } catch (error) {
      if (isCurrent()) setFailure({ provider, model, message: error instanceof Error ? error.message : "推理强度调整失败，请重试。" });
    } finally { pending.current = false; if (mounted.current) setBusy(false); }
  }
  return { busy, change, error: failure && failure.provider === provider && failure.model === model ? failure.message : null };
}
