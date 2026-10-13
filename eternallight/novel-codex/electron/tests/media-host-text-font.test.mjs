import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { MultimediaHost, registerMultimediaHandlers } from "../media-host.mjs";

test("Host text pages preserve the exact requested revision and reject mismatched authority receipts", async () => {
  const requests = [], request = { revisionId: "fixed-revision", branchId: "branch", field: { kind: "shotDescription", shotId: "shot" }, charOffset: 3, charLimit: 10 };
  const source = { projectId: "project", objectId: "edition", revisionId: "fixed-revision", branchId: "branch" };
  let response = { source, unit: "unicodeScalar" };
  const host = new MultimediaHost({ executables: {}, runtimeRequest: async (path, options) => { requests.push({ path, body: JSON.parse(options.body) }); return response; } });
  assert.deepEqual(await host.textPage("project", "edition", request), response);
  assert.equal(requests[0].path, "/api/projects/project/editions/edition/text-page?branchId=branch");
  assert.deepEqual(requests[0].body, { revisionId: "fixed-revision", field: request.field, charOffset: 3, charLimit: 10 });
  for (const [field, value] of [["projectId", "other-project"], ["objectId", "other-edition"], ["revisionId", "HEAD"], ["branchId", "other-branch"]]) {
    response = { source: { ...source, [field]: value }, unit: "unicodeScalar" };
    await assert.rejects(host.textPage("project", "edition", request), /不一致/);
  }
  response = { source, unit: "utf16" };
  await assert.rejects(host.textPage("project", "edition", request), /不一致/);
  await assert.rejects(host.textPage("project", "edition", { ...request, revisionId: "../outside" }), /无效/);
});

test("subtitle font preview exposes only configured bytes behind token and content digest", async t => {
  const directory = await mkdtemp(join(tmpdir(), "media-font-preview-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const fontPath = join(directory, "configured-font.ttf"), bytes = Buffer.alloc(32); bytes.writeUInt32BE(0x00010000);
  await writeFile(fontPath, bytes);
  const host = new MultimediaHost({ directory, executables: { fontPath } });
  const metadata = await host.subtitleFont();
  assert.ok(/^[a-f0-9]{64}$/.test(metadata.digest));
  assert.ok(!metadata.previewUrl.includes(directory));
  const result = await host.preview(new Request(metadata.previewUrl));
  assert.equal(result.status, 200);
  assert.equal(result.headers.get("content-type"), "font/sfnt");
  assert.equal(result.headers.get("cache-control"), "private, no-store");
  assert.deepEqual(Buffer.from(await result.arrayBuffer()), bytes);
  for (const url of [metadata.previewUrl.replace(host.previewToken, "wrong-token"), metadata.previewUrl.replace("creative-media:", "https:"), `${metadata.previewUrl}?file=outside`, `creative-media://${host.previewToken}/fonts/%2E%2E%2Fsecret`]) {
    assert.equal((await host.preview(new Request(url))).status, 404);
  }
  assert.equal((await host.preview(new Request(metadata.previewUrl, { method: "POST" }))).status, 404);
  bytes[31] = 1; await writeFile(fontPath, bytes);
  assert.equal((await host.preview(new Request(metadata.previewUrl))).status, 404, "a changed configured font cannot be served as its previous content");
  assert.notEqual((await host.subtitleFont()).digest, metadata.digest);
  await writeFile(fontPath, "not a font file at all");
  await assert.rejects(host.subtitleFont(), /格式无效/);
  host.executables.fontPath = null;
  assert.equal(await host.subtitleFont(), null);
});

test("current preload channels dispatch font and fixed-revision text through the trusted Host", async () => {
  const handlers = new Map(), calls = [];
  const host = { textPage: async (...args) => calls.push(args), subtitleFont: async () => ({ previewUrl: "private-font", digest: "fixed" }) };
  registerMultimediaHandlers({ handle: (channel, handler) => handlers.set(channel, handler) }, host);
  const request = { revisionId: "fixed", field: { kind: "shotDescription", shotId: "shot" } };
  await handlers.get("media:read-text-page")({}, "project", "edition", request);
  assert.deepEqual(calls, [["project", "edition", request]]);
  assert.deepEqual(await handlers.get("media:read-subtitle-font")({}), { previewUrl: "private-font", digest: "fixed" });
});

test("source catalog IPC fixes the snapshot on continuation and rejects invalid ranges before transport", async () => {
  const paths = [], handlers = new Map();
  const host = new MultimediaHost({ executables: {}, runtimeRequest: async path => { paths.push(path); return { items: [] }; } });
  registerMultimediaHandlers({ handle: (name, handler) => handlers.set(name, handler) }, host);
  const read = (...args) => handlers.get("media:source-catalog")({}, ...args);
  await read("project", "edition");
  await read("project", "edition", "snapshot-fixed", 20, 10);
  assert.equal(paths[1], "/api/projects/project/editions/edition/source-catalog?offset=20&limit=10&snapshotId=snapshot-fixed");
  for (const args of [["project","edition",undefined,1], ["project","edition","../escape"], ["project","edition",undefined,0,101], ["project","edition",undefined,-1], ["../project","edition"]]) await assert.rejects(read(...args));
  assert.equal(paths.length, 2);
});
