const id = value => typeof value === "string" && value.trim() === value && value.length > 0 && Buffer.byteLength(value) <= 160 && !/[\p{Cc}]/u.test(value);
export function validateConversationId(value) {
  if (!id(value)) throw Error("对话 ID 无效");
  return value;
}
const collection = projectId => `/api/projects/${encodeURIComponent(validateConversationId(projectId))}/conversations`;
export async function readConversation(runtimeRequest, projectId, threadId, writable = false) {
  const record = await runtimeRequest(`${collection(projectId)}/${encodeURIComponent(validateConversationId(threadId))}`);
  if (record?.id !== threadId || record.projectId !== projectId) throw Error("对话不属于当前作品");
  if (writable && record.isArchived) throw Error("该对话已归档，请先恢复后继续");
  return record;
}
export function conversationSummary(record) {
  return { ...record, createdAt: Math.floor(record.createdAt / 1000), updatedAt: Math.floor(record.updatedAt / 1000), name: record.title, preview: "", status: "idle", modelProvider: null, ephemeral: false };
}
export function registerConversationHandlers(ipcMain, { runtimeRequest, adapter, isBusy, exclusive, readHistory }) {
  const pendingCreations = new Map();
  const patch = async (projectId, threadId, changes) => {
    return (async () => {
      const record = await readConversation(runtimeRequest, projectId, threadId);
      const updated = await runtimeRequest(`${collection(projectId)}/${encodeURIComponent(threadId)}`, {
        method: "PATCH", body: JSON.stringify({ expectedVersion: record.version, ...changes }),
      });
      if (updated?.id !== threadId || updated.projectId !== projectId) throw Error("对话更新回执不匹配");
      return { thread: conversationSummary(updated) };
    })();
  };
  ipcMain.handle("agent:create-thread", async (_event, projectId, options = {}) => {
    collection(projectId);
    if (!options || Array.isArray(options) || typeof options !== "object" || Object.keys(options).some(key => !["sourceThreadId", "lastTurnId", "scopeId"].includes(key))
      || Boolean(options.sourceThreadId) !== Boolean(options.lastTurnId)) throw Error("新建对话参数无效");
    if (options.sourceThreadId) { validateConversationId(options.lastTurnId); await readConversation(runtimeRequest, projectId, options.sourceThreadId); }
    if (options.scopeId !== undefined) validateConversationId(options.scopeId);
    return exclusive(async () => {
      const key = JSON.stringify([projectId, options.scopeId ?? null, options.sourceThreadId ?? null, options.lastTurnId ?? null]);
      let registration = pendingCreations.get(key);
      if (!registration) {
        const onCreated = thread => {
          validateConversationId(thread?.id);
          pendingCreations.set(key, { threadId: thread.id, title: "新对话",
            forkedFromId: null, lastTurnId: null, scopeId: options.scopeId ?? null });
        };
        const result = options.sourceThreadId ? await adapter().forkThread(options.sourceThreadId, options.lastTurnId)
          : await adapter().createThread({ onCreated });
        validateConversationId(result?.thread?.id);
        registration = { threadId: result.thread.id, title: options.sourceThreadId ? "分支对话" : "新对话",
          forkedFromId: options.sourceThreadId ?? null, lastTurnId: options.lastTurnId ?? null, scopeId: options.scopeId ?? null };
        pendingCreations.set(key, registration);
      }
      if (!options.sourceThreadId) {
        const saved = await adapter().saveThreadName(registration.threadId, registration.title, { requireHistory: true });
        if (saved?.thread?.id !== registration.threadId || saved.thread.name !== registration.title) throw Error("新对话名称保存回执不匹配");
      }
      const record = await runtimeRequest(collection(projectId), { method: "POST", body: JSON.stringify(registration) });
      if (record?.id !== registration.threadId || record.projectId !== projectId) throw Error("新对话尚未登记，请重试；不会重复创建");
      pendingCreations.delete(key);
      return { thread: conversationSummary(record) };
    });
  });
  ipcMain.handle("agent:list-threads", async (_event, projectId, archived = false, cursor = null, scopeId) => {
    if (typeof archived !== "boolean" || cursor !== null) throw Error("对话列表参数无效");
    if (scopeId !== undefined) validateConversationId(scopeId);
    const result = await runtimeRequest(`${collection(projectId)}?archived=${archived}${scopeId ? `&scopeId=${encodeURIComponent(scopeId)}` : ""}`);
    if (!Array.isArray(result.conversations) || result.conversations.some(record => record.projectId !== projectId)) throw Error("对话列表作品归属无效");
    return { threads: result.conversations.map(conversationSummary), nextCursor: null };
  });
  ipcMain.handle("agent:read-conversation", async (_event, projectId, threadId) => {
    const record = await readConversation(runtimeRequest, projectId, threadId);
    const detail = await readHistory(projectId, threadId);
    if (detail.thread.id !== threadId) throw Error("对话读取回执不匹配");
    return { ...detail, thread: { ...detail.thread, ...conversationSummary(record) } };
  });
  ipcMain.handle("agent:rename-thread", async (_event, projectId, threadId, title) => {
    if (typeof title !== "string" || !title.trim() || title !== title.trim() || [...title].length > 120 || /[\p{Cc}]/u.test(title)) throw Error("对话名称需为 1–120 字符");
    return patch(projectId, threadId, { title });
  });
  ipcMain.handle("agent:set-thread-pinned", (_event, projectId, threadId, isPinned) => {
    if (typeof isPinned !== "boolean") throw Error("置顶参数无效");
    return patch(projectId, threadId, { isPinned });
  });
  ipcMain.handle("agent:archive-thread", async (_event, projectId, threadId) => ({ ...await patch(projectId, threadId, { isArchived: true }), archived: true }));
  ipcMain.handle("agent:unarchive-thread", (_event, projectId, threadId) => patch(projectId, threadId, { isArchived: false }));
}
