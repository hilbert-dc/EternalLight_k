import { mkdir, open, rename, rm } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { dirname } from "node:path";

function profileFrom(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)
    || typeof value.name !== "string" || !value.name.trim() || value.name.length > 40
    || typeof value.email !== "string" || value.email.length > 254 || !/^\S+@\S+\.\S+$/.test(value.email)
    || typeof value.passwordSalt !== "string" || !value.passwordSalt || value.passwordSalt.length > 256
    || typeof value.passwordHash !== "string" || !/^[a-f0-9]{64}$/.test(value.passwordHash)) {
    throw new Error("本机作者资料无效，请检查会话文件。");
  }
  return { name: value.name.trim(), email: value.email.trim(), passwordSalt: value.passwordSalt, passwordHash: value.passwordHash };
}

// The sole author-session authority. Reads and mutations share a queue so a
// reload cannot observe half a write or undo an explicit sign-out.
export class AuthorSessionStore {
  #path;
  #queue = Promise.resolve();
  constructor(path) { this.#path = path; }

  #serialize(operation) {
    const result = this.#queue.then(operation);
    this.#queue = result.catch(() => {});
    return result;
  }

  async #read() {
    let file;
    try { file = await open(this.#path, "r"); }
    catch (error) {
      if (error.code === "ENOENT") return { profile: null, signedIn: false };
      throw new Error("无法读取本机作者会话，请检查应用数据目录的权限。", { cause: error });
    }
    try {
      if ((await file.stat()).size > 4096) throw new Error("会话文件过大");
      const value = JSON.parse(await file.readFile("utf8"));
      if (!value || typeof value.signedIn !== "boolean" || (value.profile === null && value.signedIn)) throw new Error("会话状态无效");
      return { profile: value.profile === null ? null : profileFrom(value.profile), signedIn: value.signedIn };
    } catch (error) {
      throw new Error("本机作者会话损坏，无法恢复登录。请检查会话文件后重试。", { cause: error });
    } finally { await file.close(); }
  }

  async #write(snapshot) {
    const temporary = `${this.#path}.${randomUUID()}.tmp`;
    try {
      await mkdir(dirname(this.#path), { recursive: true, mode: 0o700 });
      const file = await open(temporary, "wx", 0o600);
      try { await file.writeFile(JSON.stringify(snapshot)); await file.sync(); }
      finally { await file.close(); }
      await rename(temporary, this.#path);
      return snapshot;
    } catch (error) {
      throw new Error("无法保存本机作者会话，请检查应用数据目录的权限。", { cause: error });
    } finally { await rm(temporary, { force: true }).catch(() => {}); }
  }

  read() { return this.#serialize(() => this.#read()); }
  save(profile) {
    const valid = profileFrom(profile);
    return this.#serialize(() => this.#write({ profile: valid, signedIn: true }));
  }
  signOut() {
    return this.#serialize(async () => this.#write({ ...(await this.#read()), signedIn: false }));
  }
  clear() { return this.#serialize(() => this.#write({ profile: null, signedIn: false })); }
}

export function registerAuthorSessionHandlers(ipc, store) {
  ipc.handle("author:read", () => store.read());
  ipc.handle("author:save", (_event, profile) => store.save(profile));
  ipc.handle("author:sign-out", () => store.signOut());
  ipc.handle("author:clear", () => store.clear());
}
