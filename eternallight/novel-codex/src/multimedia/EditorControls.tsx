import { useEffect, useState } from "react";
import type { MediaAsset } from "../../shared/multimedia-types";

export function NumberField({ label, value, onChange, min = 0, max, step = 1, disabled }: {
  label: string; value: number; onChange: (value: number) => void; min?: number; max?: number; step?: number; disabled?: boolean;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(Number(value.toFixed(5)))), [value]);
  function commit() { const next = Number(text); if (text.trim() && Number.isFinite(next) && next >= min && (max === undefined || next <= max)) onChange(next); else setText(String(value)); }
  return <label className="mm-number-field"><span>{label}</span><input type="number" value={text} min={min} max={max} step={step} disabled={disabled} onChange={event => setText(event.target.value)} onBlur={commit} onKeyDown={event => { if (event.key === "Enter") { event.preventDefault(); commit(); } }} /></label>;
}

export function AssetChoice({ assets, value, onChange, label = "画面素材", disabled, kinds }: {
  assets: readonly MediaAsset[]; value: string | null; onChange: (id: string | null) => void; label?: string; disabled?: boolean; kinds: MediaAsset["kind"][];
}) {
  return <label className="mm-field"><span>{label}</span><select value={value ?? ""} disabled={disabled} onChange={event => onChange(event.target.value || null)}><option value="">暂未选择</option>{assets.filter(asset => kinds.includes(asset.kind)).map(asset => <option key={asset.assetRevisionId} value={asset.assetRevisionId}>{asset.name}</option>)}</select></label>;
}

export function AssetReferences({ assets, value, onChange }: { assets: readonly MediaAsset[]; value: readonly string[]; onChange: (value: string[]) => void }) {
  const images = assets.filter(asset => asset.kind === "image");
  if (!images.length) return null;
  return <details className="mm-reference-assets"><summary>角色与场景参考{value.length ? ` · ${value.length}` : ""}</summary>{images.map(asset => <label key={asset.assetRevisionId}><input type="checkbox" checked={value.includes(asset.assetRevisionId)} onChange={event => onChange(event.target.checked ? [...new Set([...value, asset.assetRevisionId])] : value.filter(id => id !== asset.assetRevisionId))} /><span>{asset.name}</span></label>)}</details>;
}
