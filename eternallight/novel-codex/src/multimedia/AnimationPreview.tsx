import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { AnimationClip, AnimationDocument, MediaAsset, MultimediaApi } from "../../shared/multimedia-types";
import { mediaSubtitleLayout } from "../../shared/media-subtitle-layout.mjs";
import { subtitleCaptionError, subtitleFontCoverage } from "../../shared/media-subtitle-font.mjs";
import { readMediaView, saveMediaView } from "./view-state";
import { animationLength, clipEnd, formatTime, timeSeconds } from "./editor-helpers";

function PlayingMedia({ asset, clip, time, playing }: { asset: MediaAsset; clip: AnimationClip; time: number; playing: boolean }) {
  const media = useRef<HTMLMediaElement | null>(null);
  const sourceTime = timeSeconds(clip.sourceStart) + (time - timeSeconds(clip.start)) * clip.playbackRate.numerator / clip.playbackRate.denominator;
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const node = media.current; if (!node) return;
    node.playbackRate = clip.playbackRate.numerator / clip.playbackRate.denominator;
    const target = Math.max(0, sourceTime);
    if (Number.isFinite(target) && Math.abs(node.currentTime - target) > .12) node.currentTime = target;
    if (playing) void node.play().catch(() => setError("点击播放区域以允许声音播放。")); else node.pause();
  }, [sourceTime, playing, clip.playbackRate]);
  useEffect(() => () => { media.current?.pause(); }, []);
  if (asset.kind === "image") return <img src={asset.previewUrl} alt={asset.name} />;
  return <>{asset.kind === "video" ? <video ref={node => { media.current = node; }} src={asset.previewUrl} playsInline preload="auto" aria-label={asset.name} onError={() => setError(`无法播放“${asset.name}”。`)} /> : <audio ref={node => { media.current = node; }} src={asset.previewUrl} preload="auto" aria-label={asset.name} onError={() => setError(`无法播放“${asset.name}”。`)} />}{error && <span className="mm-playback-error" role="status">{error}</span>}</>;
}

export function AnimationPreview({ document, assets, viewStorageKey, readSubtitleFont }: { document: AnimationDocument; viewStorageKey: string; assets: readonly MediaAsset[]; readSubtitleFont: MultimediaApi["readSubtitleFont"] }) {
  const [time, setTime] = useState(() => { const saved = readMediaView(viewStorageKey).time; return typeof saved === "number" && Number.isFinite(saved) && saved >= 0 ? Math.min(animationLength(document), saved) : 0; }), [playing, setPlaying] = useState(false);
  const duration = animationLength(document);
  const hasSubtitles = document.timeline.tracks.some(track => !track.muted && track.kind === "subtitle" && track.clips.some(clip => clip.caption.trim()));
  const [font, setFont] = useState<{ family: string; coverage: ReturnType<typeof subtitleFontCoverage>; face: FontFace } | null>(null);
  const [fontError, setFontError] = useState<string | null>(null);
  useEffect(() => {
    if (!hasSubtitles) return;
    const documentFonts = window.document.fonts;
    const controller = new AbortController(); let alive = true, face: FontFace | null = null;
    setFont(null); setFontError(null);
    void (async () => {
      const reference = await readSubtitleFont();
      if (!reference) throw Error("字幕字体尚未配置，字幕预览暂不可用。字幕与时间轴已保留。");
      if (!/^[a-f0-9]{64}$/.test(reference.digest) || !reference.previewUrl.startsWith("creative-media://")) throw Error("字幕字体引用无效。");
      const response = await fetch(reference.previewUrl, { signal: controller.signal });
      if (!response.ok || Number(response.headers.get("content-length")) > 32 * 1024 * 1024) throw Error("字幕字体尚未读取。");
      const bytes = await response.arrayBuffer();
      const digest = [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map(value => value.toString(16).padStart(2, "0")).join("");
      if (digest !== reference.digest) throw Error("字幕字体内容已变化，预览暂不可用。");
      const coverage = subtitleFontCoverage(new Uint8Array(bytes)), family = `MediaSubtitleFont_${digest}`;
      face = await new FontFace(family, bytes).load();
      if (alive) { documentFonts.add(face); setFont({ family, coverage, face }); }
    })().catch(reason => { if (alive) setFontError(reason instanceof Error ? reason.message : "字幕字体暂不可用。"); });
    return () => { alive = false; controller.abort(); if (face) documentFonts.delete(face); };
  }, [readSubtitleFont, hasSubtitles]);
  const position = useRef(0); position.current = time;
  useEffect(() => { const timer = window.setTimeout(() => saveMediaView(viewStorageKey, { time }), 300); return () => clearTimeout(timer); }, [time, viewStorageKey]);
  useEffect(() => () => { saveMediaView(viewStorageKey, { time: position.current }); }, [viewStorageKey]);
  useEffect(() => { if (time > duration) setTime(duration); }, [duration, time]);
  useEffect(() => {
    if (!playing) return;
    let frame = 0, last = performance.now();
    const step = (now: number) => { const next = Math.min(duration, position.current + (now - last) / 1000); last = now; position.current = next; setTime(next); if (next >= duration) { setPlaying(false); return; } frame = requestAnimationFrame(step); };
    frame = requestAnimationFrame(step); return () => cancelAnimationFrame(frame);
  }, [playing, duration]);
  const active = document.timeline.tracks.filter(track => !track.muted).flatMap(track => track.clips.filter(clip => time >= timeSeconds(clip.start) && time < clipEnd(clip)).map(clip => ({ track, clip, asset: assets.find(asset => asset.assetRevisionId === (clip.assetRevisionId ?? document.shots.find(shot => shot.id === clip.shotId)?.assetRevisionId)) })));
  const video = active.filter(item => item.track.kind === "video");
  const captions = active.filter(item => item.track.kind === "subtitle" && item.clip.caption.trim());
  const fontReady = font && window.document.fonts.has(font.face) && font.face.status === "loaded";
  const captionError = fontReady && captions.map(item => subtitleCaptionError(item.clip.caption, font.coverage)).find(Boolean);
  const layout = mediaSubtitleLayout(document.width, document.height);
  const subtitleBox = useRef<HTMLDivElement>(null), subtitleBand = useRef<HTMLDivElement>(null);
  const [layoutOverflow, setLayoutOverflow] = useState(false);
  const captionKey = JSON.stringify(captions.map(item => [item.clip.id, item.clip.caption]));
  useLayoutEffect(() => {
    const box = subtitleBox.current?.getBoundingClientRect(), band = subtitleBand.current?.getBoundingClientRect();
    setLayoutOverflow(captions.length > 16 || Boolean(box && band && (box.top < band.top - .5 || box.left < band.left - .5 || box.right > band.right + .5)));
  }, [captionKey, font, document.width, document.height]);
  return <section className="mm-animation-player" aria-label="动画预览">
    <div className="mm-animation-screen" style={{ aspectRatio: `${document.width}/${document.height}` }}>
      {video.length ? video.map(item => item.asset ? <PlayingMedia key={item.clip.id} asset={item.asset} clip={item.clip} time={time} playing={playing} /> : <span key={item.clip.id} className="mm-playback-error">这个片段尚未选择画面。</span>) : <span className="mm-preview-empty">{duration ? "当前时间没有画面" : "把镜头加入时间轴即可预览"}</span>}
      {active.filter(item => item.track.kind === "audio" && item.asset).map(item => <PlayingMedia key={item.clip.id} asset={item.asset!} clip={item.clip} time={time} playing={playing} />)}
      {fontReady && !captionError && <svg className="mm-subtitles-layer" style={{ visibility: layoutOverflow ? "hidden" : "visible" }} viewBox={`0 0 ${document.width} ${document.height}`} aria-label="字幕预览"><foreignObject x="0" y={document.height - layout.bandHeight} width={document.width} height={layout.bandHeight}>
        <div ref={subtitleBand} style={{ position: "relative", width: document.width, height: layout.bandHeight }}><div ref={subtitleBox} style={{ position: "absolute", left: layout.horizontalMargin, right: layout.horizontalMargin, bottom: layout.bottomMargin, display: "flex", flexDirection: "column", alignItems: "center", gap: layout.gap, textAlign: "center", fontFamily: font.family, fontSize: layout.fontSize, lineHeight: layout.lineHeight, fontSynthesis: "none" }}>
          {captions.map(item => <div className="mm-preview-caption" key={item.clip.id} style={{ maxWidth: "100%", padding: `${layout.paddingY}px ${layout.paddingX}px`, background: "rgba(0,0,0,.6)", color: "white", whiteSpace: "pre-wrap", overflowWrap: "anywhere", unicodeBidi: "plaintext" }}>{item.clip.caption}</div>)}
        </div></div>
      </foreignObject></svg>}
    </div>
    {hasSubtitles && (!fontReady || captionError || layoutOverflow) && <p className="mm-preview-font-status" role="status">{fontError || captionError || (layoutOverflow ? "字幕超出画面安全区域，请拆分字幕或调整时间线。" : "正在读取字幕字体…")}</p>}
    <div className="mm-playback-controls"><button type="button" disabled={!duration} onClick={() => { if (time >= duration) { position.current = 0; setTime(0); } setPlaying(value => !value); }}>{playing ? "暂停预览" : "播放预览"}</button><input aria-label="播放位置" type="range" min={0} max={duration || 1} step={document.frameRate.denominator / document.frameRate.numerator} value={time} disabled={!duration} onChange={event => { setTime(Number(event.target.value)); }} /><output>{formatTime(time)} / {formatTime(duration)}</output></div>
  </section>;
}
