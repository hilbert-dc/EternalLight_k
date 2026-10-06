import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi } from "./types";
type Connections = Awaited<ReturnType<NovelCodexApi["agent"]["listConnections"]>>["connections"];
type Status = Awaited<ReturnType<NovelCodexApi["agent"]["configure"]>>;
type Mode = "verify" | "apply" | "verifyAndApply";
export type ConnectionProgress = { scope: string; provider: string; model: string; startedAt: number;
  phase: "verifying" | "reading" | "applying" | "ready" | "failed"; error: string | null; mode: Mode };

export function useModelConnection({ api, scope, blocked, onStatus, onConnections }: {
  api: NovelCodexApi | undefined; scope: string; blocked: boolean;
  onStatus: (status: Status) => void; onConnections: (connections: Connections) => void;
}) {
  const [snapshot, setSnapshot] = useState<{ owner: object; progress: ConnectionProgress } | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef<object | null>(null);
  const current = useRef({ api, scope, blocked, onStatus, onConnections });
  current.current = { api, scope, blocked, onStatus, onConnections };
  const lifetime = useRef({ api, scope, owner: {} });
  if (lifetime.current.api !== api || lifetime.current.scope !== scope) lifetime.current = { api, scope, owner: {} };
  const owner = lifetime.current.owner;
  const progress = snapshot?.owner === owner ? snapshot.progress : null;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  async function run(provider: string, model: string, mode: Mode) {
    if (!api || pending.current || current.current.blocked) return false;
    const operation = {};
    pending.current = operation;
    const isCurrent = () => mounted.current && lifetime.current.owner === owner && !current.current.blocked;
    const value: ConnectionProgress = { scope, provider, model, mode, startedAt: Date.now(), phase: mode === "apply" ? "applying" : "verifying", error: null };
    const publish = (progress: ConnectionProgress) => setSnapshot({ owner, progress });
    setBusy(true); publish(value);
    try {
      if (mode !== "apply") {
        await api.agent.verifyConnection(provider, model);
        if (!isCurrent()) return false;
        publish({ ...value, phase: "reading" });
        const available = await api.agent.listConnections();
        if (!isCurrent()) return false;
        current.current.onConnections(available.connections);
        if (!available.connections.some(item => item.provider === provider && item.model === model && item.status === "verified")) {
          throw new Error("连接尚未通过创作所需能力验证，请检查模型服务后重试。");
        }
      }
      if (mode !== "verify") {
        if (!isCurrent()) return false;
        publish({ ...value, phase: "applying" });
        const status = await api.agent.configure(provider, model);
        if (!isCurrent()) return false;
        current.current.onStatus(status);
      }
      if (isCurrent()) publish({ ...value, phase: "ready" });
      return isCurrent();
    } catch (reason) {
      if (isCurrent()) publish({ ...value, phase: "failed", error: reason instanceof Error ? reason.message : "连接处理失败，请重试。" });
      return false;
    } finally { pending.current = null; if (mounted.current) setBusy(false); }
  }
  return { busy, progress: progress?.scope === scope ? progress : null, run,
    retry: () => progress?.scope === scope ? run(progress.provider, progress.model, progress.mode) : Promise.resolve(false) };
}
