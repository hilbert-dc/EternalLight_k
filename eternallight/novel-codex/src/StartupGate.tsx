import { useEffect, useRef, useState, type ReactNode } from "react";
import type { RuntimeStatus } from "./types";
import "./startup-gate.css";

const starting: RuntimeStatus = { state: "starting", mode: "local", error: null };

export default function StartupGate({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<RuntimeStatus>(starting);
  const [ready, setReady] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const readyRef = useRef(false);
  const retryRef = useRef(false);
  const request = useRef(0);

  // Once the workspace has opened, its own connection and dirty-edit guards
  // own recovery. Startup must never replace that mounted editor.
  const receive = (next: RuntimeStatus, sequence: number) => {
    if (readyRef.current || sequence !== request.current) return;
    setStatus(next);
    if (next.state === "online") {
      readyRef.current = true;
      setReady(true);
    }
  };

  useEffect(() => {
    if (ready) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      const sequence = ++request.current;
      try {
        if (!window.novelCodex) throw Error("当前窗口未连接应用服务。");
        const next = await window.novelCodex.runtime.status();
        if (active) receive(next, sequence);
      } catch (error) {
        if (active) receive({ state: "offline", mode: "local", error: String(error) }, sequence);
      }
      if (active && !readyRef.current) timer = setTimeout(poll, 1000);
    };
    void poll();
    return () => { active = false; ++request.current; clearTimeout(timer); };
  }, [ready]);

  useEffect(() => {
    if (ready) return;
    const api = window.novelCodex;
    return api?.app.onCloseRequested(() => { if (!readyRef.current) void api.app.confirmClose(); });
  }, [ready]);

  const retry = async () => {
    if (retryRef.current || readyRef.current) return;
    retryRef.current = true;
    setRetrying(true);
    setStatus(starting);
    const sequence = ++request.current;
    try {
      if (!window.novelCodex) throw Error("当前窗口未连接应用服务。");
      receive(await window.novelCodex.runtime.retryStartup(), sequence);
    } catch (error) {
      receive({ state: "offline", mode: "local", error: String(error) }, sequence);
    } finally {
      retryRef.current = false;
      setRetrying(false);
    }
  };

  if (ready) return children;
  const offline = status.state === "offline";
  return <main className="startup-gate" aria-label="应用启动">
    <div className="startup-gate-content">
      <p role="status" aria-live="polite">
        {!offline && <span className="startup-gate-dot" aria-hidden="true" />}
        {offline ? "启动未完成，请重试。" : status.state === "slow" ? "启动较慢，仍在等待…" : "正在启动小说工坊…"}
      </p>
      {offline && <button className="startup-gate-retry" onClick={() => void retry()} disabled={retrying}>重新启动</button>}
      {offline && status.error && <details className="startup-gate-details">
        <summary>查看详情</summary>
        <pre>{status.error}</pre>
      </details>}
    </div>
  </main>;
}
