import { randomUUID } from "node:crypto";
import { mkdir, open, readFile, rename } from "node:fs/promises";
import { dirname } from "node:path";

const actions = new Set(["pause", "pauseAfterStep", "resume", "cancel", "recoverSavedResult"]);
const identity = value => typeof value === "string" && value.length > 0 && value.length <= 160 && !/[\p{Cc}]/u.test(value);
const rejectionReasons = {
  TaskBindingInvalid: "任务归属或对话已变化", WorkTaskStale: "任务已有更新操作",
  TaskNotWritable: "当前任务状态不允许此操作", TaskInputStale: "任务输入已变化，需要先核对",
  WorkUsageUnknown: "先前执行的用量尚未核清", AuthoringRunStale: "创作进度已有更新",
};
const failure = (code, message) => Object.assign(new Error(message), { code });
const basePath = command => `/api/projects/${encodeURIComponent(command.projectId)}/work-tasks/${encodeURIComponent(command.workTaskId)}`;
const same = (a, b) => ["projectId", "workTaskId", "threadId", "command", "expectedControlRevision"].every(key => a[key] === b[key]);
const valid = command => identity(command?.commandId) && identity(command.projectId) && identity(command.workTaskId)
  && typeof command.threadId === "string" && command.threadId.length <= 160 && !/[\p{Cc}]/u.test(command.threadId)
  && actions.has(command.command) && Number.isSafeInteger(command.expectedControlRevision) && command.expectedControlRevision >= 0;

// This is a transmission outbox, not a second Work/command authority. Only a
// matching Native receipt can remove an uncertain command. Restart reads never
// resume a model or silently manufacture a replacement command ID.
export class WorkControlCommands {
  #request; #path; #loaded; #records = new Map(); #writes = Promise.resolve(); #inflight = new Map();
  constructor({ request, path }) { this.#request = request; this.#path = path; }
  async #load() {
    this.#loaded ??= (async () => {
      let text;
      try { text = await readFile(this.#path(), "utf8"); } catch (error) { if (error.code === "ENOENT") return; throw error; }
      const data = JSON.parse(text);
      if (!Array.isArray(data.commands) || Object.keys(data).join(",") !== "commands" || data.commands.some(command => !valid(command))
        || new Set(data.commands.map(command => command.commandId)).size !== data.commands.length) throw new Error("待确认控制记录损坏；没有重发任何控制。");
      for (const command of data.commands) this.#records.set(command.commandId, command);
    })();
    return this.#loaded;
  }
  async #save() {
    const payload = JSON.stringify({ commands: [...this.#records.values()] });
    const target = this.#path();
    const next = this.#writes.catch(() => {}).then(async () => {
      await mkdir(dirname(target), { recursive: true });
      const temporary = `${target}.${randomUUID()}.tmp`;
      const file = await open(temporary, "wx", 0o600);
      try { await file.writeFile(payload); await file.sync(); } finally { await file.close(); }
      await rename(temporary, target);
      // Directory fsync is supported on POSIX; Windows does not expose it.
      if (process.platform !== "win32") {
        const directory = await open(dirname(target), "r");
        try { await directory.sync(); } finally { await directory.close(); }
      }
    });
    this.#writes = next;
    return next;
  }
  #verify(command, receipt) {
    if (!receipt || receipt.commandId !== command.commandId || !same(command, receipt)
      || !["applied", "rejected"].includes(receipt.status)
      || !Number.isSafeInteger(receipt.workVersion) || !Number.isSafeInteger(receipt.createdAt)
      || (receipt.status === "applied" && receipt.appliedControlRevision !== command.expectedControlRevision + 1)
      || (receipt.status === "rejected" && (receipt.appliedControlRevision !== null || typeof receipt.reason !== "string"))) {
      throw failure("WORK_CONTROL_RECEIPT_INVALID", "任务控制回执无法核对；保留原命令等待核对。");
    }
  }
  async #lookup(command) {
    const result = await this.#request(`${basePath(command)}/control-commands/${encodeURIComponent(command.commandId)}`);
    if (!result || !Object.hasOwn(result, "receipt")) throw new Error("控制查询回执无效");
    if (result.receipt === null) return null;
    this.#verify(command, result.receipt);
    return result.receipt;
  }
  async #forget(command) {
    this.#records.delete(command.commandId);
    try { await this.#save(); } catch (error) { this.#records.set(command.commandId, command); throw error; }
  }
  async send(work, action) {
    await this.#load();
    const target = { projectId: work.projectId, workTaskId: work.id, threadId: work.threadId, command: action,
      expectedControlRevision: work.controlRevision };
    if (!valid({ ...target, commandId: "pending" })) throw new Error("任务控制身份无效");
    const key = JSON.stringify(target);
    if (this.#inflight.has(key)) return this.#inflight.get(key);
    const promise = this.#send(target);
    this.#inflight.set(key, promise);
    try { return await promise; } finally { this.#inflight.delete(key); }
  }
  async #send(target) {
    const unresolved = [...this.#records.values()].find(item => item.projectId === target.projectId && item.workTaskId === target.workTaskId);
    if (unresolved && !same(unresolved, target)) {
      await this.reconcile(target.projectId);
      if (this.#records.has(unresolved.commandId)) throw failure("WORK_CONTROL_DELIVERY_UNKNOWN", "原任务还有控制待核对；尚未提交另一条控制。");
    }
    const command = unresolved && same(unresolved, target) ? unresolved : { commandId: randomUUID(), ...target };
    this.#records.set(command.commandId, command);
    // No HTTP mutation before the exact envelope is durable.
    await this.#save();
    const { commandId, threadId, expectedControlRevision, command: action } = command;
    let outcome, reconciled = false;
    try {
      outcome = await this.#request(`${basePath(command)}/control`, { method: "POST", body: JSON.stringify({ commandId, threadId, expectedControlRevision, command: action }) });
    } catch (original) {
      let receipt;
      try { receipt = await this.#lookup(command); } catch { /* Keep unknown; never replace the command. */ }
      if (!receipt) throw failure("WORK_CONTROL_DELIVERY_UNKNOWN", `任务控制尚未确认，原命令已保留；刷新任务会核对回执。${original instanceof Error ? ` ${original.message}` : ""}`);
      outcome = { receipt, work: await this.#request(basePath(command)), replayed: true };
      reconciled = true;
    }
    this.#verify(command, outcome?.receipt);
    if (typeof outcome.replayed !== "boolean") throw failure("WORK_CONTROL_RECEIPT_INVALID", "控制结果尚未核清；原命令已保留，本次没有再次启动执行。");
    const current = outcome.work;
    if (!current || current.id !== command.workTaskId || current.projectId !== command.projectId
      || (outcome.receipt.status === "applied" && current.threadId !== command.threadId) || !Number.isSafeInteger(current.controlRevision)) {
      throw failure("WORK_CONTROL_RECEIPT_INVALID", "任务控制快照归属无法核对；保留原命令。");
    }
    await this.#forget(command);
    if (outcome.receipt.status === "rejected") throw failure("WORK_CONTROL_REJECTED", `原任务控制被拒绝：${rejectionReasons[outcome.receipt.reason] ?? "当前条件不允许此操作"}。请根据最新任务状态重新操作。`);
    if (current.controlRevision !== outcome.receipt.appliedControlRevision) throw failure("WORK_CONTROL_SUPERSEDED", "该控制曾经生效，但原任务已有更新操作；没有控制后来的执行。");
    if ((reconciled || outcome.replayed) && ["resume", "recoverSavedResult"].includes(action)) {
      throw failure("WORK_CONTROL_RECONCILED", "已核对原控制生效；请查看原任务当前状态。本次没有重复启动执行。");
    }
    return current;
  }
  async reconcile(projectId) {
    await this.#load();
    for (const command of [...this.#records.values()].filter(item => item.projectId === projectId)) {
      if ([...this.#inflight.keys()].some(key => same(JSON.parse(key), command))) continue;
      try { if (await this.#lookup(command)) await this.#forget(command); } catch { /* Unknown stays durable and visible. */ }
    }
  }
  async project(work) {
    await this.#load();
    const pending = [...this.#records.values()].find(item => item.projectId === work.projectId && item.workTaskId === work.id);
    return pending ? { ...work, controlDelivery: { commandId: pending.commandId, command: pending.command, state: "deliveryUnknown" } } : work;
  }
}
