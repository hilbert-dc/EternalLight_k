// @ts-check
import { mediaMessageInput } from "./media-conversation.mjs";
import { randomUUID } from "node:crypto";
import { validateConversationId, readConversation } from "./conversation-handlers.mjs";
import { validateAuthorIntent } from "../shared/author-input-budget.mjs";
import { validateConversationFocus } from "./conversation-analysis.mjs";
const collection = (project, thread) => `/api/projects/${encodeURIComponent(validateConversationId(project))}/conversations/${encodeURIComponent(validateConversationId(thread))}/messages`;
const post = (request, path, body) => request(path, { method: "POST", body: JSON.stringify(body) });
const currentStates = new Set(["queued", "claimed", "sending", "delivered", "failed", "cancelled"]);
export class ConversationMessages {
  #lastConversation = new Map();
  #active = new Map();
  #stopped = false;
  pause() { this.#stopped=true; }
  async settled() { await Promise.allSettled([...this.#active.values()]); }
  start() { if(this.#active.size)throw Error('原消息尚未结束');this.#stopped=false; }
  get activeCount() { return this.#active.size; }
  constructor({ request, execute, available, maxActive = 4, changed = (_message) => {}, started = (_message) => {}, ended = (_message, _result, _error) => {}, sessionId = randomUUID(), connectionId = () => null, bindTrigger = async (_project, _task, _trigger) => {} }) {
    if(!Number.isSafeInteger(maxActive) || maxActive<1 || maxActive>16)throw Error('消息执行容量无效');
    this.request = request;
    this.execute = execute;
    this.available = available;
    this.maxActive = maxActive;
    this.changed = changed;
    this.started = started;
    this.ended = ended;
    this.sessionId = sessionId;
    this.connectionId = connectionId;
    this.bindTrigger = bindTrigger;
  }
  validate(message, project, thread, id = message?.id) {
    if (message?.projectId !== project || message.threadId !== thread || message.id !== id
      || !Number.isSafeInteger(message.revision) || message.revision < 0 || !currentStates.has(message.status)) throw Error("消息回执归属不匹配");
    validateConversationId(message.id);
    return message;
  }
  async list(project, thread) {
    const result = await this.request(collection(project, thread));
    if (!Array.isArray(result?.messages)) throw Error("消息列表回执无效");
    return result.messages.map(item => ({ ...this.validate(item, project, thread), requiresContinue: item.trigger?.kind !== "steer" && (item.requiresContinue === true || item.sessionId !== this.sessionId) }));
  }
  async read(project, thread, id) {
    validateConversationId(id);
    return this.validate(await this.request(`${collection(project, thread)}/${encodeURIComponent(id)}`), project, thread, id);
  }
  async submit(project, thread, message) {
    if (!message || Object.keys(message).some(key => !["messageId", "input", "trigger"].includes(key))) throw Error("消息参数无效");
    validateConversationId(message.messageId);
    const media=mediaMessageInput(message.input?.media);
    validateAuthorIntent(message.input?.text,{imageAssetRevisionIds:media?.imageAssetRevisionIds});
    const focus = validateConversationFocus(message.input?.focus ?? null);
    if (media && (media.scope.projectId!==project || focus)) throw Error("媒介消息范围不匹配");
    if (focus?.excerpts?.some(x => x.projectId !== project)) throw Error("引用不属于当前作品");
    await readConversation(this.request, project, thread, true);
    const input = { text: message.input.text, focus, connectionId: message.input.connectionId ?? this.connectionId(), ...(media?{media}:{}) };
    await this.bindTrigger(project, thread, message.trigger);
    const result = this.validate(await post(this.request, collection(project, thread), {
      messageId: message.messageId, input, trigger: message.trigger, sessionId: this.sessionId,
    }), project, thread, message.messageId);
    this.changed(result);
    return result;
  }
  async change(message, action, data = {}) {
    const body = { action, expectedRevision: message.revision, ...data,
      ...(["continue", "claim", "sending", "delivered", "fail"].includes(action) ? {sessionId: this.sessionId} : {}) };
    const result = this.validate(await post(this.request, `${collection(message.projectId, message.threadId)}/${encodeURIComponent(message.id)}`, body), message.projectId, message.threadId, message.id);
    this.changed(result);
    return result;
  }
  async dispatch(message, event, onStarted = () => {}) {
    if (this.#stopped || message.status !== "queued" || message.triggerReady === false || message.requiresContinue === true || message.sessionId !== this.sessionId || !await this.available(message)) return false;
    let claimed;
    try { claimed = await this.change(message, "claim"); }
    catch (error) {
      // Native can hold a valid queue item for its original condition or another turn.
      if (["message_held", "message_stale"].includes(error?.code)) return false;
      throw error;
    }
    const delivery = { message: claimed };
    let failure = null, result = null;
    try {
      if(this.#stopped)throw Error('应用正在关闭，已领取消息保留待核对；没有发送模型。');
      if(!await this.available(claimed) || this.#stopped)throw Error('领取后执行条件已变化，消息保留待核对；没有发送模型。');
      this.started(claimed);onStarted();
      result = await this.execute(event, delivery);
    }
    catch (error) {
      failure = error?.message ?? "执行未完成";
      if (delivery.message.status !== "delivered") {
        try { delivery.message = await this.change(delivery.message, "fail", { error: error?.message ?? "执行结果待核对" }); }
        catch { /* Leave the original claim visible; never implicitly retry. */ }
      }
      throw error;
    } finally { this.ended(delivery.message, failure, result); }
    return true;
  }
  async bind(delivery, task) {
    if (!delivery || delivery.message.taskId) return;
    const prior = delivery.message;
    const bound = this.validate(await this.request(`${collection(prior.projectId, prior.threadId)}/${encodeURIComponent(prior.id)}`), prior.projectId, prior.threadId, prior.id);
    if (bound.taskId !== task.id || bound.status !== "claimed" || bound.sessionId !== this.sessionId) throw Error("消息未绑定到原执行");
    delivery.message = bound;
  }
  async sending(delivery) {
    if (!delivery || delivery.message.status !== "claimed") return;
    delivery.message = await this.change(delivery.message, "sending");
  }
  async delivered(delivery, turnId) {
    if (!delivery || delivery.message.status !== "sending") return;
    delivery.message = await this.change(delivery.message, "delivered", { turnId });
  }
  async #launch(message,event) {
    const key=JSON.stringify([message.projectId,message.threadId]);
    if(this.#stopped || this.#active.has(key) || this.#active.size>=this.maxActive)return false;
    let resolveReady,rejectReady,started=false;
    const ready=new Promise((resolve,reject)=>{resolveReady=resolve;rejectReady=reject;});
    const completion=Promise.resolve().then(()=>this.dispatch(message,event,()=>{started=true;resolveReady(true);}))
      .then(()=>resolveReady(false),error=>{
        // After dispatch, Native's original receipt and ended notification retain
        // the error. Before dispatch, the scanner records its ordinary health error.
        if(!started)rejectReady(error);
      }).finally(()=>this.#active.delete(key));
    this.#active.set(key,completion);
    return ready;
  }
  // Called by the existing Native event consumer, before internal continuations.
  async drain(project, event) {
    if(this.#stopped || this.#active.size>=this.maxActive)return false;
    const { conversations } = await this.request(`/api/projects/${encodeURIComponent(project)}/conversations?archived=false`);
    if (!Array.isArray(conversations)) throw Error("对话目录回执无效");
    const last=conversations.findIndex(item=>item.id===this.#lastConversation.get(project));
    for (let offset=1;offset<=conversations.length;offset++) {
      const conversation=conversations[(last+offset)%conversations.length];
      const messages = await this.list(project, conversation.id);
      const first = messages.find(message => message.status === "queued" && message.trigger.kind === "interlude" && message.triggerReady) ?? messages.find(message => message.status === "queued");
      if (first && await this.#launch(first, event)) {
        this.#lastConversation.set(project,conversation.id);
        return true;
      }
    }
    return false;
  }
}
export function registerConversationMessageHandlers(ipcMain, transport) {
  ipcMain.handle("agent:read-message", (_event, project, thread, id) => transport.read(project, thread, id));
  ipcMain.handle("agent:list-messages", (_event, project, thread) => transport.list(project, thread));
  ipcMain.handle("agent:submit-message", (_event, project, thread, message) => transport.submit(project, thread, message));
  ipcMain.handle("agent:change-message", async (event, project, thread, id, change) => {
    if (!change || !["edit", "cancel", "move", "continue", "reconcile"].includes(change.action)) throw Error("消息操作无效");
    const message = (await transport.list(project, thread)).find(item => item.id === id);
    if (!message || change.expectedRevision !== message.revision) throw Error("消息已变化，请刷新后重试");
    const data = change.action === "edit" ? { input: { ...change.input, focus: validateConversationFocus(change.input?.focus ?? null) } }
      : change.action === "move" ? { direction: change.direction } : {};
    if (change.action === "edit") validateAuthorIntent(change.input?.text,{imageAssetRevisionIds:message.input.media?.imageAssetRevisionIds});
    const result = await transport.change(message, change.action, data);
    return result;
  });
}
