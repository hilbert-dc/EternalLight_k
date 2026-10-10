import { useEffect, useRef, useState } from "react";
import type { RuntimeStatus } from "./types";
import "./runtime-connection-status.css";

export function RuntimeConnectionStatus({ status, onStatus, retryStartup }: {
  status: RuntimeStatus;
  onStatus: (status: RuntimeStatus) => void;
  retryStartup?: () => Promise<RuntimeStatus>;
}) {
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const current = useRef(status);
  const mounted = useRef(true);
  current.current = status;
  const isOnline = () => current.current.state === "online";
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const reconnect = async () => {
    if (!retryStartup || pendingRef.current || current.current.state !== "offline") return;
    pendingRef.current = true;
    setPending(true);
    onStatus({ state: "starting", mode: "local", error: null });
    try {
      const next = await retryStartup();
      if (mounted.current && !isOnline()) onStatus(next);
    } catch {
      if (mounted.current && !isOnline()) {
        onStatus({ state: "offline", mode: "local", error: "本地服务重新连接失败，可以重试。" });
      }
    } finally {
      pendingRef.current = false;
      if (mounted.current) setPending(false);
    }
  };

  const online = status.state === "online";
  const waiting = pending || status.state === "starting" || status.state === "slow";
  return <div className="runtime-card runtime-connection-status" role="status" aria-live="polite">
    <span className={`online-dot${online ? "" : " offline"}`} aria-hidden="true" />
    <span>创作运行时</span>
    <span className="runtime-connection-label">{online ? "本地在线" : waiting ? status.state === "slow" ? "仍在等待…" : "连接中…" : "未连接"}</span>
    {!online && !waiting && <button type="button" disabled={!retryStartup} onClick={() => void reconnect()}>重新连接</button>}
  </div>;
}
