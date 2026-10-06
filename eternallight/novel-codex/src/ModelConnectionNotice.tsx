import { useEffect, useState } from "react";
import type { ConnectionProgress } from "./useModelConnection";
import "./model-connection.css";
const phases = { verifying: "正在验证账号与模型能力", reading: "正在读取验证结果", applying: "正在应用模型连接", ready: "模型连接已就绪", failed: "模型连接未完成" };
export function ModelConnectionNotice({ progress, busy, onRetry }: { progress: ConnectionProgress | null; busy: boolean; onRetry: () => void }) {
  const [, tick] = useState(0);
  useEffect(() => { if (!busy) return; const timer = window.setInterval(() => tick(value => value + 1), 1000); return () => window.clearInterval(timer); }, [busy]);
  if (!progress) return busy ? <p className="model-connection-notice" role="status">上一项连接检查正在结束，请稍候。</p> : null;
  return <div className="model-connection-notice" role={progress.phase === "failed" ? "alert" : "status"} aria-label="模型连接状态">
    <strong>{progress.provider} · {progress.model}</strong>
    <p>{progress.phase === "ready" && progress.mode === "verify" ? "连接已验证，可以应用配置。" : phases[progress.phase]}{busy && ` · 已等待 ${Math.floor((Date.now() - progress.startedAt) / 1000)} 秒`}</p>
    {progress.error && <p>{progress.error}</p>}
    {progress.phase === "verifying" && <small>正在请求模型服务，创作草稿会保留。</small>}
    {progress.phase === "failed" && <button className="secondary-button" type="button" disabled={busy} onClick={onRetry}>重试此连接</button>}
  </div>;
}
