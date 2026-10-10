import test from "node:test";
import assert from "node:assert/strict";
import { spawn, execFile } from "node:child_process";
import { createHash, randomBytes, randomUUID, createCipheriv, createDecipheriv } from "node:crypto";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { promisify } from "node:util";
import { DatabaseSync } from "node:sqlite";
import { PersistentMediaLedger, PersistentTaskBudget } from "@eternallight/agent-engine";
import { MultimediaHost, MediaConnectionStore } from "../media-host.mjs";
import { createMediaExecutionServer } from "../media-execution-server.mjs";
import { verifyMediaArchive, recoverMediaArchive } from "../media-archive.mjs";
import { bindTaskProviderBudget } from "../task-provider-budget.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";

const run = promisify(execFile);
const time = value => ({ value, rate: { numerator: 1000, denominator: 1 } });

// The Runtime, MCP, domain database, Engine budget database, Host file storage
// and callback server are production components. Only provider HTTP and OS
// credential encryption are controlled fixtures; no paid service is contacted.
test("production media Runtime/Host pipeline pins, saves, recovers and binds real outputs", { timeout: 180000, skip: !process.env.NOVEL_CODEX_FFMPEG || !process.env.NOVEL_CODEX_FFPROBE }, async t => {
  const ffmpeg = process.env.NOVEL_CODEX_FFMPEG, ffprobe = process.env.NOVEL_CODEX_FFPROBE;
  assert.ok(ffmpeg && ffprobe, "Set explicit NOVEL_CODEX_FFMPEG and NOVEL_CODEX_FFPROBE for this real-media integration test");
  const directory = await mkdtemp(join(tmpdir(), "media-runtime-host-"));
  const storyDb = join(directory, "story.sqlite"), engineDb = join(directory, "engine.sqlite");
  const mediaDirectory = join(directory, "media"), salt = join(directory, "credential-salt");
  await writeFile(salt, randomBytes(32).toString("base64url"));
  const pngPath = join(directory, "fixture.png"), videoPath = join(directory, "fixture.mp4");
  await run(ffmpeg, ["-v", "error", "-f", "lavfi", "-i", "color=c=red:s=64x64", "-frames:v", "1", "-y", pngPath]);
  await run(ffmpeg, ["-v", "error", "-f", "lavfi", "-i", "color=c=blue:s=1280x720:r=24", "-t", "2", "-pix_fmt", "yuv420p", "-y", videoPath]);
  const png = await readFile(pngPath), video = await readFile(videoPath);
  const author = "media-author-fixture-1234567890123456", candidate = "media-candidate-fixture-1234567890123", capability = "media-capability-fixture-123456789012";
  const key = randomBytes(32), encryption = {
    isEncryptionAvailable: () => true,
    encryptString(text) { const iv = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, iv), body = Buffer.concat([cipher.update(text), cipher.final()]); return Buffer.concat([iv, cipher.getAuthTag(), body]); },
    decryptString(bytes) { const cipher = createDecipheriv("aes-256-gcm", key, bytes.subarray(0, 12)); cipher.setAuthTag(bytes.subarray(12, 28)); return Buffer.concat([cipher.update(bytes.subarray(28)), cipher.final()]).toString(); },
  };
  let origin, runtime, mcp, server, host, activeExecution = null, nextRpc = 0;
  const pending = new Map(), providerCalls = [], boundaries = [], budgets = [];
  const originalFetch = globalThis.fetch;
  const sqlRows = (path, sql, parameters = []) => { const db = new DatabaseSync(path, { readOnly: true }); try { return db.prepare(sql).all(...parameters); } finally { db.close(); } };
  const stopChild = async child => { if (child && child.exitCode === null && child.signalCode === null) { const done = new Promise(resolve => child.once("exit", resolve)); child.kill(); await done; } };
  const stop = async () => { await stopChild(mcp); mcp = null; await stopChild(runtime); runtime = null; await server?.close(); server = null; };
  t.after(async () => { globalThis.fetch = originalFetch; for (const budget of budgets) budget.close(); await stop(); await rm(directory, { recursive: true, force: true }); });
  const authorRequest = async (path, options = {}) => {
    const response = await originalFetch(`${origin}${path}`, { ...options, headers: { Authorization: `Bearer ${author}`, "Content-Type": "application/json", ...options.headers } });
    const value = await response.json(); assert.ok(response.ok, `${options.method ?? "GET"} ${path}: ${JSON.stringify(value)}`);
    if (path.endsWith("/outputs")) boundaries.push("output-pins");
    if (path.endsWith("/assets") && options.method === "POST") boundaries.push("asset");
    if (path.endsWith("/seal")) boundaries.push("seal");
    return value;
  };
  globalThis.fetch = async (url, options) => {
    const address = new URL(String(url));
    if (address.hostname === "127.0.0.1") return originalFetch(url, options);
    if (address.hostname === "fixture.cloudfront.net") return new Response(video, { headers: { "Content-Type": "video/mp4" } });
    assert.equal(address.hostname, "provider.invalid", "test forbids external provider network");
    providerCalls.push({ path: address.pathname, method: options.method });
    if (options.method === "POST") {
      const body = JSON.parse(options.body), prompt = body.prompt ?? body.promptText;
      const intents = sqlRows(storyDb, "SELECT operation_json FROM story_media_operation_intents_v1").map(row => JSON.parse(row.operation_json));
      const pinned = intents.find(item => item.request.prompt === prompt); assert.ok(pinned, "domain intent must precede provider dispatch");
      const record = new PersistentMediaLedger(engineDb).get(pinned.id); assert.equal(record.state, "submitting"); assert.equal(record.dispatchAttempted, true); boundaries.push("provider");
      if (prompt === "uncertain image") throw Error("controlled dropped provider response");
      return address.pathname.endsWith("text_to_video") ? Response.json({ id: "video-job" })
        : Response.json({ data: [{ b64_json: png.toString("base64") }] }, { headers: { "x-request-id": "image-receipt" } });
    }
    assert.equal(address.pathname, "/v1/tasks/video-job");
    return Response.json({ id: "video-job", status: "SUCCEEDED", output: ["https://fixture.cloudfront.net/video.mp4"] });
  };
  const connect = async () => {
    const connections = new MediaConnectionStore({ directory: mediaDirectory, encryption, credentialSaltPath: salt });
    if (!(await connections.list()).connections.length) {
      await connections.save({ id: "images", title: "Images", protocol: "openai-images", baseUrl: "https://provider.invalid/v1", model: "gpt-image-1.5", apiKey: "controlled-image-key" });
      await connections.save({ id: "video", title: "Video", protocol: "runway", baseUrl: "https://provider.invalid", model: "gen4.5", apiKey: "controlled-video-key" });
    }
    host = new MultimediaHost({ runtimeRequest: authorRequest, directory: mediaDirectory, connections,
      chooseFiles: async () => [], openPath: async () => "", renderHtml: async () => { throw Error("not used in this generation test"); },
      executables: { ffmpegPath: ffmpeg, ffprobePath: ffprobe },
      decodeImage: async bytes => { assert.deepEqual(bytes, png); await run(ffmpeg, ["-v", "error", "-i", pngPath, "-frames:v", "1", "-f", "null", "-"]); const { stdout } = await run(ffprobe, ["-v", "error", "-show_streams", "-of", "json", pngPath]); return JSON.parse(stdout).streams[0]; },
    });
    server = await createMediaExecutionServer({ runtimeRequest: authorRequest, multimediaHost: host, databasePath: engineDb, getExecution: () => activeExecution });
    runtime = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0", "--db", storyDb, "--engine-budget-db", engineDb, "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
    let errors = ""; runtime.stderr.on("data", chunk => { errors = `${errors}${chunk}`.slice(-3000); });
    origin = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Error(`Runtime startup timeout: ${errors}`)), 90000);
      const finish = (error, value) => { clearTimeout(timer); error ? reject(error) : resolve(value); };
      createInterface({ input: runtime.stdout }).on("line", line => { if (line.startsWith("Story Runtime listening on ")) finish(null, line.slice("Story Runtime listening on ".length)); });
      runtime.once("error", finish); runtime.once("exit", () => finish(Error(`Runtime exited: ${errors}`)));
      runtime.stdin.end(`${JSON.stringify({ authorToken: author, candidateToken: candidate, mediaHostUrl: server.url, mediaHostToken: server.token })}\n`);
    });
    mcp = spawn(nativeTestBinary("novel-workspace-mcp"), [], { env: { PATH: process.env.PATH, NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidate, NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "advisory" }, stdio: ["pipe", "pipe", "pipe"] });
    mcp.stderr.resume(); createInterface({ input: mcp.stdout }).on("line", line => { const response = JSON.parse(line), waiter = pending.get(response.id); if (waiter) { clearTimeout(waiter.timer); pending.delete(response.id); waiter.resolve(response); } });
  };
  const call = async (binding, name, extra = {}, { errorPattern = null } = {}) => {
    const rpcId = ++nextRpc;
    const response = await new Promise((resolve, reject) => { const timer = setTimeout(() => { pending.delete(rpcId); reject(Error(`${name} timed out`)); }, 60000); pending.set(rpcId, { resolve, timer }); mcp.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: rpcId, method: "tools/call", params: { name, arguments: { ...binding, ...extra }, _meta: { sessionId: binding.threadId, rootThreadId: binding.threadId, callerThreadId: binding.threadId, turnId: "media-test-turn", callId: `media-test-${rpcId}` } } })}\n`); });
    assert.equal(response.error, undefined, JSON.stringify(response.error)); const result = response.result;
    const text = result.content.map(item => item.text ?? "").join("");
    if (errorPattern) { assert.equal(result.isError, true, `${name} must reject this request`); assert.match(text, errorPattern); return text; }
    assert.notEqual(result.isError, true, `${name}: ${text}`); return JSON.parse(text);
  };
  const createWork = async (medium, unitLimits) => {
    const created = await authorRequest("/api/projects", { method: "POST", body: JSON.stringify({ title: `Standalone ${medium}`, logline: "", medium, idempotencyKey: randomUUID() }) });
    assert.equal(created.novelEdition, undefined); const projectId = created.project.id, edition = created.editions[0];
    const workspace = await host.workspace(projectId, edition.id), threadId = randomUUID();
    await authorRequest(`/api/projects/${projectId}/conversations`, { method: "POST", body: JSON.stringify({ scopeId: edition.scopeId, threadId, title: "Media generation" }) });
    const deadlineAt = Math.floor(Date.now() / 1000) + 600;
    const task = await authorRequest(`/api/projects/${projectId}/tasks`, { method: "POST", body: JSON.stringify({ operation: "advisory", inputRevision: workspace.revision, threadId, authorIntent: "Create media and place the approved output in this document.",
      capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: deadlineAt,
      target: { type: "edition", editionId: edition.id, scopeId: edition.scopeId, branchId: edition.branchId, mediaBudget: { maxRequests: 2, unitLimits, maxCostMinor: null, currency: null, expiresAt: deadlineAt * 1000 } },
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId, expectedSourceDigest: workspace.sourceDigest, maxRequests: 4, maxTokens: 1000, deadlineAt } }) });
    activeExecution = { task }; await bindTaskProviderBudget(activeExecution, { runtimeRequest: authorRequest, projectId, taskId: task.id, databasePath: engineDb }); budgets.push(activeExecution.taskBudget);
    return { projectId, edition, workspace, task, budget: activeExecution.taskBudget, binding: { projectId, taskId: task.id, threadId, expectedRevision: workspace.revision } };
  };
  await connect();
  const comic = await createWork("comic", { images: 2 });
  const imageRequest = { providerId: "images", kind: "image.generate", parameters: { prompt: "red comic panel" }, idempotencyKey: "comic-image" };
  const image = await call(comic.binding, "request_media_generation", { request: imageRequest });
  assert.equal(image.state, "ready", JSON.stringify(image.execution)); assert.equal(image.execution.state, "ready"); assert.deepEqual(boundaries, ["provider", "output-pins", "asset", "seal"]);
  const imageId = image.seal.outputs.assetRevisionIds[0]; assert.ok(imageId);
  const same = await call(comic.binding, "request_media_generation", { request: imageRequest }); assert.equal(same.operation.id, image.operation.id); assert.equal(providerCalls.length, 1); assert.equal(comic.budget.snapshot().requests, 1);
  await host.workspace(comic.projectId, comic.edition.id); assert.deepEqual((await host.assetBytes(imageId, comic.projectId, comic.edition.scopeId)).bytes, png);
  const unknown = await call(comic.binding, "request_media_generation", { request: { ...imageRequest, parameters: { prompt: "uncertain image" }, idempotencyKey: "uncertain-image" } });
  assert.equal(unknown.state, "prepared"); assert.equal(unknown.execution.state, "outcome_unknown"); assert.equal(unknown.seal, null); assert.equal(comic.budget.snapshot().requests, 2);
  comic.budget.close(); activeExecution = null; await stop(); PersistentTaskBudget.recoverOrphans(engineDb); await connect();
  const restored = await call(comic.binding, "read_media_generation", { operationId: image.operation.id }); assert.equal(restored.state, "ready"); assert.equal(restored.seal.outputs.assetRevisionIds[0], imageId);
  const replayed = await call(comic.binding, "request_media_generation", { request: imageRequest }); assert.equal(replayed.operation.id, image.operation.id); assert.equal(replayed.state, "ready");
  const uncertain = await call(comic.binding, "wait_media_generation", { operationId: unknown.operation.id }); assert.equal(uncertain.operation.id, unknown.operation.id); assert.equal(uncertain.execution.state, "outcome_unknown"); assert.equal(providerCalls.filter(item => item.method === "POST").length, 2); assert.equal(uncertain.seal, null);
  const comicDocument = { kind: "comic", title: "Comic", format: "pages", pages: [{ id: "page", title: "Opening", width: 200, height: 300, readingDirection: "leftToRight", panels: [{ id: "panel", description: "A red square", rect: { x: 0, y: 0, width: 100, height: 100 }, assetRevisionId: imageId, referenceAssetRevisionIds: [], balloons: [], locked: false }] }] };
  const comicSaved = await call(comic.binding, "save_media_document", { request: { expectedRevisionId: null, document: comicDocument, idempotencyKey: "bind-comic-asset", readSet: [], sources: [] } });
  assert.ok(comicSaved.revisionId); assert.equal((await host.workspace(comic.projectId, comic.edition.id)).document.pages[0].panels[0].assetRevisionId, imageId);
  const animation = await createWork("animation", { videoMilliseconds: 2000 });
  const videoRequest = { providerId: "video", kind: "video.generate", parameters: { prompt: "moving blue panel", duration: 2, ratio: "1280:720" }, idempotencyKey: "animation-video" };
  const submitted = await call(animation.binding, "request_media_generation", { request: videoRequest }); assert.equal(submitted.execution.state, "submitted", JSON.stringify(submitted));
  await new Promise(resolve => setTimeout(resolve, Math.max(0, submitted.execution.nextPollAt - Date.now() + 20)));
  const ready = await call(animation.binding, "wait_media_generation", { operationId: submitted.operation.id }); assert.equal(ready.state, "ready", JSON.stringify(ready.execution)); const videoId = ready.seal.outputs.assetRevisionIds[0];
  await host.workspace(animation.projectId, animation.edition.id); const storedVideo = await host.assetBytes(videoId, animation.projectId, animation.edition.scopeId); assert.deepEqual(storedVideo.bytes, video); assert.equal(storedVideo.duration.value, 2000000);
  const longDescription = "暮色🙂e\u0301，雨。\n".repeat(1800), descriptionChars = Array.from(longDescription);
  const sha = value => createHash("sha256").update(value, "utf8").digest("hex");
  const animationDocument = { kind: "animation", title: "Animation", width: 64, height: 64, frameRate: { numerator: 24, denominator: 1 }, shots: [{ id: "shot", title: "Shot", description: longDescription, assetRevisionId: videoId, referenceAssetRevisionIds: [], locked: false }], timeline: { tracks: [{ id: "track", title: "Video", kind: "video", muted: false, clips: [{ id: "clip", shotId: "shot", assetRevisionId: videoId, start: time(0), duration: time(2000), sourceStart: time(0), playbackRate: { numerator: 1, denominator: 1 }, caption: "" }] }] } };
  const saved = await call(animation.binding, "save_media_document", { request: { expectedRevisionId: null, document: animationDocument, idempotencyKey: "bind-video-asset", readSet: [], sources: [] } }); assert.ok(saved.revisionId);
  assert.equal((await host.workspace(animation.projectId, animation.edition.id)).document.timeline.tracks[0].clips[0].assetRevisionId, videoId);
  const patched = await call(animation.binding, "patch_media_document", { request: { expectedRevisionId: saved.revisionId, idempotencyKey: "retitle-shot", readSet: [], sources: [],
    operations: [{ kind: "upsertShot", shot: { id: "shot", title: "Updated shot", description: longDescription, assetRevisionId: videoId, referenceAssetRevisionIds: [] } }] } });
  assert.ok(patched.revisionId);
  const textField = { kind: "shotDescription", shotId: "shot" }, pages = [];
  const shotOverview = await call(animation.binding, "read_media_workspace", { version: "working", section: "shots", offset: 0, limit: 20 });
  assert.equal(shotOverview.items[0].description, undefined, "structural reads expose a descriptor instead of all long text");
  assert.equal(shotOverview.items[0].textFields[0].textDigest, sha(longDescription));
  let charOffset = 0;
  do {
    const page = await call(animation.binding, "read_media_workspace", { version: "working", section: "text", textField, charOffset, charLimit: 8000 });
    assert.equal(page.unit, "unicodeScalar"); assert.equal(page.startChar, charOffset);
    assert.equal(page.text, descriptionChars.slice(page.startChar, page.endChar).join(""));
    assert.equal(page.textDigest, sha(longDescription)); assert.equal(page.sliceDigest, sha(page.text));
    assert.equal(page.totalChars, descriptionChars.length); assert.equal(page.source.revisionId, patched.revisionId);
    assert.equal(page.returnedSource.objectId, animation.edition.id); assert.equal(page.returnedSource.selector.kind, "fieldText");
    assert.equal(page.returnedSource.selector.quote, page.text);
    assert.ok(Buffer.byteLength(JSON.stringify(page)) <= 64 * 1024, "actual Unicode page must fit its transport budget");
    assert.ok(page.endChar > charOffset, "nonempty long text paging must progress");
    pages.push(page); charOffset = page.nextCharOffset;
  } while (charOffset !== null);
  assert.ok(pages.length >= 3); assert.equal(pages.map(page => page.text).join(""), longDescription);
  const firstPage = pages[0];
  const exactSourcePage = await call(animation.binding, "read_media_source", { source: firstPage.returnedSource, section: "text", textField, charOffset: 2, charLimit: 4 });
  assert.equal(exactSourcePage.text, descriptionChars.slice(2, 6).join(""), "a task can reread its proven own-save field source");
  await call(animation.binding, "read_media_source", { source: firstPage.returnedSource, section: "text", textField: { kind: "clipCaption", trackId: "track", clipId: "clip" }, charOffset: 0, charLimit: 1 }, { errorPattern: /TaskOutputForbidden/ });
  const authorPage = await host.textPage(animation.projectId, animation.edition.id, { revisionId: patched.revisionId, field: textField, charOffset: 2, charLimit: 4 });
  assert.equal(authorPage.text, exactSourcePage.text); assert.equal(authorPage.source.digest, patched.documentSource.digest);
  await assert.rejects(() => host.textPage(animation.projectId, animation.edition.id, { revisionId: patched.revisionId, field: textField, charOffset: descriptionChars.length + 1, charLimit: 1 }), /media_text_offset_invalid/);
  await assert.rejects(() => host.textPage(animation.projectId, animation.edition.id, { revisionId: comicSaved.revisionId, field: textField, charOffset: 0, charLimit: 1 }));
  await call(animation.binding, "read_media_source", { source: comicSaved.documentSource, section: "text", textField, charOffset: 0, charLimit: 1 }, { errorPattern: /TaskOutputForbidden/ });
  const replacement = "晨光🪷\n", selected = descriptionChars.slice(2, 8).join("");
  const replaceOperation = { kind: "replaceTextRanges", field: textField, expectedTextDigest: firstPage.textDigest,
    edits: [{ startChar: 2, endChar: 8, expectedSliceDigest: sha(selected), replacement }] };
  const readSet = [{ source: firstPage.returnedSource, freshness: "pinned" }];
  await call(animation.binding, "patch_media_document", { request: { expectedRevisionId: patched.revisionId, idempotencyKey: "reject-wrong-text-digest", readSet, sources: [],
    operations: [{ ...replaceOperation, expectedTextDigest: "0".repeat(64) }] } }, { errorPattern: /media_text_digest_changed/ });
  assert.equal((await host.workspace(animation.projectId, animation.edition.id)).revisionId, patched.revisionId, "rejected text edit commits no revision");
  const beforeTextExport = await host.exportEdition(animation.projectId, animation.edition.id, { format: "project", expectedRevisionId: patched.revisionId });
  const textSourceLink = { relation: "derivedFrom", source: firstPage.returnedSource, targetObjectId: animation.edition.id,
    targetSelector: { kind: "shot", shotId: "shot" } };
  const textRequest = { expectedRevisionId: patched.revisionId, idempotencyKey: "replace-exact-unicode-range", readSet, sources: [textSourceLink], operations: [replaceOperation] };
  const edited = await call(animation.binding, "patch_media_document", { request: textRequest });
  const repeatedEdit = await call(animation.binding, "patch_media_document", { request: textRequest });
  assert.equal(repeatedEdit.revisionId, edited.revisionId, "identical retry returns the actual committed revision");
  const expectedDescription = [...descriptionChars.slice(0, 2), ...Array.from(replacement), ...descriptionChars.slice(8)].join("");
  const editedWorkspace = await host.workspace(animation.projectId, animation.edition.id, undefined, edited.revisionId);
  assert.equal(editedWorkspace.document.shots[0].description, expectedDescription); assert.deepEqual(editedWorkspace.document.timeline, animationDocument.timeline);
  const savedReadSet = sqlRows(storyDb, "SELECT dependency_json FROM story_media_read_dependencies_v1 WHERE revision_id=? ORDER BY ordinal", [edited.revisionId]).map(row => JSON.parse(row.dependency_json));
  assert.deepEqual(savedReadSet, readSet, "the exact text leaf remains a persisted source dependency");
  const unchangedHistoricalPage = await host.textPage(animation.projectId, animation.edition.id, { revisionId: patched.revisionId, field: textField, charOffset: 2, charLimit: 4 });
  const preservedSourcePage = await call(animation.binding, "read_media_source", { source: firstPage.returnedSource, section: "text", textField, charOffset: 2, charLimit: 4 });
  const changedCurrentPage = await host.textPage(animation.projectId, animation.edition.id, { revisionId: edited.revisionId, field: textField, charOffset: 2, charLimit: Array.from(replacement).length });
  assert.equal(unchangedHistoricalPage.text, descriptionChars.slice(2, 6).join(""));
  assert.equal(preservedSourcePage.text, unchangedHistoricalPage.text, "a saved source remains fixed after a later task-owned edit");
  assert.equal(changedCurrentPage.text, replacement);
  const afterTextExport = await host.exportEdition(animation.projectId, animation.edition.id, { format: "project", expectedRevisionId: edited.revisionId });
  const beforeTextArchive = await verifyMediaArchive({ directory: join(mediaDirectory, "exports", beforeTextExport.id) });
  const afterTextArchive = await verifyMediaArchive({ directory: join(mediaDirectory, "exports", afterTextExport.id) });
  assert.equal(beforeTextArchive.workspace.document.shots[0].description, longDescription);
  assert.equal(afterTextArchive.workspace.document.shots[0].description, expectedDescription);
  assert.equal(afterTextArchive.workspace.documentEncoding, editedWorkspace.documentEncoding);
  assert.deepEqual(afterTextArchive.workspace.sources, [textSourceLink], "the archive preserves the new fieldText source link without widening its selection");
  assert.equal(sha(afterTextArchive.workspace.documentEncoding), edited.documentSource.digest);
  assert.notEqual(beforeTextArchive.workspace.documentSource.digest, afterTextArchive.workspace.documentSource.digest);
  const beforeCanon = await host.workspace(animation.projectId, animation.edition.id);
  assert.equal(beforeCanon.document.shots[0].title, "Updated shot");
  assert.deepEqual(beforeCanon.document.timeline, animationDocument.timeline, "a real MCP patch preserves unread timeline clips");
  assert.equal(createHash("sha256").update(beforeCanon.documentEncoding).digest("hex"), beforeCanon.documentSource.digest);
  const canon = await host.canon(animation.projectId);
  const newCanon = await host.saveCanon(animation.projectId, { expectedRevisionId: canon.revisionId, idempotencyKey: "shared-blue-setting",
    document: { title: "Shared setting", entries: [{ id: "blue-world", category: "world", statement: "The opening takes place in a blue world.", sources: [beforeCanon.documentSource] }] } });
  assert.notEqual(newCanon.snapshotId, beforeCanon.edition.canonSnapshotId);
  assert.equal((await host.sourceImpacts(animation.projectId, animation.edition.id)).canon.changed, true);
  const adopted = await host.patch(animation.projectId, animation.edition.id, { expectedRevisionId: beforeCanon.revisionId, idempotencyKey: "adopt-shared-setting",
    operations: [{ kind: "adoptCanonSnapshot", canonSnapshotId: newCanon.snapshotId }], readSet: [], sources: [] });
  assert.notEqual(adopted.revisionId, beforeCanon.revisionId); assert.equal(adopted.edition.canonSnapshotId, newCanon.snapshotId);
  assert.equal(adopted.documentEncoding, beforeCanon.documentEncoding, "adopting a setting does not rewrite saved media bytes");
  const historical = await host.workspace(animation.projectId, animation.edition.id, undefined, beforeCanon.revisionId);
  assert.equal(historical.edition.canonSnapshotId, beforeCanon.edition.canonSnapshotId);
  const oldExport = await host.exportEdition(animation.projectId, animation.edition.id, { format: "project", expectedRevisionId: historical.revisionId });
  const newExport = await host.exportEdition(animation.projectId, animation.edition.id, { format: "project", expectedRevisionId: adopted.revisionId });
  const oldArchive = await verifyMediaArchive({ directory: join(mediaDirectory, "exports", oldExport.id), expectedProjectId: animation.projectId });
  const newArchive = await verifyMediaArchive({ directory: join(mediaDirectory, "exports", newExport.id), expectedProjectId: animation.projectId });
  assert.equal(oldArchive.workspace.edition.canonSnapshotId, historical.edition.canonSnapshotId);
  assert.equal(newArchive.canonSnapshot.snapshotId, newCanon.snapshotId);
  assert.equal(newArchive.workspace.documentEncoding, adopted.documentEncoding);
  assert.deepEqual(newExport.assetRevisionIds, [videoId]);
  await host.openExport(animation.projectId, newExport.id);
  const recovery = join(directory, "recovered-animation");
  await recoverMediaArchive({ directory: join(mediaDirectory, "exports", newExport.id), destinationDirectory: recovery });
  assert.equal((await verifyMediaArchive({ directory: recovery })).workspace.revisionId, adopted.revisionId);
  assert.equal(providerCalls.filter(item => item.method === "POST").length, 3); assert.equal(animation.budget.snapshot().requests, 1);
  assert.equal(sqlRows(storyDb, "SELECT * FROM story_media_assets_v1").length, 2);
  assert.equal(sqlRows(storyDb, "SELECT * FROM story_media_imports_v1 WHERE status='registered'").length, 2);
  assert.equal(new PersistentMediaLedger(engineDb).get(unknown.operation.id).state, "outcome_unknown");
});
