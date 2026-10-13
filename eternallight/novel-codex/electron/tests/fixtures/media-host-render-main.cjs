const { app, BrowserWindow, session, nativeImage } = require("electron");
const { pathToFileURL } = require("node:url");
const { join } = require("node:path");
const { readFile, writeFile } = require("node:fs/promises");
app.commandLine.appendSwitch("disable-gpu");
app.on("window-all-closed", () => {});
app.whenReady().then(async () => {
  const root = process.argv[2], directory = process.argv[3];
  const stage = value => writeFile(join(directory, "stage.txt"), value);
  await stage("imports");
  const { renderMediaHtml } = await import(pathToFileURL(join(root, "media-html-renderer.mjs")));
  const { MultimediaHost } = await import(pathToFileURL(join(root, "media-host.mjs")));
  await stage("picture-render");
  const renderHtml = options => renderMediaHtml(options, { BrowserWindow, session });
  const picture = await renderHtml({ html: '<html><body style="margin:0;background:#ec4338;width:64px;height:64px"></body></html>', width: 64, height: 64, format: "png" });
  await stage("asset-import");
  const rect = { x: 0, y: 0, width: 200, height: 150 };
  let asset = null;
  const savedDocument = { kind: "comic", title: "双页漫画", format: "pages", pages: ["one", "two"].map(id => ({
    id, title: id, width: 200, height: 300, readingDirection: "leftToRight", panels: [{ id: `panel-${id}`, description: "画面", rect,
      assetRevisionId: "image-revision", referenceAssetRevisionIds: [], locked: false,
      balloons: [{ id: `balloon-${id}`, kind: "caption", speaker: "旁白", text: '真实导出 <script>伪指令</script>', rect: { x: 5, y: 70, width: 185, height: 50 } }] }] })) };
  const workspace = { project: { id: "project" }, edition: { id: "edition", projectId: "project", scopeId: "scope", medium: "comic" },
    revisionId: "saved-version", document: savedDocument, sources: [], assets: [] };
  const requests = [];
  const host = new MultimediaHost({ directory, connections: {}, chooseFiles: async () => [], openPath: async () => "", renderHtml,
    executables: { ffmpegPath: null, ffprobePath: null }, decodeImage: bytes => {
      const image = nativeImage.createFromBuffer(Buffer.from(bytes)); if (image.isEmpty()) throw Error("invalid bitmap"); return image.getSize();
    }, runtimeRequest: async (path, options) => {
      requests.push(path);
      if (path.endsWith("/imports")) return {};
      if (path.endsWith("/assets")) {
        const body = JSON.parse(options.body); asset = { ...body.asset, id: "image", assetRevisionId: "image-revision", projectId: "project", scopeId: "scope" };
        workspace.assets = [asset]; return asset;
      }
      if (path.endsWith("/workspace?revisionId=saved-version")) return workspace;
      throw Error(`unexpected route ${path}`);
    } });
  const imported = await host.importFiles("project", "edition", [{ name: "参考.png", bytes: picture }]);
  if (imported.assets[0].width !== 64) throw Error("picture metadata mismatch");
  await stage("comic-png");
  const png = await host.exportEdition("project", "edition", { expectedRevisionId: "saved-version", format: "png" });
  await stage("comic-pdf");
  const pdf = await host.exportEdition("project", "edition", { expectedRevisionId: "saved-version", format: "pdf" });
  const pngPath = join(directory, "exports", png.id, "page-001.png");
  const bitmap = nativeImage.createFromBuffer(await readFile(pngPath));
  if (bitmap.getSize().width !== 200 || bitmap.getSize().height !== 300) throw Error("export pixels do not match document");
  const pdfBytes = await readFile(join(directory, "exports", pdf.id, "comic.pdf"));
  if (!pdfBytes.subarray(0, 5).equals(Buffer.from("%PDF-")) || !/\/Count\s+2\b/.test(pdfBytes.toString("latin1"))) throw Error("expected one real two-page PDF");
  const manifest = JSON.parse(await readFile(join(directory, "exports", pdf.id, "manifest.json"), "utf8"));
  if (manifest.revisionId !== "saved-version" || manifest.outputs.length !== 1 || manifest.assetRevisionIds[0] !== "image-revision") throw Error("export provenance mismatch");
  process.stdout.write(JSON.stringify({ passed: true, pngPath, png: bitmap.getSize(), pdfPages: 2, requests }));
  app.quit();
}).catch(error => { process.stderr.write(String(error.stack)); app.exit(1); });
