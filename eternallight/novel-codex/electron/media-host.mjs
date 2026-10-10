import { lstat, mkdir, readFile, rename, stat, writeFile } from "node:fs/promises";
import { createHash, createHmac, randomUUID } from "node:crypto";
import { basename, join, isAbsolute } from "node:path";
import { importMediaBytes, resolveMediaBlob } from "./media-files.mjs";
import { renderComicDocument, renderAnimationDocument } from "./media-render.mjs";
import { writeMediaArchive, verifyMediaArchive } from "./media-archive.mjs";
import { credentialRealmId, mediaConnectionCapabilities } from "@eternallight/agent-engine";

const id = value => typeof value === "string" && value.trim() === value && value.length > 0
  && Buffer.byteLength(value) <= 160 && !/[\p{Cc}/\\]/u.test(value) && ![".", ".."].includes(value);
const protocols = new Map([
  ["openai-images", ["image.generate", "image.edit"]],
  ["openai-audio", ["audio.speech", "audio.transcribe"]],
  ["runway", ["video.generate"]],
]);
export function validateMediaConnection(request) {
  if (!request || !id(request.id) || !protocols.has(request.protocol)
    || typeof request.title !== "string" || !request.title.trim() || request.title.length > 120
    || typeof request.model !== "string" || !request.model.trim() || request.model.length > 160
    || request.apiKey !== undefined && (typeof request.apiKey !== "string" || request.apiKey.length > 8_000)) throw Error("生成服务配置无效");
  const url = new URL(request.baseUrl);
  if (url.username || url.password || url.search || url.hash || url.protocol !== "https:"
    && !(url.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname))) throw Error("生成服务地址必须是 HTTPS 或本机服务");
  return { id: request.id, title: request.title.trim(), protocol: request.protocol,
    baseUrl: url.href.replace(/\/$/, ""), model: request.model.trim() };
}

/** Credentials stay in the trusted Host and are encrypted by the OS key store. */
export class MediaConnectionStore {
  constructor({ directory, encryption, credentialSaltPath }) { this.path = join(directory, "media-services.json"); this.encryption = encryption; this.credentialSaltPath = credentialSaltPath; this.tail = Promise.resolve(); }
  async records() {
    let contents;
    try { contents = await readFile(this.path, "utf8"); } catch (error) { if (error.code === "ENOENT") return []; throw error; }
    const value = JSON.parse(contents);
    if (!Array.isArray(value)) throw Error("生成服务配置损坏");
    for (const item of value) validateMediaConnection(item);
    return value;
  }
  publicRecord(record) {
    const available = Boolean(record.encryptedCredential && this.encryption.isEncryptionAvailable());
    return { ...validateMediaConnection(record), credentialConfigured: available,
      capabilities: mediaConnectionCapabilities({ ...record, credentialConfigured: available }) };
  }
  async list() { const connections = (await this.records()).map(record => this.publicRecord(record)); return { connections, capabilities: connections.flatMap(record => record.capabilities) }; }
  async save(request) {
    const operation = this.tail.then(async () => {
      const normalized = validateMediaConnection(request), records = await this.records();
      const previous = records.find(record => record.id === normalized.id);
      let encryptedCredential = previous?.encryptedCredential ?? null;
      if (request.apiKey !== undefined) {
        if (request.apiKey && !this.encryption.isEncryptionAvailable()) throw Error("系统密钥存储不可用，服务密钥尚未保存");
        encryptedCredential = request.apiKey ? this.encryption.encryptString(request.apiKey).toString("base64") : null;
      }
      // A changed service must not silently inherit a credential for another endpoint.
      if (previous && (previous.baseUrl !== normalized.baseUrl || previous.protocol !== normalized.protocol)
        && request.apiKey === undefined) encryptedCredential = null;
      const next = { ...normalized, encryptedCredential };
      const output = [...records.filter(record => record.id !== normalized.id), next];
      const temporary = `${this.path}.${randomUUID()}.tmp`;
      await mkdir(join(this.path, ".."), { recursive: true, mode: 0o700 });
      await writeFile(temporary, JSON.stringify(output), { mode: 0o600 });
      await rename(temporary, this.path);
      return this.publicRecord(next);
    });
    this.tail = operation.catch(() => {});
    return operation;
  }
  async resolve(connectionId, capability) {
    const record = (await this.records()).find(item => item.id === connectionId);
    if (!record || !this.publicRecord(record).capabilities.some(item => item.kind === capability && item.available)) throw Error("所选服务不支持这项生成能力");
    if (!record.encryptedCredential || !this.encryption.isEncryptionAvailable()) throw Error("服务密钥不可用");
    const apiKey = this.encryption.decryptString(Buffer.from(record.encryptedCredential, "base64"));
    const salt = (await readFile(this.credentialSaltPath, "utf8")).trim();
    if (!/^[A-Za-z0-9_-]{43}$/.test(salt)) throw Error("凭据作用域密钥缺失或无效");
    const scope = `credential-v1:${createHmac("sha256", salt).update(`${new URL(record.baseUrl).origin}\0${apiKey}`, "utf8").digest("hex")}`;
    return { ...validateMediaConnection(record), apiKey, credentialRealmId: credentialRealmId(scope) };
  }
}

export function resolveMediaExecutables({ packaged, resourcesPath, environment }) {
  const executable = name => {
    const path = packaged ? join(resourcesPath, "media-runtime", process.platform === "win32" ? `${name}.exe` : name)
      : environment[`NOVEL_CODEX_${name.toUpperCase()}`] ?? null;
    if (path !== null && (!isAbsolute(path) || path.includes("\0"))) throw Error(`${name} 必须配置为绝对路径`);
    return path;
  };
  const fontPath = packaged ? join(resourcesPath, "media-runtime", "subtitle-font.ttf") : environment.NOVEL_CODEX_MEDIA_FONT ?? null;
  if (fontPath !== null && (!isAbsolute(fontPath) || fontPath.includes("\0"))) throw Error("字幕字体必须配置为绝对路径");
  return { ffmpegPath: executable("ffmpeg"), ffprobePath: executable("ffprobe"), fontPath };
}

export class MultimediaHost {
  constructor({ runtimeRequest, directory, connections, chooseFiles, decodeImage, renderHtml, openPath, executables }) {
    Object.assign(this, { runtimeRequest, directory, connections, chooseFiles, decodeImage, renderHtml, openPath, executables });
    this.previewToken = randomUUID(); this.assets = new Map(); this.exports = new Map();
  }
  route(projectId, editionId) {
    if (!id(projectId) || !id(editionId)) throw Error("作品或版本编号无效");
    return `/api/projects/${encodeURIComponent(projectId)}/editions/${encodeURIComponent(editionId)}`;
  }
  rememberAsset(asset) {
    if (!id(asset.assetRevisionId) || !id(asset.projectId) || !/^[a-f0-9]{64}$/.test(asset.digest)) throw Error("素材回执无效");
    this.assets.set(asset.assetRevisionId, asset);
    return { ...asset, previewUrl: `creative-media://${this.previewToken}/${encodeURIComponent(asset.assetRevisionId)}` };
  }
  async workspace(projectId, editionId, branchId, revisionId) {
    if (branchId !== undefined && !id(branchId)) throw Error("分支编号无效");
    if (revisionId !== undefined && !id(revisionId)) throw Error("作品修订编号无效");
    const params = new URLSearchParams();
    if (branchId) params.set("branchId", branchId);
    if (revisionId) params.set("revisionId", revisionId);
    const query = params.size ? `?${params}` : "";
    const workspace = await this.runtimeRequest(`${this.route(projectId, editionId)}/workspace${query}`);
    return { ...workspace, assets: workspace.assets.map(asset => this.rememberAsset(asset)) };
  }
  async save(projectId, editionId, request) {
    if (!request || Buffer.byteLength(JSON.stringify(request)) > 8 * 1024 * 1024) throw Error("文档保存请求无效或过大");
    const value = await this.runtimeRequest(`${this.route(projectId, editionId)}/workspace`, { method: "POST", body: JSON.stringify(request) });
    return { ...value, assets: value.assets.map(asset => this.rememberAsset(asset)) };
  }
  async canon(projectId, snapshotId) {
    if (!id(projectId) || snapshotId !== undefined && !id(snapshotId)) throw Error("共同设定编号无效");
    return this.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/canon${snapshotId ? `?${new URLSearchParams({ snapshotId })}` : ""}`);
  }
  async patch(projectId, editionId, request) {
    if (!request || Buffer.byteLength(JSON.stringify(request)) > 256 * 1024) throw Error("局部编辑请求无效或过大");
    const value = await this.runtimeRequest(`${this.route(projectId, editionId)}/patch`, { method: "POST", body: JSON.stringify(request) });
    return { ...value, assets: value.assets.map(asset => this.rememberAsset(asset)) };
  }
  async textPage(projectId, editionId, request) {
    if (!request || !id(request.revisionId) || request.branchId !== undefined && !id(request.branchId)
      || Buffer.byteLength(JSON.stringify(request)) > 16 * 1024) throw Error("文本读取请求无效或过大");
    const { branchId, ...body } = request;
    const query = branchId ? `?${new URLSearchParams({ branchId })}` : "";
    const value = await this.runtimeRequest(`${this.route(projectId, editionId)}/text-page${query}`, { method: "POST", body: JSON.stringify(body) });
    if (value.source?.projectId !== projectId || value.source?.objectId !== editionId
      || value.source?.revisionId !== request.revisionId || value.unit !== "unicodeScalar"
      || branchId !== undefined && value.source?.branchId !== branchId) throw Error("文本读取回执与固定作品修订不一致");
    return value;
  }
  async saveCanon(projectId, request) {
    if (!id(projectId) || !request || Buffer.byteLength(JSON.stringify(request)) > 2 * 1024 * 1024) throw Error("共同设定保存请求无效或过大");
    return this.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/canon`, { method: "POST", body: JSON.stringify(request) });
  }
  async sourceImpacts(projectId, editionId, offset = 0, limit = 20) {
    if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(limit) || limit < 1 || limit > 100) throw Error("来源读取范围无效");
    return this.runtimeRequest(`${this.route(projectId, editionId)}/source-impacts?${new URLSearchParams({ offset: String(offset), limit: String(limit) })}`);
  }
  async importFiles(projectId, editionId, files) {
    const route = this.route(projectId, editionId);
    if (files !== undefined && (!Array.isArray(files) || files.length > 20 || files.some(file => typeof file.name !== "string"
      || !(file.bytes instanceof Uint8Array) || file.bytes.byteLength > 64 * 1024 * 1024))) throw Error("图片附件无效或过大");
    const selected = files ?? await this.chooseFiles();
    if (!Array.isArray(selected) || selected.length > 20) throw Error("一次最多导入 20 个素材");
    const assets = [];
    for (const file of selected) {
      const name = basename(file.name ?? file.path ?? "");
      if (!name || name.length > 250) throw Error("素材名称无效");
      const operationId = `media-import-${randomUUID()}`, idempotencyKey = operationId;
      // Runtime pins the import before any content-addressed blob is written.
      await this.runtimeRequest(`${route}/imports`, { method: "POST", body: JSON.stringify({ operationId, idempotencyKey, name }) });
      let bytes = file.bytes;
      if (!bytes) {
        const metadata = await stat(file.path);
        if (!metadata.isFile() || metadata.size > 512 * 1024 * 1024) throw Error("素材文件过大或无效");
        bytes = await readFile(file.path);
      }
      const metadata = await importMediaBytes({ bytes, name, rootDirectory: this.directory,
        decodeImage: this.decodeImage, ffprobePath: this.executables.ffprobePath });
      const { relativePath: _relativePath, ...asset } = metadata;
      const saved = await this.runtimeRequest(`${route}/assets`, { method: "POST", body: JSON.stringify({ operationId, idempotencyKey, asset, sources: [] }) });
      assets.push(this.rememberAsset(saved));
    }
    return { assets };
  }
  async assetBytes(assetRevisionId, projectId, scopeId) {
    const asset = this.assets.get(assetRevisionId);
    if (!asset || asset.projectId !== projectId || scopeId && asset.scopeId !== scopeId) throw Error("素材不属于当前作品版本");
    const path = await resolveMediaBlob({ rootDirectory: this.directory, digest: asset.digest });
    return { ...asset, path, bytes: await readFile(path) };
  }
  async subtitleFontBytes() {
    const path = this.executables.fontPath;
    if (!path) return null;
    const metadata = await lstat(path);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 12 || metadata.size > 32 * 1024 * 1024) throw Error("字幕字体文件无效或过大");
    const bytes = await readFile(path);
    if (bytes.length !== metadata.size || !["OTTO", "ttcf"].includes(bytes.toString("ascii", 0, 4)) && bytes.readUInt32BE(0) !== 0x00010000) throw Error("字幕字体格式无效");
    return { bytes, digest: createHash("sha256").update(bytes).digest("hex") };
  }
  async subtitleFont() {
    const font = await this.subtitleFontBytes();
    return font ? { previewUrl: `creative-media://${this.previewToken}/fonts/${font.digest}`, digest: font.digest } : null;
  }
  async preview(request) {
    const url = new URL(request.url), assetRevisionId = decodeURIComponent(url.pathname.slice(1));
    if (url.protocol !== "creative-media:" || url.hostname !== this.previewToken || url.search || url.hash || request.method !== "GET") return new Response("Not found", { status: 404 });
    if (/^fonts\/[a-f0-9]{64}$/.test(assetRevisionId)) {
      const font = await this.subtitleFontBytes();
      if (!font || assetRevisionId !== `fonts/${font.digest}`) return new Response("Not found", { status: 404 });
      return new Response(font.bytes, { headers: { "Content-Type": "font/sfnt", "Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff" } });
    }
    if (!id(assetRevisionId)) return new Response("Not found", { status: 404 });
    const asset = this.assets.get(assetRevisionId);
    if (!asset) return new Response("Not found", { status: 404 });
    const content = await this.assetBytes(assetRevisionId, asset.projectId);
    const headers = { "Content-Type": asset.mimeType, "Cache-Control": "private, immutable", "X-Content-Type-Options": "nosniff", "Accept-Ranges": "bytes" };
    const range = request.headers.get("range");
    if (range) {
      const match = /^bytes=(\d+)-(\d*)$/.exec(range);
      if (!match) return new Response(null, { status: 416 });
      const start = Number(match[1]), end = match[2] ? Number(match[2]) : content.bytes.length - 1;
      if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || end >= content.bytes.length) return new Response(null, { status: 416 });
      return new Response(content.bytes.subarray(start, end + 1), { status: 206, headers: { ...headers, "Content-Range": `bytes ${start}-${end}/${content.bytes.length}` } });
    }
    return new Response(content.bytes, { headers });
  }
  async exportEdition(projectId, editionId, request) {
    if (!request || !id(request.expectedRevisionId) || !["png", "pdf", "mp4", "project"].includes(request.format)) throw Error("导出请求无效");
    const workspace = await this.workspace(projectId, editionId, undefined, request.expectedRevisionId);
    if (workspace.revisionId !== request.expectedRevisionId) throw Error("导出未读到指定作品修订");
    const exportId = `media-export-${randomUUID()}`, exportPath = join(this.directory, "exports", exportId);
    const usedIds = new Set();
    const loadAsset = async assetRevisionId => {
      // Runtime's fixed workspace is the authority for explicitly cited cross-edition assets.
      if (!workspace.assets.some(asset => asset.assetRevisionId === assetRevisionId && asset.projectId === projectId)) throw Error("导出素材不在已保存作品中");
      usedIds.add(assetRevisionId); return this.assetBytes(assetRevisionId, projectId);
    };
    if (request.format === "project") {
      const canonSnapshot = await this.canon(projectId, workspace.edition.canonSnapshotId);
      const archive = await writeMediaArchive({ destinationDirectory: exportPath, workspace, exportId, loadAsset, canonSnapshot });
      this.exports.set(exportId, { projectId, path: exportPath, format: "project" });
      return { id: exportId, editionId, revisionId: workspace.revisionId, name: workspace.document.title,
        format: "project", bytes: archive.bytes, assetRevisionIds: archive.assetRevisionIds, manifestId: exportId };
    }
    await mkdir(exportPath, { recursive: true, mode: 0o700 });
    let files, subtitleEvidence = [];
    if (workspace.document.kind === "comic" && ["png", "pdf"].includes(request.format)) {
      files = await renderComicDocument({ document: workspace.document, pageIds: request.pageIds, format: request.format, loadAsset, renderHtml: this.renderHtml });
    } else if (workspace.document.kind === "animation" && request.format === "mp4") {
      if (!this.executables.ffmpegPath || !this.executables.ffprobePath) throw Error("本机视频渲染组件尚未配置");
      const render = await renderAnimationDocument({ document: workspace.document, resolveAsset: loadAsset, renderHtml: this.renderHtml,
        ...this.executables, temporaryDirectory: join(this.directory, "renders") });
      try { files = [{ name: "animation.mp4", mimeType: "video/mp4", bytes: await readFile(render.path) }]; subtitleEvidence = render.subtitleEvidence; }
      finally { await render.cleanup(); }
    } else throw Error("此作品不支持所选导出格式");
    const outputs = [];
    for (const file of files) {
      if (basename(file.name) !== file.name) throw Error("渲染结果文件名无效");
      const bytes = Buffer.from(file.bytes), digest = createHash("sha256").update(bytes).digest("hex");
      await writeFile(join(exportPath, file.name), bytes, { mode: 0o600 });
      outputs.push({ name: file.name, mimeType: file.mimeType, bytes: bytes.length, digest });
    }
    const manifest = { id: exportId, projectId, editionId, revisionId: workspace.revisionId,
      medium: workspace.edition.medium, format: request.format, artifactKind: request.format === "mp4" ? "renderedVideo" : request.format === "project" ? "editableProject" : "renderedPages",
      sources: workspace.sources, assetRevisionIds: [...usedIds], outputs, subtitleEvidence };
    await writeFile(join(exportPath, "manifest.json"), JSON.stringify(manifest, null, 2), { mode: 0o600 });
    this.exports.set(exportId, { projectId, path: exportPath, format: request.format });
    return { id: exportId, editionId, revisionId: workspace.revisionId, name: workspace.document.title,
      format: request.format, bytes: outputs.reduce((sum, file) => sum + file.bytes, 0), assetRevisionIds: [...usedIds], manifestId: exportId };
  }
  async openExport(projectId, exportId) {
    if (!id(projectId) || !id(exportId)) throw Error("导出编号无效");
    const path = join(this.directory, "exports", exportId);
    const manifest = JSON.parse(await readFile(join(path, "manifest.json"), "utf8"));
    if (manifest.projectId !== projectId || manifest.id !== exportId) throw Error("导出结果不属于当前作品");
    if (this.exports.get(exportId)?.format === "project" || manifest.archiveKind === "creativeEditionArchive" || manifest.format === "project") {
      await verifyMediaArchive({ directory: path, expectedProjectId: projectId, expectedExportId: exportId });
    }
    const error = await this.openPath(path);
    if (error) throw Error("无法打开导出结果");
    return { opened: true };
  }
}

export function registerMultimediaHandlers(ipcMain, host) {
  ipcMain.handle("media:list-editions", (_event, projectId) => { if (!id(projectId)) throw Error("作品编号无效"); return host.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/editions`); });
  ipcMain.handle("media:create-edition", (_event, projectId, request) => { if (!id(projectId) || !request) throw Error("作品编号无效"); return host.runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/editions`, { method: "POST", body: JSON.stringify(request) }); });
  ipcMain.handle("media:read-workspace", (_event, projectId, editionId, branchId) => host.workspace(projectId, editionId, branchId));
  ipcMain.handle("media:read-text-page", (_event, projectId, editionId, request) => host.textPage(projectId, editionId, request));
  ipcMain.handle("media:read-subtitle-font", () => host.subtitleFont());
  ipcMain.handle("media:save-document", (_event, projectId, editionId, request) => host.save(projectId, editionId, request));
  ipcMain.handle("media:read-canon", (_event, projectId, snapshotId) => host.canon(projectId, snapshotId));
  ipcMain.handle("media:patch-document", (_event, projectId, editionId, request) => host.patch(projectId, editionId, request));
  ipcMain.handle("media:save-canon", (_event, projectId, request) => host.saveCanon(projectId, request));
  ipcMain.handle("media:source-impacts", (_event, projectId, editionId, offset, limit) => host.sourceImpacts(projectId, editionId, offset, limit));
  ipcMain.handle("media:import-files", (_event, projectId, editionId, files) => host.importFiles(projectId, editionId, files));
  ipcMain.handle("media:export-edition", (_event, projectId, editionId, request) => host.exportEdition(projectId, editionId, request));
  ipcMain.handle("media:open-export", (_event, projectId, exportId) => host.openExport(projectId, exportId));
  ipcMain.handle("media:list-connections", () => host.connections.list());
  ipcMain.handle("media:save-connection", (_event, request) => host.connections.save(request));
}
