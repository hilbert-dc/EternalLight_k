import type { CreativeEdition } from "../../shared/multimedia-types";
const mediaLabel = (medium: string) => ({ novel: "小说", comic: "漫画", animation: "动画" })[medium] ?? medium;
import "./edition-switcher.css";

export function EditionSwitcher({ editions, currentEditionId, busy, onSelect, onCreate }: {
  editions: CreativeEdition[]; currentEditionId: string | null; busy: boolean;
  onSelect: (edition: CreativeEdition) => void; onCreate: () => void;
}) {
  return <div className="edition-switcher"><select aria-label="作品版本" title={editions.find(item => item.id === currentEditionId)?.title} disabled={busy || !editions.length} value={currentEditionId ?? ""} onChange={event => { const edition = editions.find(item => item.id === event.target.value); if (edition) onSelect(edition); }}>
    {!currentEditionId && <option value="" disabled>选择作品版本</option>}
    {editions.map(edition => <option key={edition.id} value={edition.id}>{editions.length === 1 ? mediaLabel(edition.medium) : `${edition.title} · ${mediaLabel(edition.medium)}`}</option>)}
  </select><button type="button" className="icon-button" aria-label="新建作品版本" title="新建漫画或动画版本" disabled={busy} onClick={onCreate}>＋</button></div>;
}
