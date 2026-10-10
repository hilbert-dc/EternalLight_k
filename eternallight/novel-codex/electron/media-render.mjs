import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, mkdtemp, writeFile, copyFile, rm } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { probeMediaFile } from "./media-files.mjs";
import { prepareSubtitlePlan } from "./media-subtitles.mjs";

const execute = promisify(execFile);
const escapeHtml = text => String(text).replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]);
function number(value, label, max = 100_000) { if (!Number.isFinite(value) || value < 0 || value > max) throw Error(`${label}无效`); return value; }
function positive(value, label, max) { number(value, label, max); if (!value) throw Error(`${label}必须大于零`); return value; }
const rectStyle = rect => `left:${number(rect.x, "坐标")}px;top:${number(rect.y, "坐标")}px;width:${positive(rect.width,"宽度")}px;height:${positive(rect.height,"高度")}px`;
export function mediaTimeSeconds(time) {
  if(![time?.value,time?.rate?.numerator,time?.rate?.denominator].every(Number.isSafeInteger))throw Error("媒体时间需要整数刻度与有理数速率");
  return number(time.value, "时间值", Number.MAX_SAFE_INTEGER) * positive(time.rate.denominator, "时间分母", 1e9) / positive(time.rate.numerator, "时间分子", 1e9);
}
export async function renderComicDocument({ document, pageIds, format, loadAsset, renderHtml, signal }) {
  if (document?.kind !== "comic" || !["png", "pdf"].includes(format)) throw Error("漫画导出格式无效");
  if (pageIds !== undefined && (!Array.isArray(pageIds) || !pageIds.length || new Set(pageIds).size !== pageIds.length || pageIds.some(id => !document.pages.some(page => page.id === id)))) throw Error("导出页面无效");
  const pages = document.pages.filter(page => !pageIds || pageIds.includes(page.id));
  if (!pages.length || pages.length > 200) throw Error("导出页数无效");
  for (const page of pages) { positive(page.width,"页面宽度",16000); positive(page.height,"页面高度",16000); if (page.width * page.height > 100_000_000) throw Error("页面尺寸过大"); }
  if (format === "pdf" && pages.some(page => page.width !== pages[0].width || page.height !== pages[0].height)) throw Error("多页 PDF 需要统一页面尺寸，请调整尺寸或分别导出 PNG");
  const cache = new Map();
  const pageHtml = async page => {
    let html = `<section class="page" style="width:${page.width}px;height:${page.height}px">`;
    for (const panel of page.panels) {
      html += `<div class="panel" style="${rectStyle(panel.rect)}">`;
      if (panel.assetRevisionId) {
        if (!cache.has(panel.assetRevisionId)) {
          const asset = await loadAsset(panel.assetRevisionId);
          if (!/^image\/(png|jpeg|webp|gif)$/.test(asset.mimeType) || !(asset.bytes instanceof Uint8Array) || asset.bytes.length > 64 * 1024 * 1024) throw Error("漫画画面需要有效图片素材");
          cache.set(panel.assetRevisionId, `data:${asset.mimeType};base64,${Buffer.from(asset.bytes).toString("base64")}`);
        }
        html += `<img alt="" src="${cache.get(panel.assetRevisionId)}">`;
      }
      for (const balloon of panel.balloons) html += `<div class="balloon ${["speech","thought","caption","sound"].includes(balloon.kind) ? balloon.kind : "speech"}" style="${rectStyle(balloon.rect)}">${escapeHtml(balloon.text)}</div>`;
      html += "</div>";
    }
    return html + "</section>";
  };
  const wrap = (body, page) => `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'"><style>@page{size:${page.width}px ${page.height}px;margin:0}*{box-sizing:border-box}html,body{margin:0;padding:0;background:white}.page{position:relative;overflow:hidden;break-after:page}.page:last-child{break-after:auto}.panel{position:absolute;overflow:hidden;border:1px solid #222}.panel img{width:100%;height:100%;object-fit:cover}.balloon{position:absolute;padding:6px;display:flex;align-items:center;justify-content:center;white-space:pre-wrap;overflow:hidden;overflow-wrap:anywhere;background:white;border:1px solid #222;border-radius:50%;font:18px sans-serif}.caption{border-radius:0}.sound{background:transparent;border:0;font-weight:bold}.thought{border-style:dashed}</style></head><body>${body}</body></html>`;
  if (format === "pdf") {
    const content = []; for (const page of pages) { signal?.throwIfAborted(); content.push(await pageHtml(page)); }
    return [{ name: "comic.pdf", mimeType: "application/pdf", bytes: await renderHtml({ html: wrap(content.join(""),pages[0]), width: pages[0].width, height: pages[0].height, format, signal }) }];
  }
  const output = [];
  for (let index = 0; index < pages.length; index++) { signal?.throwIfAborted(); const page = pages[index]; output.push({ name:`page-${String(index+1).padStart(3,"0")}.png`, mimeType:"image/png", bytes:await renderHtml({html:wrap(await pageHtml(page),page),width:page.width,height:page.height,format,signal}) }); }
  return output;
}
function tempoFilters(rate) { const values = []; while (rate > 2) { values.push("atempo=2"); rate /= 2; } while (rate < 0.5) { values.push("atempo=0.5"); rate /= 0.5; } values.push(`atempo=${rate}`); return values.join(","); }
export async function renderAnimationDocument({ document, resolveAsset, ffmpegPath, ffprobePath, fontPath = null, renderHtml, processEnvironment, temporaryDirectory, signal }) {
  signal?.throwIfAborted();
  if (document?.kind !== "animation" || !isAbsolute(ffmpegPath ?? "") || !isAbsolute(ffprobePath ?? "")) throw Error("视频渲染组件尚未配置");
  if (fontPath !== null && (!isAbsolute(fontPath) || fontPath.includes("\0"))) throw Error("字幕字体必须使用明确的绝对路径");
  const width = positive(document.width,"视频宽度",8192), height = positive(document.height,"视频高度",8192);
  if (!Number.isInteger(width) || !Number.isInteger(height) || width%2 || height%2) throw Error("MP4 宽高需要为偶数整数");
  const fps = positive(document.frameRate?.numerator,"帧率分子",240000) / positive(document.frameRate?.denominator,"帧率分母",10000);
  if (![document.frameRate.numerator, document.frameRate.denominator].every(Number.isSafeInteger)) throw Error("视频帧率需要整数有理数");
  const fpsExpression = `${document.frameRate.numerator}/${document.frameRate.denominator}`;
  if (fps < 1 || fps > 120) throw Error("视频帧率须在 1–120 之间");
  const clips = document.timeline.tracks.filter(track => !track.muted).flatMap(track => track.clips.map(clip => ({ ...clip, trackKind: track.kind })));
  if (!clips.length || clips.length > 300) throw Error("时间线为空或片段过多");
  let duration = 0;
  for (const clip of clips) {
    clip.startSeconds = mediaTimeSeconds(clip.start); clip.durationSeconds = mediaTimeSeconds(clip.duration); clip.sourceSeconds = mediaTimeSeconds(clip.sourceStart);
    clip.speed = positive(clip.playbackRate?.numerator,"播放速率",1000) / positive(clip.playbackRate?.denominator,"播放速率",1000);
    if (!clip.durationSeconds || clip.speed < 0.0625 || clip.speed > 16) throw Error("片段时长或播放速率无效");
    duration = Math.max(duration, clip.startSeconds + clip.durationSeconds);
  }
  if (duration > 3600) throw Error("单次本机导出最多一小时");
  const subtitles = await prepareSubtitlePlan({ document, fontPath, signal });
  if (subtitles.rasters.length && typeof renderHtml !== "function") throw Error("字幕页面渲染器尚未配置；作品原文已保留");
  await mkdir(temporaryDirectory, {recursive:true,mode:0o700}); const directory = await mkdtemp(join(temporaryDirectory,"render-"));
  const cleanup = () => rm(directory,{recursive:true,force:true});
  try {
    const args = ["-hide_banner","-loglevel","error","-nostdin","-y","-f","lavfi","-i",`color=c=black:s=${width}x${height}:r=${fpsExpression}:d=${duration}`];
    const filters = []; let visual = "0:v", inputIndex = 1; const audio = [];
    for (let index=0; index<clips.length; index++) {
      signal?.throwIfAborted(); const clip=clips[index];
      if (clip.trackKind === "subtitle") {
        continue;
      }
      const revisionId=clip.assetRevisionId ?? document.shots.find(shot=>shot.id===clip.shotId)?.assetRevisionId;
      if (!revisionId) throw Error("片段尚未绑定素材");
      const asset = await resolveAsset(revisionId);
      if (!isAbsolute(asset.path ?? "") || !["image","video","audio"].includes(asset.kind)) throw Error("时间线素材无效");
      if (clip.trackKind === "video" && asset.kind === "audio" || clip.trackKind === "audio" && asset.kind === "image") throw Error("素材与轨道类型不匹配");
      if (asset.kind !== "image") { const available=mediaTimeSeconds(asset.duration); if (clip.sourceSeconds+clip.durationSeconds*clip.speed > available+0.05) throw Error("片段超出素材可用时长"); }
      const source=join(directory,`input-${index}`); await copyFile(asset.path,source);
      if (asset.kind === "image") args.push("-loop","1");
      args.push("-protocol_whitelist","file,pipe","-i",source); const input=inputIndex++;
      if (clip.trackKind === "video") {
        filters.push(`[${input}:v]trim=start=${clip.sourceSeconds}:duration=${clip.durationSeconds*clip.speed},setpts=(PTS-STARTPTS)/${clip.speed}+${clip.startSeconds}/TB,scale=${width}:${height}:force_original_aspect_ratio=decrease,pad=${width}:${height}:(ow-iw)/2:(oh-ih)/2,setsar=1[clip${index}]`);
        filters.push(`[${visual}][clip${index}]overlay=eof_action=pass:enable='gte(t,${clip.startSeconds})*lt(t,${clip.startSeconds+clip.durationSeconds})'[v${index}]`);visual=`v${index}`;
      } else {
        filters.push(`[${input}:a]atrim=start=${clip.sourceSeconds}:duration=${clip.durationSeconds*clip.speed},asetpts=PTS-STARTPTS,${tempoFilters(clip.speed)},adelay=${Math.round(clip.startSeconds*1000)}:all=1[a${index}]`);audio.push(`a${index}`);
      }
    }
    const subtitleEvidence = []; let subtitleBytes = 0;
    for (const raster of subtitles.rasters) {
      signal?.throwIfAborted();
      const rendered = await renderHtml({ html: subtitles.htmlForRaster(raster), width: raster.width, height: raster.height,
        format: "png", purpose: "subtitle", subtitleNodeIds: raster.nodeIds, signal });
      if (!(rendered?.bytes instanceof Uint8Array) || !rendered.bytes.length || rendered.bytes.length > 16 * 1024 * 1024
        || !Array.isArray(rendered.fontEvidence) || rendered.fontEvidence.length !== raster.nodeIds.length
        || rendered.fontEvidence.some((record, index) => record.nodeId !== raster.nodeIds[index] || !record.fonts?.length
          || record.fonts.some(font => font.isCustomFont !== true || !Number.isInteger(font.glyphCount) || font.glyphCount <= 0))) throw Error("字幕渲染缺少实际固定字体证据");
      const bitmap = Buffer.from(rendered.bytes);
      if (bitmap.length < 33 || !bitmap.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))
        || bitmap.toString("ascii", 12, 16) !== "IHDR" || bitmap.readUInt32BE(16) !== raster.width || bitmap.readUInt32BE(20) !== raster.height
        || ![4, 6].includes(bitmap[25])) throw Error("字幕图片尺寸、格式或透明通道无效");
      subtitleBytes += rendered.bytes.length; if (subtitleBytes > 64 * 1024 * 1024) throw Error("字幕图片总量超过本机导出限制");
      const path = join(directory, `subtitle-${raster.id}.png`); await writeFile(path, rendered.bytes, { mode: 0o600 });
      args.push("-loop", "1", "-framerate", fpsExpression, "-protocol_whitelist", "file,pipe", "-i", path);
      const input = inputIndex++, periods = subtitles.states.filter(state => state.rasterId === raster.id);
      const enabled = periods.map(state => `gte(n,${state.startFrame})*lt(n,${state.endFrame})`).join("+");
      filters.push(`[${visual}][${input}:v]overlay=0:${raster.y}:alpha=straight:eof_action=pass:enable='${enabled}'[subtitle${raster.id}]`);
      visual = `subtitle${raster.id}`;
      subtitleEvidence.push({ rasterId: raster.id, fontDigest: subtitles.fontDigest, periods: periods.map(({ startFrame, endFrame, clipIds }) => ({ startFrame, endFrame, clipIds })), fontEvidence: rendered.fontEvidence });
    }
    if (audio.length) filters.push(`${audio.map(label=>`[${label}]`).join("")}amix=inputs=${audio.length}:duration=longest:normalize=0[mixed]`);
    if (filters.length) args.push("-filter_complex",filters.join(";"));
    args.push("-map",visual==="0:v"?"0:v":`[${visual}]`); if(audio.length) args.push("-map","[mixed]","-c:a","aac","-b:a","192k");
    const path=join(directory,"animation.mp4"); args.push("-t",String(duration),"-r",fpsExpression,"-c:v","libx264","-pix_fmt","yuv420p","-fs",String(512*1024*1024),"-movflags","+faststart",path);
    await execute(ffmpegPath,args,{cwd:directory,signal,env:processEnvironment,timeout:30*60*1000,maxBuffer:1024*1024,windowsHide:true});
    const metadata=await probeMediaFile(path,{ffprobePath,signal,processEnvironment});
    if(mediaTimeSeconds(metadata.duration)+1/fps<duration)throw Error("导出结果超过文件限制或未完整生成，请缩短片段后重试");
    return {path,metadata,cleanup,subtitleEvidence};
  } catch(error) { await cleanup(); if(signal?.aborted) throw signal.reason; throw error; }
}
