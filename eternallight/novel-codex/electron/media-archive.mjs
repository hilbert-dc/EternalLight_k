// @ts-check
import { createHash, randomUUID } from "node:crypto";
import { createReadStream } from "node:fs";
import { copyFile, lstat, mkdir, open, readFile, readdir, realpath, rename, rm } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";

const JSON_LIMIT = 8 * 1024 * 1024;
const ASSET_LIMIT = 512 * 1024 * 1024;
const ARCHIVE_LIMIT = 8 * 1024 * 1024 * 1024;
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const digest = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const identity = value => typeof value === "string" && value.trim() === value && Buffer.byteLength(value) > 0
  && Buffer.byteLength(value) <= 160 && !/[\p{Cc}/\\]/u.test(value) && ![".", ".."].includes(value);
const invalid = message => { throw Error(`离线作品包${message}`); };
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
  return JSON.stringify(value);
}

function archivePath(value) {
  if (typeof value !== "string" || !value || value.length > 220 || !/^[a-zA-Z0-9_.\/-]+$/.test(value)
    || isAbsolute(value) || value.split("/").some(part => !part || [".", ".."].includes(part))) invalid("文件路径无效");
  return value;
}

function frozenWorkspace(workspace) {
  if (!workspace || !identity(workspace.project?.id) || !identity(workspace.edition?.id)
    || !identity(workspace.edition.scopeId) || !identity(workspace.edition.branchId)
    || workspace.edition.projectId !== workspace.project.id || workspace.edition.editionId !== workspace.edition.id
    || !identity(workspace.revisionId) || !identity(workspace.headSnapshotId) || !digest(workspace.sourceDigest)
    || !Number.isSafeInteger(workspace.revision) || workspace.revision < 0
    || !["comic", "animation"].includes(workspace.edition.medium) || workspace.document?.kind !== workspace.edition.medium
    || !Array.isArray(workspace.assets) || workspace.assets.length > 4096 || !Array.isArray(workspace.sources)
    || !Array.isArray(workspace.edition.sources)) invalid("作品修订或作用域无效");
  const assetIds = new Set();
  const assets = workspace.assets.map(asset => {
    if (!identity(asset.id) || !identity(asset.assetRevisionId) || assetIds.has(asset.assetRevisionId)
      || asset.projectId !== workspace.project.id || !identity(asset.scopeId)
      || !digest(asset.digest) || !Number.isSafeInteger(asset.bytes) || asset.bytes <= 0 || asset.bytes > ASSET_LIMIT
      || !["image", "audio", "video"].includes(asset.kind) || typeof asset.mimeType !== "string"
      || !asset.mimeType.startsWith(`${asset.kind}/`) || typeof asset.name !== "string" || !asset.name
      || asset.name.length > 255 || /[\p{Cc}]/u.test(asset.name)) invalid("素材记录无效");
    assetIds.add(asset.assetRevisionId);
    return Object.fromEntries(["id", "assetRevisionId", "projectId", "scopeId", "name", "kind", "mimeType", "digest", "bytes", "width", "height", "duration", "sampleRate", "channels", "createdAt"].map(key => [key, asset[key]]));
  });
  const checkSource = source => {
    if (!source || !["projectId", "scopeId", "branchId", "objectId", "revisionId"].every(key => identity(source[key]))
      || !digest(source.digest) || !source.selector || typeof source.selector.kind !== "string") invalid("来源引用无效");
  };
  workspace.edition.sources.forEach(checkSource);
  for (const link of workspace.sources) {
    checkSource(link.source);
    if (!["adapts", "depicts", "usesReference", "derivedFrom"].includes(link.relation) || !identity(link.targetObjectId) || !link.targetSelector) invalid("来源关联无效");
  }
  if (!workspace.documentSource) invalid("缺少固定文档来源");
  checkSource(workspace.documentSource);
  if (workspace.documentSource.projectId !== workspace.project.id || workspace.documentSource.scopeId !== workspace.edition.scopeId
    || workspace.documentSource.branchId !== workspace.edition.branchId || workspace.documentSource.revisionId !== workspace.revisionId
    || workspace.documentSource.objectId !== workspace.edition.id || workspace.documentSource.selector.kind !== "whole") invalid("文档来源与固定修订不一致");
  if (typeof workspace.documentEncoding !== "string" || Buffer.byteLength(workspace.documentEncoding) > JSON_LIMIT
    || hash(Buffer.from(workspace.documentEncoding)) !== workspace.documentSource.digest
    || canonicalJson(JSON.parse(workspace.documentEncoding)) !== canonicalJson(workspace.document)) invalid("原始文档字节与修订来源不一致");
  const refs = [];
  if (workspace.document.kind === "comic") {
    if (!Array.isArray(workspace.document.pages)) invalid("漫画文档无效");
    for (const page of workspace.document.pages) {
      if (!Array.isArray(page.panels)) invalid("漫画页面无效");
      for (const panel of page.panels) {
        if (panel.assetRevisionId !== null) refs.push(panel.assetRevisionId);
        if (!Array.isArray(panel.referenceAssetRevisionIds)) invalid("漫画参考素材无效");
        refs.push(...panel.referenceAssetRevisionIds);
      }
    }
  } else {
    if (!Array.isArray(workspace.document.shots) || !Array.isArray(workspace.document.timeline?.tracks)) invalid("动画文档无效");
    for (const shot of workspace.document.shots) {
      if (shot.assetRevisionId !== null) refs.push(shot.assetRevisionId);
      if (!Array.isArray(shot.referenceAssetRevisionIds)) invalid("动画参考素材无效");
      refs.push(...shot.referenceAssetRevisionIds);
    }
    for (const track of workspace.document.timeline.tracks) {
      if (!Array.isArray(track.clips)) invalid("动画轨道无效");
      for (const clip of track.clips) if (clip.assetRevisionId !== null) refs.push(clip.assetRevisionId);
    }
  }
  if (refs.some(id => !assetIds.has(id))) invalid("缺少文档引用的素材");
  // Runtime admits cross-scope assets through the immutable document's original readSet/source grants.
  // An unrelated asset from another scope cannot be added merely because it shares the project.
  if (assets.some(asset => asset.scopeId !== workspace.edition.scopeId && !refs.includes(asset.assetRevisionId))) invalid("跨版本素材未被固定文档引用");
  return { project: workspace.project, edition: workspace.edition, revision: workspace.revision, revisionId: workspace.revisionId,
    headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest, documentSource: workspace.documentSource,
    document: workspace.document, documentEncoding: workspace.documentEncoding, assets, sources: workspace.sources };
}

async function syncDirectory(path) {
  let handle;
  try { handle = await open(path, "r"); await handle.sync(); }
  catch (error) { if (process.platform !== "win32" || !["EPERM", "EISDIR", "ENOTSUP"].includes(error.code)) throw error; }
  finally { await handle?.close(); }
}

async function writeDurable(path, bytes) {
  const file = await open(path, "wx", 0o600);
  try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
}

async function assertAbsent(path) {
  try { await lstat(path); invalid("恢复或导出目录已存在，请选择新目录"); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
}

async function prospectivePath(path) {
  const requested = resolve(path);
  let ancestor = requested;
  while (true) {
    try { return join(await realpath(ancestor), relative(ancestor, requested)); }
    catch (error) { if (error.code !== "ENOENT" || dirname(ancestor) === ancestor) throw error; ancestor = dirname(ancestor); }
  }
}

async function privateFile(directory, relativePath, maximum, signal) {
  archivePath(relativePath);
  const root = await realpath(directory), path = join(root, ...relativePath.split("/"));
  const parts = relativePath.split("/");
  let ancestor = root;
  for (const part of parts) {
    ancestor = join(ancestor, part);
    const info = await lstat(ancestor);
    if (info.isSymbolicLink() || (!info.isFile() && !info.isDirectory())) invalid("不能包含符号链接或特殊文件");
  }
  const info = await lstat(path), actual = await realpath(path);
  if (!info.isFile() || info.size > maximum || relative(root, actual).startsWith("..")) invalid("文件越界或过大");
  let bytes = 0;
  const sha = createHash("sha256");
  for await (const chunk of createReadStream(actual)) {
    signal?.throwIfAborted(); bytes += chunk.length;
    if (bytes > maximum) invalid("文件读取超过限制");
    sha.update(chunk);
  }
  if (bytes !== info.size) invalid("文件在读取时发生变化");
  return { path: actual, bytes, digest: sha.digest("hex") };
}

async function listFiles(directory) {
  const paths = [];
  const visit = async (path, prefix) => {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const name = prefix ? `${prefix}/${entry.name}` : entry.name;
      archivePath(name);
      if (entry.isDirectory()) { if (name !== "assets") invalid("包含未声明的目录"); await visit(join(path, entry.name), name); }
      else if (entry.isFile()) paths.push(name);
      else invalid("不能包含符号链接或特殊文件");
      if (paths.length > 4100) invalid("文件数量超过限制");
    }
  };
  await visit(directory, "");
  return paths.sort();
}

async function verifiedJson(file) {
  const bytes = await readFile(file.path);
  if (bytes.length !== file.bytes || hash(bytes) !== file.digest) invalid("文档在读取过程中发生变化");
  return JSON.parse(bytes.toString("utf8"));
}

/** One immutable Edition, its assets and fixed canon. Cross-scope original sources stay explicit references. */
export async function writeMediaArchive({ destinationDirectory, workspace, exportId, loadAsset, canonSnapshot = null, signal = undefined }) {
  if (!isAbsolute(destinationDirectory ?? "") || !identity(exportId) || typeof loadAsset !== "function") invalid("导出请求无效");
  const snapshot = frozenWorkspace(workspace);
  if (snapshot.edition.canonSnapshotId !== null && (!canonSnapshot || canonSnapshot.snapshotId !== snapshot.edition.canonSnapshotId)
    || snapshot.edition.canonSnapshotId === null && canonSnapshot !== null) invalid("共同设定快照不匹配");
  const snapshotBytes = Buffer.from(JSON.stringify(snapshot));
  const canonBytes = canonSnapshot === null ? null : Buffer.from(JSON.stringify(canonSnapshot));
  if (snapshotBytes.length > JSON_LIMIT || canonBytes?.length > JSON_LIMIT) invalid("文档或共同设定超过 8 MiB");
  const destination = resolve(destinationDirectory);
  await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
  await assertAbsent(destination);
  const staging = `${destination}.${randomUUID()}.tmp`;
  await mkdir(staging, { mode: 0o700 });
  try {
    await mkdir(join(staging, "assets"), { mode: 0o700 });
    await writeDurable(join(staging, "workspace.json"), snapshotBytes);
    const documentBytes = Buffer.from(snapshot.documentEncoding);
    await writeDurable(join(staging, "document.json"), documentBytes);
    const outputs = [{ name: "workspace.json", mimeType: "application/json", bytes: snapshotBytes.length, digest: hash(snapshotBytes) },
      { name: "document.json", mimeType: "application/json", bytes: documentBytes.length, digest: hash(documentBytes) }];
    if (canonBytes) {
      await writeDurable(join(staging, "canon.json"), canonBytes);
      outputs.push({ name: "canon.json", mimeType: "application/json", bytes: canonBytes.length, digest: hash(canonBytes) });
    }
    const assetFiles = [], written = new Set();
    let total = outputs.reduce((sum, file) => sum + file.bytes, 0);
    for (const asset of snapshot.assets) {
      signal?.throwIfAborted();
      const path = `assets/${asset.digest}`;
      assetFiles.push({ assetRevisionId: asset.assetRevisionId, path, digest: asset.digest, bytes: asset.bytes });
      if (written.has(path)) continue;
      const loaded = await loadAsset(asset.assetRevisionId);
      if (!(loaded.bytes instanceof Uint8Array) || loaded.bytes.length !== asset.bytes || hash(loaded.bytes) !== asset.digest) invalid("素材与固定回执不一致");
      total += loaded.bytes.length;
      if (total > ARCHIVE_LIMIT) invalid("总大小超过 8 GiB");
      await writeDurable(join(staging, "assets", asset.digest), loaded.bytes);
      outputs.push({ name: path, mimeType: asset.mimeType, bytes: asset.bytes, digest: asset.digest });
      written.add(path);
    }
    const manifest = { id: exportId, projectId: snapshot.project.id, editionId: snapshot.edition.id, revisionId: snapshot.revisionId,
      medium: snapshot.edition.medium, format: "project", artifactKind: "editableProject", archiveKind: "creativeEditionArchive",
      workspaceFile: "workspace.json", documentFile: "document.json", canonFile: canonBytes ? "canon.json" : null,
      sourceCoverage: "editionAndAssetsWithExternalSourceReferences", sources: snapshot.sources,
      assetRevisionIds: snapshot.assets.map(asset => asset.assetRevisionId), assetFiles, outputs };
    const manifestBytes = Buffer.from(JSON.stringify(manifest, null, 2));
    if (manifestBytes.length > JSON_LIMIT) invalid("清单超过限制");
    await writeDurable(join(staging, "manifest.json"), manifestBytes);
    await syncDirectory(join(staging, "assets")); await syncDirectory(staging);
    await verifyMediaArchive({ directory: staging, expectedProjectId: snapshot.project.id, expectedExportId: exportId, signal });
    signal?.throwIfAborted(); await assertAbsent(destination);
    await rename(staging, destination); await syncDirectory(dirname(destination));
    return { manifest, assetRevisionIds: manifest.assetRevisionIds, bytes: total };
  } finally { await rm(staging, { recursive: true, force: true }); }
}

export async function verifyMediaArchive({ directory, expectedProjectId = undefined, expectedExportId = undefined, signal = undefined }) {
  if (!isAbsolute(directory ?? "")) invalid("目录无效");
  const rootInfo = await lstat(directory);
  if (!rootInfo.isDirectory() || rootInfo.isSymbolicLink()) invalid("目录不能是符号链接");
  const manifestFile = await privateFile(directory, "manifest.json", JSON_LIMIT, signal);
  const manifest = await verifiedJson(manifestFile);
  if (!identity(manifest.id) || !identity(manifest.projectId) || !identity(manifest.editionId) || !identity(manifest.revisionId)
    || manifest.archiveKind !== "creativeEditionArchive" || manifest.format !== "project" || manifest.artifactKind !== "editableProject"
    || manifest.sourceCoverage !== "editionAndAssetsWithExternalSourceReferences" || manifest.workspaceFile !== "workspace.json" || manifest.documentFile !== "document.json"
    || ![null, "canon.json"].includes(manifest.canonFile) || !Array.isArray(manifest.outputs) || manifest.outputs.length > 4099
    || !Array.isArray(manifest.assetFiles) || !Array.isArray(manifest.assetRevisionIds) || !Array.isArray(manifest.sources)
    || expectedProjectId !== undefined && manifest.projectId !== expectedProjectId
    || expectedExportId !== undefined && manifest.id !== expectedExportId) invalid("清单或所属作品不匹配");
  const outputNames = new Set(), validatedFiles = new Map();
  let total = manifestFile.bytes;
  for (const output of manifest.outputs) {
    archivePath(output.name);
    if (output.name === "manifest.json" || outputNames.has(output.name) || !digest(output.digest)
      || !Number.isSafeInteger(output.bytes) || output.bytes <= 0 || output.bytes > ASSET_LIMIT
      || typeof output.mimeType !== "string") invalid("文件清单无效");
    outputNames.add(output.name);
    const maximum = output.name.endsWith(".json") ? JSON_LIMIT : ASSET_LIMIT;
    const file = await privateFile(directory, output.name, maximum, signal);
    if (file.bytes !== output.bytes || file.digest !== output.digest) invalid("文件完整性校验失败");
    total += file.bytes;
    if (total > ARCHIVE_LIMIT + JSON_LIMIT) invalid("总大小超过限制");
    validatedFiles.set(output.name, file);
  }
  if (JSON.stringify(await listFiles(directory)) !== JSON.stringify(["manifest.json", ...outputNames].sort())) invalid("文件集合与清单不一致");
  if (!validatedFiles.has("workspace.json") || !validatedFiles.has("document.json") || manifest.canonFile !== null && !validatedFiles.has("canon.json")) invalid("缺少文档或共同设定");
  const workspace = frozenWorkspace(await verifiedJson(validatedFiles.get("workspace.json")));
  if (workspace.project.id !== manifest.projectId || workspace.edition.id !== manifest.editionId || workspace.revisionId !== manifest.revisionId
    || workspace.edition.medium !== manifest.medium || JSON.stringify(workspace.sources) !== JSON.stringify(manifest.sources)
    || JSON.stringify(workspace.assets.map(asset => asset.assetRevisionId)) !== JSON.stringify(manifest.assetRevisionIds)
    || manifest.assetFiles.length !== workspace.assets.length) invalid("文档与清单不一致");
  const document = validatedFiles.get("document.json");
  if (document.digest !== workspace.documentSource.digest || !(await readFile(document.path)).equals(Buffer.from(workspace.documentEncoding))) invalid("原始文档文件与修订来源不一致");
  const expectedFiles = new Set(["workspace.json", "document.json", ...(manifest.canonFile ? ["canon.json"] : [])]);
  const assetFiles = workspace.assets.map((asset, index) => {
    const entry = manifest.assetFiles[index], expectedPath = `assets/${asset.digest}`, file = validatedFiles.get(expectedPath);
    if (entry?.assetRevisionId !== asset.assetRevisionId || entry.path !== expectedPath || entry.digest !== asset.digest
      || entry.bytes !== asset.bytes || !file || file.digest !== asset.digest || file.bytes !== asset.bytes) invalid("素材绑定或字节缺失");
    expectedFiles.add(expectedPath);
    return { assetRevisionId: asset.assetRevisionId, path: file.path, digest: file.digest, bytes: file.bytes };
  });
  if (JSON.stringify([...expectedFiles].sort()) !== JSON.stringify([...outputNames].sort())) invalid("包含未声明的作品内容");
  const canonSnapshot = manifest.canonFile === null ? null : await verifiedJson(validatedFiles.get("canon.json"));
  if (workspace.edition.canonSnapshotId !== null && canonSnapshot?.snapshotId !== workspace.edition.canonSnapshotId
    || workspace.edition.canonSnapshotId === null && canonSnapshot !== null) invalid("共同设定快照不匹配");
  return { manifest, manifestDigest: manifestFile.digest, workspace, canonSnapshot, assetFiles, bytes: total };
}

/** Restores an offline data package to a fresh directory; never imports into or overwrites the application database. */
export async function recoverMediaArchive({ directory, destinationDirectory, signal = undefined }) {
  if (!isAbsolute(destinationDirectory ?? "")) invalid("恢复目录无效");
  const verified = await verifyMediaArchive({ directory, signal });
  const sourceRoot = await realpath(directory), target = await prospectivePath(destinationDirectory), withinSource = relative(sourceRoot, target);
  if (withinSource === "" || !withinSource.startsWith("..") && !isAbsolute(withinSource)) invalid("恢复目录必须位于原始作品包之外");
  await mkdir(dirname(destinationDirectory), { recursive: true, mode: 0o700 });
  await assertAbsent(destinationDirectory);
  const staging = `${resolve(destinationDirectory)}.${randomUUID()}.tmp`;
  await mkdir(staging, { mode: 0o700 });
  try {
    await mkdir(join(staging, "assets"), { mode: 0o700 });
    for (const path of ["manifest.json", ...verified.manifest.outputs.map(output => output.name)]) {
      signal?.throwIfAborted();
      const input = await privateFile(directory, path, path.endsWith(".json") ? JSON_LIMIT : ASSET_LIMIT, signal);
      await copyFile(input.path, join(staging, ...path.split("/")));
      const file = await open(join(staging, ...path.split("/")), "r"); try { await file.chmod(0o600); await file.sync(); } finally { await file.close(); }
    }
    const restored = await verifyMediaArchive({ directory: staging, expectedProjectId: verified.manifest.projectId, expectedExportId: verified.manifest.id, signal });
    if (restored.manifestDigest !== verified.manifestDigest) invalid("在恢复过程中发生变化");
    await syncDirectory(join(staging, "assets")); await syncDirectory(staging);
    signal?.throwIfAborted(); await assertAbsent(destinationDirectory);
    await rename(staging, destinationDirectory); await syncDirectory(dirname(destinationDirectory));
    return { ...restored, assetFiles: restored.assetFiles.map(file => ({ ...file, path: join(resolve(destinationDirectory), "assets", file.digest) })) };
  } finally { await rm(staging, { recursive: true, force: true }); }
}
