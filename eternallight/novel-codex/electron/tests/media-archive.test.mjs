import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { recoverMediaArchive, verifyMediaArchive, writeMediaArchive } from "../media-archive.mjs";
import { importMediaBytes } from "../media-files.mjs";
import { MultimediaHost } from "../media-host.mjs";

const picture = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aL1sAAAAASUVORK5CYII=", "base64");
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
function workspace() {
  const document = { kind: "comic", title: "离线故事", format: "pages", pages: [{ id: "page", title: "开篇", width: 200, height: 300,
    readingDirection: "leftToRight", panels: [{ id: "panel", rect: { x: 0, y: 0, width: 200, height: 100 }, description: "画面", assetRevisionId: "asset-revision",
      referenceAssetRevisionIds: ["reference-revision"], balloons: [], locked: false }] }] };
  const asset = { id: "asset", assetRevisionId: "asset-revision", projectId: "project", scopeId: "scope", name: "素材.png", kind: "image", mimeType: "image/png",
    digest: sha(picture), bytes: picture.length, width: 1, height: 1, duration: null, sampleRate: null, channels: null, createdAt: 1, previewUrl: "creative-media://private-token/asset-revision" };
  // Rust serde retains f64 spelling (0.0), which a parsed JavaScript object cannot preserve.
  const documentEncoding = JSON.stringify(document).replace('"x":0', '"x":0.0').replace('"y":0', '"y":0.0');
  const documentSource = { projectId: "project", scopeId: "scope", branchId: "branch", objectId: "edition", revisionId: "revision", digest: sha(documentEncoding), selector: { kind: "whole" } };
  const novelSource = { projectId: "project", scopeId: "novel-scope", branchId: "main", objectId: "chapter", revisionId: "novel-revision", digest: "e".repeat(64), selector: { kind: "text", startByte: 0, endByte: 6, quote: "开篇" } };
  return { project: { id: "project", title: "作品", logline: "共同世界", updated_at: 1 }, edition: { id: "edition", editionId: "edition", projectId: "project", scopeId: "scope", branchId: "branch", medium: "comic", title: "漫画版本", canonSnapshotId: "canon-snapshot", sources: [novelSource] },
    revision: 2, revisionId: "revision", headSnapshotId: "snapshot", sourceDigest: "a".repeat(64), documentSource, document, documentEncoding,
    assets: [asset, { ...asset, id: "reference", assetRevisionId: "reference-revision" }], sources: [{ relation: "adapts", source: novelSource, targetObjectId: "panel", targetSelector: { kind: "panel", pageId: "page", panelId: "panel" } }] };
}
async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "media-archive-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const archive = join(directory, "export");
  const input = { destinationDirectory: archive, workspace: workspace(), exportId: "export-id", canonSnapshot: { snapshotId: "canon-snapshot", claims: [{ id: "character", claim: "同一角色" }] }, loadAsset: async () => ({ bytes: picture }) };
  await writeMediaArchive(input);
  return { directory, archive, input };
}

test("offline archive preserves fixed document, Edition sources, canon, all assets and recovers to a fresh directory", async t => {
  const { directory, archive, input } = await fixture(t);
  const verified = await verifyMediaArchive({ directory: archive, expectedProjectId: "project", expectedExportId: "export-id" });
  assert.deepEqual(verified.workspace.document, input.workspace.document);
  assert.deepEqual(verified.workspace.edition.sources, input.workspace.edition.sources);
  assert.deepEqual(verified.workspace.documentSource, input.workspace.documentSource);
  assert.notEqual(verified.workspace.documentSource.digest, verified.workspace.sourceDigest);
  assert.notEqual(verified.workspace.documentSource.digest, sha(JSON.stringify(verified.workspace.document)));
  assert.equal(await readFile(join(archive, "document.json"), "utf8"), input.workspace.documentEncoding);
  assert.equal(verified.canonSnapshot.claims[0].claim, "同一角色");
  assert.equal(verified.assetFiles.length, 2);
  assert.deepEqual(await readdir(join(archive, "assets")), [sha(picture)]);
  assert.ok(!(await readFile(join(archive, "workspace.json"), "utf8")).includes("private-token"));
  const restored = await recoverMediaArchive({ directory: archive, destinationDirectory: join(directory, "restored") });
  assert.deepEqual(restored.workspace, verified.workspace);
  assert.deepEqual(await readFile(restored.assetFiles[0].path), picture);
  assert.equal(restored.manifest.sourceCoverage, "editionAndAssetsWithExternalSourceReferences");
  await assert.rejects(recoverMediaArchive({ directory: archive, destinationDirectory: archive }), /原始作品包之外/);
  await assert.rejects(recoverMediaArchive({ directory: archive, destinationDirectory: join(archive, "child", "restore") }), /原始作品包之外/);
  assert.deepEqual((await verifyMediaArchive({ directory: archive })).workspace, verified.workspace, "rejected nested recovery must leave the original archive unchanged");
});

test("trusted fixed document can archive referenced assets from another scope while unrelated assets remain rejected", async t => {
  const directory = await mkdtemp(join(tmpdir(), "media-archive-scopes-")); t.after(() => rm(directory, { recursive: true, force: true }));
  const snapshot = workspace(); snapshot.assets[1].scopeId = "another-edition-scope";
  const options = { destinationDirectory: join(directory, "allowed"), workspace: snapshot, exportId: "allowed", canonSnapshot: { snapshotId: "canon-snapshot" }, loadAsset: async () => ({ bytes: picture }) };
  await writeMediaArchive(options);
  const recovered = await verifyMediaArchive({ directory: options.destinationDirectory });
  assert.equal(recovered.workspace.assets[1].scopeId, "another-edition-scope");
  const unknown = { ...snapshot.assets[1], id: "extra", assetRevisionId: "unrelated-revision" };
  await assert.rejects(writeMediaArchive({ ...options, destinationDirectory: join(directory, "denied"), workspace: { ...snapshot, assets: [...snapshot.assets, unknown] } }), /未被固定文档引用/);
});

test("tampering, missing bytes, extra files and cross-project access reject without publishing recovered files", async t => {
  const { directory, archive } = await fixture(t);
  await assert.rejects(verifyMediaArchive({ directory: archive, expectedProjectId: "other" }), /所属作品/);
  const blob = join(archive, "assets", sha(picture));
  await writeFile(blob, "tampered");
  await assert.rejects(recoverMediaArchive({ directory: archive, destinationDirectory: join(directory, "broken") }), /完整性/);
  assert.ok(!(await readdir(directory)).includes("broken"));
  await writeFile(blob, picture);
  await writeFile(join(archive, "extra.txt"), "unknown");
  await assert.rejects(verifyMediaArchive({ directory: archive }), /文件集合/);
  await rm(join(archive, "extra.txt")); await rm(blob);
  await assert.rejects(verifyMediaArchive({ directory: archive }), /ENOENT/);
});

test("malicious manifest path and symbolic-link content are rejected", async t => {
  const { directory, archive } = await fixture(t);
  const manifestPath = join(archive, "manifest.json"), original = await readFile(manifestPath);
  const manifest = JSON.parse(original); manifest.outputs[0].name = "../outside";
  await writeFile(manifestPath, JSON.stringify(manifest));
  await assert.rejects(verifyMediaArchive({ directory: archive }), /路径无效/);
  await writeFile(manifestPath, original);
  const blob = join(archive, "assets", sha(picture)), outside = join(directory, "outside.png");
  await rm(blob); await writeFile(outside, picture); await symlink(outside, blob);
  await assert.rejects(verifyMediaArchive({ directory: archive }), /符号链接/);
});

test("incomplete references, stale canon and interrupted export leave no success directory", async t => {
  const directory = await mkdtemp(join(tmpdir(), "media-archive-reject-")); t.after(() => rm(directory, { recursive: true, force: true }));
  const destinationDirectory = join(directory, "export"), current = workspace();
  const options = { destinationDirectory, workspace: current, exportId: "export", canonSnapshot: { snapshotId: "canon-snapshot" }, loadAsset: async () => ({ bytes: picture }) };
  await assert.rejects(writeMediaArchive({ ...options, canonSnapshot: { snapshotId: "other" } }), /快照不匹配/);
  await assert.rejects(writeMediaArchive({ ...options, workspace: { ...current, assets: [] } }), /缺少文档引用/);
  await assert.rejects(writeMediaArchive({ ...options, loadAsset: async () => { throw Error("磁盘不可读"); } }), /磁盘不可读/);
  assert.deepEqual(await readdir(directory), []);
  const controller = new AbortController(); controller.abort(Error("用户停止"));
  await assert.rejects(writeMediaArchive({ ...options, signal: controller.signal }), /用户停止/);
  assert.deepEqual(await readdir(directory), []);
});

test("real Host cannot bypass archive verification by changing the manifest format and never opens the tampered package", async t => {
  const directory = await mkdtemp(join(tmpdir(), "media-host-archive-integrity-")); t.after(() => rm(directory, { recursive: true, force: true }));
  await importMediaBytes({ bytes: picture, name: "素材.png", rootDirectory: directory, decodeImage: () => ({ width: 1, height: 1 }) });
  const frozen = workspace(), openedPaths = [];
  const host = new MultimediaHost({ directory, connections: {}, chooseFiles: async () => [], executables: {},
    openPath: async path => { openedPaths.push(path); return ""; },
    runtimeRequest: async path => {
      if (path === "/api/projects/project/editions/edition/workspace?revisionId=revision") return frozen;
      if (path === "/api/projects/project/canon?snapshotId=canon-snapshot") return { snapshotId: "canon-snapshot", document: { title: "共同设定", entries: [] } };
      throw Error(`unexpected route ${path}`);
    } });
  const exported = await host.exportEdition("project", "edition", { format: "project", expectedRevisionId: "revision" });
  assert.equal(host.exports.get(exported.id).format, "project", "trusted Host receipt preserves the actual export type");
  const manifestPath = join(directory, "exports", exported.id, "manifest.json"), manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  await writeFile(manifestPath, JSON.stringify({ ...manifest, format: "png", archiveKind: "altered" }));
  await assert.rejects(host.openExport("project", exported.id), /清单或所属作品不匹配/);
  assert.deepEqual(openedPaths, [], "a trusted project receipt forces strict verification even when both disk labels change");
  host.exports.clear();
  await writeFile(manifestPath, JSON.stringify({ ...manifest, format: "png" }));
  await assert.rejects(host.openExport("project", exported.id), /清单或所属作品不匹配/);
  assert.deepEqual(openedPaths, [], "archive marker forces strict verification without an in-memory receipt");
  await writeFile(manifestPath, JSON.stringify({ ...manifest, archiveKind: "altered" }));
  await assert.rejects(host.openExport("project", exported.id), /清单或所属作品不匹配/);
  assert.deepEqual(openedPaths, [], "project format independently requires the current archive contract");
});
