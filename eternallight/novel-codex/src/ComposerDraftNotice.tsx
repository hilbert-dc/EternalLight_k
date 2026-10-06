import "./composer-draft-notice.css";

export function ComposerDraftNotice({ reason }: { reason: string | null }) {
  if (!reason) return null;
  return <p id="composer-draft-notice" className="composer-draft-notice" role="status" aria-live="polite">{reason}</p>;
}
