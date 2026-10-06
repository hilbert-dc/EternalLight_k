import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { AuthorSessionStore } from "../author-session-store.mjs";

const profile = { name: "测试作者", email: "author@example.invalid", passwordSalt: "local-only-salt", passwordHash: "a".repeat(64) };
const signedIn = { profile, signedIn: true };
const empty = { profile: null, signedIn: false };
async function fixture(t) {
  const directory = await mkdtemp(join(tmpdir(), "novel-author-session-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return join(directory, "author-session.json");
}

test("a new store restores a signed-in author and persists explicit logout and deletion", async t => {
  const path = await fixture(t);
  const first = new AuthorSessionStore(path);
  assert.deepEqual(await first.read(), empty);
  assert.deepEqual(await first.save(profile), signedIn);
  assert.deepEqual(await new AuthorSessionStore(path).read(), signedIn);
  assert.deepEqual(await first.signOut(), { profile, signedIn: false });
  const restarted = new AuthorSessionStore(path);
  assert.deepEqual(await restarted.read(), { profile, signedIn: false });
  await restarted.save(profile);
  await restarted.clear();
  assert.deepEqual(await new AuthorSessionStore(path).read(), empty);
  if (process.platform !== "win32") assert.equal((await stat(path)).mode & 0o777, 0o600);
});

test("queued save, logout and read never restore a stale signed-in snapshot", async t => {
  const store = new AuthorSessionStore(await fixture(t));
  const saved = store.save(profile);
  const loggedOut = store.signOut();
  const read = store.read();
  assert.deepEqual(await Promise.all([saved, loggedOut, read]), [signedIn, { profile, signedIn: false }, { profile, signedIn: false }]);
});

test("a failed atomic replacement keeps the existing file and queue usable", async t => {
  const path = await fixture(t);
  await mkdir(path);
  const store = new AuthorSessionStore(path);
  await assert.rejects(store.save(profile), /无法保存/);
  assert.equal((await stat(path)).isDirectory(), true);
  await rm(path, { recursive: true });
  assert.deepEqual(await store.save(profile), signedIn);
  assert.deepEqual(JSON.parse(await readFile(path, "utf8")), signedIn);
});

for (const contents of ["not json", JSON.stringify({ profile: null, signedIn: true }), JSON.stringify({ profile, signedIn: "true" }), " ".repeat(4097)]) {
  test("an invalid session reports a restore error instead of silently creating an author", async t => {
    const path = await fixture(t);
    await writeFile(path, contents);
    await assert.rejects(new AuthorSessionStore(path).read(), /会话损坏/);
    assert.equal(await readFile(path, "utf8"), contents);
  });
}

test("invalid profile input cannot replace an existing valid session", async t => {
  const path = await fixture(t);
  const store = new AuthorSessionStore(path);
  await store.save(profile);
  for (const invalid of [null, {}, { ...profile, name: " " }, { ...profile, name: "长".repeat(41) }, { ...profile, passwordHash: "plain-password" }]) {
    assert.throws(() => store.save(invalid), /作者资料无效/);
  }
  assert.deepEqual(await store.read(), signedIn);
});
