import "./read-failure-notice.css";

/** Context comes from the reader; diagnostics never determine permissions or recovery. */
export function ReadFailureNotice({ message, error, retryLabel, onRetry, busy = false }: {
  message: string; error: string; retryLabel: string; onRetry: () => void; busy?: boolean;
}) {
  return <div className="read-failure-notice">
    <p role="alert">{message}</p>
    <button type="button" className="text-button" disabled={busy} onClick={onRetry}>{retryLabel}</button>
    <details><summary>查看详情</summary><pre>{error}</pre></details>
  </div>;
}
