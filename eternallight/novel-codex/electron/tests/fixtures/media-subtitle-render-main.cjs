const { app, BrowserWindow, session, nativeImage } = require("electron");
const assert = require("node:assert/strict");
const { pathToFileURL } = require("node:url");
const { join } = require("node:path");
const { readFile, writeFile, readdir } = require("node:fs/promises");
const { execFile } = require("node:child_process");
const { promisify } = require("node:util");
const { createServer } = require("node:http");
const { createHash } = require("node:crypto");
app.commandLine.appendSwitch("disable-gpu"); app.on("window-all-closed", () => {});
app.whenReady().then(async () => {
  const root = process.argv[2], directory = process.argv[3];
  const { renderMediaHtml } = await import(pathToFileURL(join(root, "media-html-renderer.mjs")));
  const { renderAnimationDocument } = await import(pathToFileURL(join(root, "media-render.mjs")));
  const { prepareSubtitlePlan } = await import(pathToFileURL(join(root, "media-subtitles.mjs")));
  const { MultimediaHost } = await import(pathToFileURL(join(root, "media-host.mjs")));
  const width = 320, height = 180, time = value => ({ value, rate: { numerator: 1000, denominator: 1 } });
  const clip = (id, caption, start, duration) => ({ id, caption, start: time(start), duration: time(duration), sourceStart: time(0), playbackRate: { numerator: 1, denominator: 1 }, shotId: null, assetRevisionId: null });
  let transparent = false, rasters = 0;
  const renderHtml = async options => {
    const result = await renderMediaHtml(options, { BrowserWindow, session });
    if (options.purpose === "subtitle") {
      rasters++;
      const image = nativeImage.createFromBuffer(result.bytes), bitmap = image.toBitmap();
      assert.equal(image.getSize().width, width); assert.equal(bitmap[3], 0);
      assert.ok(bitmap.some((value, index) => index % 4 === 3 && value > 0)); transparent = true;
    }
    return result;
  };
  const picture = await renderHtml({ html: `<html><body style="margin:0;background:#d41414;width:${width}px;height:${height}px"></body></html>`, width, height, format: "png" });
  const imagePath = join(directory, "background.png"); await writeFile(imagePath, picture);
  // Exercise the production private-session request boundary, not just HTML
  // escaping: a real local HTTP endpoint must receive no page requests.
  let hits = 0, permission = null, permissionPolicy = null, preferences = null; const decisions = [], protocolResponses = [];
  const server = createServer((_request, response) => { hits++; response.end(picture); });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  try {
    const port = server.address().port;
    const observedSession = { fromPartition: partition => {
      const value = session.fromPartition(partition), register = value.webRequest.onBeforeRequest.bind(value.webRequest), registerPermission = value.setPermissionRequestHandler.bind(value), registerProtocol = value.protocol.handle.bind(value.protocol);
      value.webRequest.onBeforeRequest = callback => register((details, complete) => callback(details, decision => { decisions.push({ url: details.url, cancel: decision.cancel }); complete(decision); }));
      value.setPermissionRequestHandler = callback => { permissionPolicy = callback; registerPermission(callback); };
      value.protocol.handle = (scheme, callback) => registerProtocol(scheme, async request => { const response = await callback(request); protocolResponses.push({ url: request.url, status: response.status }); return response; });
      return value;
    } };
    const ObservedWindow = function (options) {
      preferences = options.webPreferences;
      const window = new BrowserWindow(options), capture = window.webContents.capturePage.bind(window.webContents);
      window.webContents.capturePage = async (...args) => {
        permissionPolicy(window.webContents, "geolocation", allowed => { permission = allowed ? "granted" : "denied"; }, { requestingUrl: window.webContents.getURL(), isMainFrame: true });
        return capture(...args);
      };
      return window;
    };
    const trustedGreen = await renderMediaHtml({ html: `<html><body style="margin:0;overflow:hidden;width:64px;height:64px;background:#00cc00"></body></html>`, width: 64, height: 64, format: "png" }, { BrowserWindow, session });
    const hostileHtml = `<html><head><style>img,iframe{position:absolute;left:128px;top:128px}</style></head><body style="margin:0;overflow:hidden;width:64px;height:64px;background:#00cc00"><img src="http://127.0.0.1:${port}/image"><img src="https://127.0.0.1:${port}/secure"><img src="${pathToFileURL(imagePath)}"><iframe src="http://127.0.0.1:${port}/frame"></iframe><iframe src="https://127.0.0.1:${port}/secure-frame"></iframe><script src="http://127.0.0.1:${port}/script"></script><script>document.body.style.background='#ff00ff'</script></body></html>`;
    const blocked = await renderMediaHtml({ html: hostileHtml, width: 64, height: 64, format: "png" }, { BrowserWindow: ObservedWindow, session: observedSession });
    const bitmap = nativeImage.createFromBuffer(blocked).toBitmap();
    assert.equal(hits, 0); assert.ok(decisions.some(item => item.url.startsWith("http://127.0.0.1:") && item.cancel));
    assert.ok(decisions.some(item => item.url.startsWith("https://127.0.0.1:") && item.cancel)
      || protocolResponses.some(item => item.url.startsWith("https://127.0.0.1:") && item.status === 403));
    assert.deepEqual(bitmap, nativeImage.createFromBuffer(trustedGreen).toBitmap(), "blocked external/file/script content did not alter the independently rendered trusted canvas");
    assert.equal(preferences.javascript, false); assert.equal(preferences.nodeIntegration, false); assert.equal(preferences.contextIsolation, true); assert.equal(preferences.sandbox, true);
    assert.equal(permission, "denied");
  } finally { await new Promise(resolve => server.close(resolve)); }
  const first = clip("first", "中文 Subtitle\n<&>", 250, 250), second = clip("second", "第二句", 500, 500), overlap = clip("overlap", "同时出现", 750, 250);
  const document = { kind: "animation", title: "实际字幕", width, height, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [
    { id: "subtitle", kind: "subtitle", muted: false, clips: [first, second, overlap] },
    { id: "muted", kind: "subtitle", muted: true, clips: [clip("muted-text", String.fromCodePoint(0x10ffff), 0, 1250)] },
    { id: "video-after-subtitles", kind: "video", muted: false, clips: [{ ...clip("video", "", 0, 1250), assetRevisionId: "image" }] }] } };
  const processEnvironment = { PATH: "", TMPDIR: directory, ...(process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {}) };
  const options = { document, ffmpegPath: process.env.NOVEL_CODEX_FFMPEG, ffprobePath: process.env.NOVEL_CODEX_FFPROBE,
    fontPath: process.env.NOVEL_CODEX_MEDIA_FONT, renderHtml, processEnvironment, temporaryDirectory: directory,
    resolveAsset: async () => ({ path: imagePath, kind: "image" }) };
  const plan = await prepareSubtitlePlan(options), original = JSON.stringify(document);
  const html = plan.htmlForRaster(plan.rasters[0]); assert.ok(html.includes("&lt;&amp;&gt;")); assert.ok(!html.includes("<script>"));
  const output = await renderAnimationDocument(options);
  assert.equal(output.metadata.width, width); assert.equal(output.metadata.height, height); assert.equal(output.subtitleEvidence.length, 3);
  const decoded = await promisify(execFile)(options.ffmpegPath, ["-v", "error", "-i", output.path, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"],
    { env: processEnvironment, encoding: "buffer", timeout: 30_000, maxBuffer: 8 * 1024 * 1024 });
  const pixels = decoded.stdout, bytesPerFrame = width * height * 3, frames = [];
  assert.equal(pixels.length, bytesPerFrame * 30);
  for (let frame = 0; frame < 30; frame++) {
    let count = 0;
    for (let index = frame * bytesPerFrame; index < (frame + 1) * bytesPerFrame; index += 3) if (pixels[index] > 180 && pixels[index + 1] > 180 && pixels[index + 2] > 180) count++;
    frames.push(count);
    const corner = frame * bytesPerFrame; assert.ok(pixels[corner] > 150 && pixels[corner + 1] < 60 && pixels[corner + 2] < 60, "transparent raster preserves the underlying red picture");
  }
  assert.equal(frames[5], 0); assert.ok(frames[6] > 10); assert.ok(frames[11] > 10); assert.ok(frames[12] > 10); assert.equal(frames[24], 0);
  assert.ok(frames[18] > frames[17]);
  const fontEvidence = output.subtitleEvidence.flatMap(item => item.fontEvidence), boundaries = plan.states.map(({ startFrame, endFrame }) => [startFrame, endFrame]);
  await output.cleanup();
  // Assert persistence at the actual Host export boundary. Only Runtime's
  // fixed workspace read is controlled; HTML, font, encoder and output are real.
  const hostDirectory = join(directory, "host"), hostFontEvidence = [], hostRequests = [];
  const hostDocument = { kind: "animation", title: "Host 字幕清单", width, height, frameRate: document.frameRate, shots: [], timeline: { tracks: [
    { id: "host-subtitle-track", kind: "subtitle", muted: false, clips: [clip("host-cue", "Host 中文", 250, 500), clip("host-empty-span", "", 0, 1000)] }] } };
  const hostWorkspace = revisionId => ({ project: { id: "subtitle-project" }, edition: { id: "subtitle-edition", projectId: "subtitle-project", scopeId: "subtitle-scope", medium: "animation" },
    revisionId, document: revisionId === "subtitle-saved-revision" ? hostDocument : { ...hostDocument, timeline: { tracks: [{ id: "empty-subtitle-track", kind: "subtitle", muted: false, clips: [clip("empty-span", "", 0, 1000)] }] } }, sources: [], assets: [] });
  const host = new MultimediaHost({ directory: hostDirectory, connections: {}, chooseFiles: async () => [], decodeImage: () => { throw Error("no image import expected"); }, openPath: async () => "",
    executables: { ffmpegPath: options.ffmpegPath, ffprobePath: options.ffprobePath, fontPath: options.fontPath },
    renderHtml: async request => { const rendered = await renderMediaHtml(request, { BrowserWindow, session }); hostFontEvidence.push(rendered.fontEvidence); return rendered; },
    runtimeRequest: async (path, request) => {
      assert.equal(request, undefined); hostRequests.push(path);
      const match = /^\/api\/projects\/subtitle-project\/editions\/subtitle-edition\/workspace\?revisionId=(subtitle-saved-revision|empty-saved-revision)$/.exec(path);
      assert.ok(match, `unexpected Host Runtime request ${path}`); return hostWorkspace(match[1]);
    } });
  const hostExport = await host.exportEdition("subtitle-project", "subtitle-edition", { expectedRevisionId: "subtitle-saved-revision", format: "mp4" });
  const hostPath = join(hostDirectory, "exports", hostExport.id), hostVideo = await readFile(join(hostPath, "animation.mp4"));
  const hostManifest = JSON.parse(await readFile(join(hostPath, "manifest.json"), "utf8"));
  assert.equal(hostManifest.projectId, "subtitle-project"); assert.equal(hostManifest.editionId, "subtitle-edition"); assert.equal(hostManifest.revisionId, "subtitle-saved-revision");
  assert.equal(hostManifest.format, "mp4"); assert.equal(hostManifest.artifactKind, "renderedVideo"); assert.equal(hostManifest.subtitleEvidence.length, 1);
  assert.equal(hostManifest.subtitleEvidence[0].fontDigest, createHash("sha256").update(await readFile(options.fontPath)).digest("hex"));
  assert.deepEqual(hostManifest.subtitleEvidence[0].periods, [{ startFrame: 6, endFrame: 18, clipIds: ["host-cue"] }]);
  assert.deepEqual(hostManifest.subtitleEvidence[0].fontEvidence, hostFontEvidence[0]);
  assert.ok(hostManifest.subtitleEvidence[0].fontEvidence.every(record => record.fonts.every(font => font.isCustomFont && font.glyphCount > 0)));
  assert.deepEqual(hostManifest.assetRevisionIds, []); assert.equal(hostManifest.outputs.length, 1);
  assert.equal(hostManifest.outputs[0].digest, createHash("sha256").update(hostVideo).digest("hex")); assert.equal(hostManifest.outputs[0].bytes, hostVideo.length);
  const hostDecoded = await promisify(execFile)(options.ffmpegPath, ["-v", "error", "-i", join(hostPath, "animation.mp4"), "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"],
    { env: processEnvironment, encoding: "buffer", timeout: 30_000, maxBuffer: 8 * 1024 * 1024 });
  assert.equal(hostDecoded.stdout.length, bytesPerFrame * 24);
  const hostFrameCounts = [5, 6, 17, 18].map(frame => {
    let count = 0;
    for (let index = frame * bytesPerFrame; index < (frame + 1) * bytesPerFrame; index += 3) if (hostDecoded.stdout[index] > 180 && hostDecoded.stdout[index + 1] > 180 && hostDecoded.stdout[index + 2] > 180) count++;
    return count;
  });
  assert.equal(hostFrameCounts[0], 0); assert.ok(hostFrameCounts[1] > 10); assert.ok(hostFrameCounts[2] > 10); assert.equal(hostFrameCounts[3], 0);
  const emptyExport = await host.exportEdition("subtitle-project", "subtitle-edition", { expectedRevisionId: "empty-saved-revision", format: "mp4" });
  const emptyManifest = JSON.parse(await readFile(join(hostDirectory, "exports", emptyExport.id, "manifest.json"), "utf8"));
  assert.deepEqual(emptyManifest.subtitleEvidence, []); assert.equal(hostFontEvidence.length, 1, "empty subtitles must not claim a raster or font was rendered");
  assert.deepEqual(hostRequests, ["/api/projects/subtitle-project/editions/subtitle-edition/workspace?revisionId=subtitle-saved-revision", "/api/projects/subtitle-project/editions/subtitle-edition/workspace?revisionId=empty-saved-revision"]);
  let negativeChecks = 0;
  await assert.rejects(renderAnimationDocument({ ...options, document: { ...document, timeline: { tracks: [{ kind: "subtitle", clips: [clip("missing", String.fromCodePoint(0x10ffff), 0, 1000)] }] } } }), /缺少字形/); negativeChecks++;
  const damaged = join(directory, "broken.ttf"); await writeFile(damaged, Buffer.alloc(16));
  await assert.rejects(renderAnimationDocument({ ...options, fontPath: damaged }), /字体/); negativeChecks++;
  await assert.rejects(renderAnimationDocument({ ...options, document: { ...document, timeline: { tracks: [{ kind: "subtitle", clips: [clip("too-tall", "中文\n".repeat(50), 0, 1000)] }] } } }), /安全区域/); negativeChecks++;
  const controller = new AbortController();
  const cancel = setTimeout(() => controller.abort(Error("author cancelled subtitles")), 15);
  await assert.rejects(renderAnimationDocument({ ...options, signal: controller.signal }), /author cancelled/); clearTimeout(cancel); negativeChecks++;
  assert.equal((await readdir(directory)).filter(name => name.startsWith("render-")).length, 0);
  process.stdout.write(JSON.stringify({ passed: true, rasters, transparent, frames, boundaries, fontEvidence, negativeChecks, captionPreserved: original === JSON.stringify(document), networkBlocked: hits === 0, permissionDenied: permission === "denied", hostManifestVerified: true, emptyHostEvidence: emptyManifest.subtitleEvidence, hostFrameCounts }));
  app.quit();
}).catch(error => { process.stderr.write(String(error.stack)); app.exit(1); });
