const { app, BrowserWindow, session } = require("electron");
const { execFile } = require("node:child_process");
const { promisify } = require("node:util");
const { mkdirSync, readFileSync } = require("node:fs");
const { isAbsolute, join } = require("node:path");
const { pathToFileURL } = require("node:url");

const configuration = JSON.parse(readFileSync(0, "utf8"));
for (const key of ["directory", "ffmpegPath", "ffprobePath", "fontPath"]) {
  if (!isAbsolute(configuration[key] ?? "") || configuration[key].includes("\0")) throw Error("媒体实测路径必须明确");
}
if (process.env.PATH !== "" || configuration.platform !== process.platform || configuration.architecture !== process.arch) throw Error("媒体实测系统或空 PATH 环境不一致");
mkdirSync(join(configuration.directory, "electron-profile"), { recursive: true, mode: 0o700 });
app.setPath("userData", join(configuration.directory, "electron-profile"));
app.commandLine.appendSwitch("disable-gpu");
app.on("window-all-closed", () => {});

app.whenReady().then(async () => {
  const execute = promisify(execFile);
  const options = { env: process.env, timeout: 30_000, maxBuffer: 4 * 1024 * 1024, windowsHide: true };
  const { renderAnimationDocument } = await import(pathToFileURL(join(__dirname, "../electron/media-render.mjs")));
  const { renderMediaHtml } = await import(pathToFileURL(join(__dirname, "../electron/media-html-renderer.mjs")));
  const time = value => ({ value, rate: { numerator: 1000, denominator: 1 } });
  const clip = (id, start, duration, caption = "", assetRevisionId = null) => ({ id, shotId: null, assetRevisionId, start: time(start), duration: time(duration), sourceStart: time(0), playbackRate: { numerator: 1, denominator: 1 }, caption });
  const audioPath = join(configuration.directory, "tone.wav");
  await execute(configuration.ffmpegPath, ["-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", audioPath], options);
  const document = { kind: "animation", title: "固定字体字幕验收", width: 512, height: 256, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [
    { id: "captions", kind: "subtitle", muted: false, clips: [clip("caption", 250, 500, "Comic animation 验收")] },
    { id: "audio", kind: "audio", muted: false, clips: [clip("sound", 0, 1000, "", "tone")] },
  ] } };
  const output = await renderAnimationDocument({ document, ...configuration, temporaryDirectory: configuration.directory,
    processEnvironment: process.env, renderHtml: request => renderMediaHtml(request, { BrowserWindow, session }),
    resolveAsset: async revisionId => {
      if (revisionId !== "tone") throw Error("实测素材编号无效");
      return { path: audioPath, kind: "audio", duration: time(1000) };
    } });
  try {
    const { stdout: rawProbe } = await execute(configuration.ffprobePath, ["-v", "error", "-protocol_whitelist", "file,pipe", "-show_streams", "-show_format", "-of", "json", output.path], options);
    const probe = JSON.parse(rawProbe), video = probe.streams?.find(stream => stream.codec_type === "video"), audio = probe.streams?.find(stream => stream.codec_type === "audio");
    if (video?.codec_name !== "h264" || video.width !== 512 || video.height !== 256 || audio?.codec_name !== "aac" || Number(probe.format?.duration) < 0.99) throw Error("实测 H.264/AAC 输出不符");
    const visiblePixels = [];
    for (const position of [0.125, 0.5, 0.875]) {
      const { stdout: bytes } = await execute(configuration.ffmpegPath, ["-v", "error", "-nostdin", "-ss", String(position), "-i", output.path, "-frames:v", "1", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"], { ...options, encoding: "buffer" });
      if (bytes.length !== 512 * 256 * 3) throw Error("字幕成片帧回读尺寸错误");
      let bright = 0;
      for (let index = 0; index < bytes.length; index += 3) if (bytes[index] > 180 && bytes[index + 1] > 180 && bytes[index + 2] > 180) bright++;
      visiblePixels.push(bright);
    }
    if (visiblePixels[0] !== 0 || visiblePixels[1] < 100 || visiblePixels[2] !== 0) throw Error("字幕没有按时间边界在真实成片出现和消失");
    if (!output.subtitleEvidence?.length) throw Error("字幕缺少实际固定字体字形证据");
    process.stdout.write(`MEDIA_RUNTIME_SMOKE ${JSON.stringify({ encode: "h264", audio: "aac", subtitles: "electronBundledFont", probe: "verified", frames: "verified", environmentPath: "empty", electronVersion: process.versions.electron, visiblePixels, subtitleEvidence: output.subtitleEvidence })}\n`);
  } finally { await output.cleanup(); }
  app.quit();
}).catch(error => { process.stderr.write(`${error.stack}\n`); app.exit(1); });
