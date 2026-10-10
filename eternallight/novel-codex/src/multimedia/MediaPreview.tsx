import { useEffect, useRef, useState } from "react";
import type { MediaAsset, MediaRect } from "../../shared/multimedia-types";
import { boundedRect } from "./editor-helpers";

/** Coordinates are in the original image, never the CSS-scaled preview. */
export function MediaPreview({ asset, region, onRegionChange, disabled = false }: {
  asset: MediaAsset; region?: MediaRect | null; onRegionChange?: (region: MediaRect | null) => void; disabled?: boolean;
}) {
  const [selecting, setSelecting] = useState(false);
  const [draft, setDraft] = useState<MediaRect | null>(null);
  const [failed, setFailed] = useState(false);
  const image = useRef<HTMLImageElement>(null);
  const pointer = useRef<{ id: number; x: number; y: number } | null>(null);
  useEffect(() => { setSelecting(false); setDraft(null); setFailed(false); pointer.current = null; }, [asset.assetRevisionId]);
  const width = asset.width ?? 1, height = asset.height ?? 1;
  const selected = draft ?? region;
  function position(event: React.PointerEvent) {
    const bounds = image.current?.getBoundingClientRect();
    if (!bounds?.width || !bounds.height) return null;
    return { x: Math.max(0, Math.min(width, (event.clientX - bounds.left) / bounds.width * width)), y: Math.max(0, Math.min(height, (event.clientY - bounds.top) / bounds.height * height)) };
  }
  if (failed) return <p className="mm-notice" role="alert">无法预览“{asset.name}”。素材仍保留，可重新读取或替换。</p>;
  if (asset.kind === "video") return <video className="mm-native-media" src={asset.previewUrl} controls preload="metadata" aria-label={asset.name} onError={() => setFailed(true)} />;
  if (asset.kind === "audio") return <audio className="mm-native-media" src={asset.previewUrl} controls preload="metadata" aria-label={asset.name} onError={() => setFailed(true)} />;
  return <div className="mm-image-preview">
    <div className={`mm-image-stage${selecting ? " selecting" : ""}`} style={{ aspectRatio: `${width} / ${height}` }}
      onPointerDown={event => { if (!selecting || disabled || event.button !== 0) return; const point = position(event); if (!point) return; event.preventDefault(); event.currentTarget.setPointerCapture(event.pointerId); pointer.current = { id: event.pointerId, ...point }; setDraft({ ...point, width: 1, height: 1 }); }}
      onPointerMove={event => { const start = pointer.current, point = position(event); if (!start || start.id !== event.pointerId || !point) return; setDraft(boundedRect({ x: Math.min(start.x, point.x), y: Math.min(start.y, point.y), width: Math.abs(point.x - start.x), height: Math.abs(point.y - start.y) }, width, height)); }}
      onPointerUp={event => { const start = pointer.current, point = position(event); if (!start || start.id !== event.pointerId || !point) return; const next = boundedRect({ x: Math.min(start.x, point.x), y: Math.min(start.y, point.y), width: Math.abs(point.x - start.x), height: Math.abs(point.y - start.y) }, width, height); pointer.current = null; setDraft(null); onRegionChange?.(next); }}
      onPointerCancel={() => { pointer.current = null; setDraft(null); }}>
      <img ref={image} src={asset.previewUrl} alt={asset.name} draggable={false} onError={() => setFailed(true)} />
      {selected && <div className="mm-image-region" aria-label="已选择的图片区域" style={{ left: `${selected.x / width * 100}%`, top: `${selected.y / height * 100}%`, width: `${selected.width / width * 100}%`, height: `${selected.height / height * 100}%` }} />}
    </div>
    {onRegionChange && <div className="mm-preview-controls"><button type="button" disabled={disabled} aria-pressed={selecting} onClick={() => { setSelecting(value => !value); if (!region) onRegionChange(boundedRect({ x: width / 4, y: height / 4, width: width / 2, height: height / 2 }, width, height)); }}>{selecting ? "结束选区" : "选择图片区域"}</button>{region && <button type="button" disabled={disabled} onClick={() => { onRegionChange(null); setSelecting(false); }}>清除选区</button>}</div>}
    {onRegionChange && selecting && region && <fieldset className="mm-region-fields" disabled={disabled}><legend>图片选区</legend>{(["x", "y", "width", "height"] as const).map((key, index) => <label key={key}><span>{["横向位置", "纵向位置", "宽度", "高度"][index]}</span><input type="number" min={key === "width" || key === "height" ? 1 : 0} max={key === "x" || key === "width" ? width : height} value={Math.round(region[key])} onChange={event => { if (event.target.value !== "") onRegionChange(boundedRect({ ...region, [key]: Number(event.target.value) }, width, height)); }} /></label>)}</fieldset>}
  </div>;
}
