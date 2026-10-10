import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { subtitleCaptionError, subtitleFontCoverage } from "../shared/media-subtitle-font.mjs";
import { MEDIA_SUBTITLE_FONT_FAMILY, mediaSubtitleLayout } from "../shared/media-subtitle-layout.mjs";

const MAX_FONT_BYTES = 32 * 1024 * 1024;
const MAX_RASTER_PIXELS = 100_000_000;
const MAX_BITMAPS = 128;
const fail = message => { throw Error(message); };
const escapeHtml = text => text.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]);
const integer = (value, label, positive = false) => {
  if (!Number.isSafeInteger(value) || value < (positive ? 1 : 0)) fail(`${label}必须为有效整数`);
  return BigInt(value);
};
function rationalTime(time) {
  const numerator = integer(time?.value, "字幕时间") * integer(time?.rate?.denominator, "字幕时间分母", true);
  return { numerator, denominator: integer(time?.rate?.numerator, "字幕时间分子", true) };
}
const ceil = (numerator, denominator) => (numerator + denominator - 1n) / denominator;
/** The sample at frame n is visible exactly when start <= n/fps < end. */
export function subtitleFrameRange(start, duration, frameRate) {
  const a = rationalTime(start), b = rationalTime(duration);
  if (!b.numerator) fail("字幕时长必须大于零");
  const fpsN = integer(frameRate?.numerator, "字幕帧率分子", true), fpsD = integer(frameRate?.denominator, "字幕帧率分母", true);
  const first = ceil(a.numerator * fpsN, a.denominator * fpsD);
  const last = ceil((a.numerator * b.denominator + b.numerator * a.denominator) * fpsN, a.denominator * b.denominator * fpsD);
  if (last > 432_000n || first >= last) fail("字幕时间超限或时长未覆盖一个输出帧");
  return { startFrame: Number(first), endFrame: Number(last) };
}

export function inspectSubtitleFont(bytes) {
  return { ...subtitleFontCoverage(bytes), digest: createHash("sha256").update(bytes).digest("hex") };
}

function validateCaption(caption, font, clipId) {
  const error = subtitleCaptionError(caption, font);
  if (error) fail(`片段 ${clipId}：${error}；作品原文已保留`);
}

export async function prepareSubtitlePlan({ document, fontPath, signal }) {
  signal?.throwIfAborted();
  const clips = document.timeline.tracks.filter(track => !track.muted && track.kind === "subtitle")
    .flatMap(track => track.clips).filter(clip => typeof clip.caption !== "string" || clip.caption.trim());
  if (!clips.length) return { states: [], rasters: [], fontDigest: null };
  if (clips.length > 300) fail("字幕片段过多");
  if (!isAbsolute(fontPath ?? "") || fontPath.includes("\0")) fail("字幕需要明确配置的字体绝对路径；作品内容已保留");
  let file, bytes;
  try {
    file = await open(fontPath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
    const stat = await file.stat();
    if (!stat.isFile() || !stat.size || stat.size > MAX_FONT_BYTES) fail("字幕字体为空、损坏或超过 32 MB");
    bytes = await file.readFile();
  } finally { await file?.close(); }
  signal?.throwIfAborted();
  const font = inspectSubtitleFont(bytes), layout = mediaSubtitleLayout(document.width, document.height), cues = [];
  let characters = 0; const cueIds = new Set();
  for (const clip of clips) {
    if (typeof clip.id !== "string" || !clip.id || clip.id.length > 240 || cueIds.has(clip.id)) fail("字幕片段编号无效或重复");
    cueIds.add(clip.id);
    validateCaption(clip.caption, font, clip.id); characters += [...clip.caption].length;
    if (characters > 100_000) fail("单次导出字幕文本总量过多");
    cues.push({ id: clip.id, text: clip.caption, ...subtitleFrameRange(clip.start, clip.duration, document.frameRate) });
  }
  const boundaries = [...new Set(cues.flatMap(cue => [cue.startFrame, cue.endFrame]))].sort((a, b) => a - b);
  const states = [], rasters = [], cache = new Map();
  for (let index = 0; index < boundaries.length - 1; index++) {
    const startFrame = boundaries[index], endFrame = boundaries[index + 1], active = cues.filter(cue => cue.startFrame <= startFrame && startFrame < cue.endFrame);
    if (!active.length) continue;
    if (active.length > 16) fail("同时显示的字幕过多，请调整时间线；作品内容已保留");
    const key = JSON.stringify(active.map(cue => cue.id)); let raster = cache.get(key);
    if (!raster) {
      if (rasters.length >= MAX_BITMAPS || (rasters.length + 1) * document.width * layout.bandHeight > MAX_RASTER_PIXELS) fail("字幕栅格总量超过本机导出限制，请分段导出；作品内容已保留");
      raster = { id: rasters.length, width: document.width, height: layout.bandHeight, y: document.height - layout.bandHeight,
        nodeIds: active.map((_, position) => `subtitle-${position}`), captions: active.map(cue => cue.text) };
      cache.set(key, raster); rasters.push(raster);
    }
    states.push({ rasterId: raster.id, startFrame, endFrame, clipIds: active.map(cue => cue.id) });
  }
  const fontSource = `data:${font.mimeType};base64,${bytes.toString("base64")}`;
  const htmlForRaster = raster => {
    const rows = raster.captions.map((caption, index) => `<div id="${raster.nodeIds[index]}" class="caption">${escapeHtml(caption)}</div>`).join("");
    return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; font-src data:; style-src 'unsafe-inline'; script-src 'none'"><style>@font-face{font-family:${MEDIA_SUBTITLE_FONT_FAMILY};src:url('${fontSource}');font-display:block}*{box-sizing:border-box}html,body{margin:0;width:${raster.width}px;height:${raster.height}px;background:transparent;overflow:hidden}.subtitles{position:absolute;left:${layout.horizontalMargin}px;right:${layout.horizontalMargin}px;bottom:${layout.bottomMargin}px;display:flex;flex-direction:column;align-items:center;gap:${layout.gap}px;font:${layout.fontSize}px/${layout.lineHeight} ${MEDIA_SUBTITLE_FONT_FAMILY};font-synthesis:none;text-align:center;direction: ltr}.caption{max-width:100%;padding:${layout.paddingY}px ${layout.paddingX}px;background:rgba(0,0,0,.6);color:white;white-space:pre-wrap;overflow-wrap:anywhere;unicode-bidi:plaintext}</style></head><body><div class="subtitles">${rows}</div></body></html>`;
  };
  return { states, rasters, fontDigest: font.digest, layout, htmlForRaster };
}
