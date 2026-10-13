// @ts-check
const identifier = value => typeof value === "string" && value.trim() === value && value.length > 0
  && Buffer.byteLength(value, "utf8") <= 160 && !/[\p{Cc}]/u.test(value);
const exact = (value, keys) => value !== null && typeof value === "object" && !Array.isArray(value)
  && Reflect.ownKeys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const integer = value => Number.isSafeInteger(value) && value >= 0;
const cursor = value => value === null || (typeof value === "string" && value.length > 0
  && Buffer.byteLength(value, "utf8") <= 1024 && !/[\p{Cc}]/u.test(value));

export const CHAPTER_STRUCTURE_ANALYSIS_PROMPT = "检查这次章节删除对人物、伏笔和故事衔接的影响，只给建议，由作者决定是否修改。";

export function validateChapterStructureContext(input) {
  if (input === null) return null;
  if (!exact(input, ["projectId", "dirty"]) || !identifier(input.projectId) || typeof input.dirty !== "boolean") {
    throw new Error("章节结构检查上下文无效");
  }
  return Object.freeze({ ...input });
}

export function validateChapterStructureChange(change, projectId, changeId = change?.id) {
  const neighbor = value => value === null || (exact(value, ["id", "number"]) && identifier(value.id) && integer(value.number) && value.number > 0);
  const sourceKeys = ["objectId", "savedRevisionId", "acceptedRevisionId", "workingRevisionId", "knowledgeBatchId"];
  const assessment = change?.assessment;
  if (!identifier(projectId) || !identifier(changeId)
    || !exact(change, ["id", "projectId", "kind", "chapterId", "chapterNumber", "title", "createdAt", "baseRevision", "resultRevision", "baseSnapshotId", "resultSnapshotId", "source", "previousChapter", "nextChapter", "assessment"])
    || change.id !== changeId || change.projectId !== projectId || change.kind !== "chapterDeleted"
    || !identifier(change.chapterId) || !integer(change.chapterNumber) || change.chapterNumber < 1 || typeof change.title !== "string"
    || !integer(change.createdAt) || !integer(change.baseRevision) || change.resultRevision !== change.baseRevision + 1
    || !integer(change.resultRevision) || !identifier(change.baseSnapshotId) || !identifier(change.resultSnapshotId)
    || !exact(change.source, sourceKeys) || change.source.objectId !== `draft-${change.chapterId}`
    || sourceKeys.slice(1).some(key => change.source[key] !== null && !identifier(change.source[key]))
    || !neighbor(change.previousChapter) || !neighbor(change.nextChapter)
    || !exact(assessment, ["state", "skipReason", "taskId", "workTaskId"])
    || !["pending", "skipped", "bound"].includes(assessment.state)
    || (assessment.state === "bound" ? !identifier(assessment.taskId) || !identifier(assessment.workTaskId) || assessment.skipReason !== null
      : assessment.taskId !== null || assessment.workTaskId !== null
        || (assessment.state === "skipped" ? assessment.skipReason !== "noSavedEvidence" : assessment.skipReason !== null))) {
    throw new Error("章节结构检查记录与作品或删除来源不一致");
  }
  return change;
}

export function registerChapterStructureReadHandlers(ipc, request) {
  ipc.handle("runtime:list-chapter-structure-changes", (_event, projectId, input) => {
    if (!identifier(projectId) || !exact(input, ["cursor", "limit", "assessmentState"]) || !cursor(input.cursor)
      || ![null, "pending"].includes(input.assessmentState)
      || !Number.isSafeInteger(input.limit) || input.limit < 1 || input.limit > 20) throw new Error("章节结构检查读取请求无效");
    const query = new URLSearchParams({ limit: String(input.limit), ...(input.cursor === null ? {} : { cursor: input.cursor }),
      ...(input.assessmentState === null ? {} : { assessmentState: input.assessmentState }) });
    return request(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes?${query}`);
  });
  ipc.handle("runtime:read-chapter-structure-change", (_event, projectId, id) => {
    if (!identifier(projectId) || !identifier(id)) throw new Error("章节结构检查读取请求无效");
    return request(`/api/projects/${encodeURIComponent(projectId)}/chapter-structure-changes/${encodeURIComponent(id)}`);
  });
}

// The durable event authorizes a single initial check. Runtime owns its unique
// task binding; this process also holds unknown attempts until explicit retry.
export class ChapterStructureAnalysisConsumer {
  #context = null;
  #epoch = 0;
  #pending = false;
  #cursor = null;
  #nextScanAt = 0;
  #attempted = new Set();
  #timer = null;
  constructor({ available, list, read, run, updated, failed }) {
    this.available = available;
    this.list = list;
    this.read = read;
    this.run = run;
    this.updated = updated;
    this.failed = failed;
  }
  get busy() { return this.#pending; }
  select(context) {
    if (context) validateChapterStructureContext({ projectId: context.projectId, dirty: context.dirty });
    this.#context = context; this.#epoch += 1; this.#cursor = null; this.#nextScanAt = 0;
  }
  start() {
    if (this.#timer) return;
    this.#timer = setInterval(() => void this.tick(), 1000); this.#timer.unref?.();
  }
  stop() { clearInterval(this.#timer); this.#timer = null; this.select(null); }
  #current(context, epoch) { return this.#epoch === epoch && this.#context === context && !context.dirty; }
  #ready(context, epoch) { return this.#current(context, epoch) && this.available(context); }
  async #notify(callback, ...args) {
    // A presentation callback cannot change a completed/unknown execution into
    // another attempt or mask the actual run failure.
    try { await callback?.(...args); } catch { /* Observer failure is not task retry authority. */ }
  }
  async #consume(context, epoch, item) {
    let change = validateChapterStructureChange(await this.read(context.projectId, item.id), context.projectId, item.id);
    if (!this.#ready(context, epoch)) return;
    if (change.assessment.state !== "pending") {
      await this.#notify(this.updated, context, change, null); return;
    }
    const key = `${context.projectId}\0${change.id}`;
    if (this.#attempted.has(key)) return;
    this.#attempted.add(key);
    let result;
    try {
      result = await this.run(context, CHAPTER_STRUCTURE_ANALYSIS_PROMPT,
        { operation: "assessChapterStructure" }, { type: "project", structureChangeId: change.id },
        () => this.#current(context, epoch));
    } catch (error) {
      // A POST or model result can be lost after the unique task was bound.
      // Read only; even a still-pending/unknown event never retries on a tick.
      try { change = validateChapterStructureChange(await this.read(context.projectId, change.id), context.projectId, change.id); } catch { /* Preserve the original failure. */ }
      if (this.#current(context, epoch)) {
        await this.#notify(this.updated, context, change, null);
      }
      // Execution settlement belongs to the original Work even if navigation
      // has changed the presentation scope; it never starts another model.
      await this.#notify(this.failed, context, change, error);
      return;
    }
    if (this.#current(context, epoch)) await this.#notify(this.updated, context, change, result);
  }
  async tick() {
    const context = this.#context; const epoch = this.#epoch;
    if (this.#pending || !context || !this.#ready(context, epoch) || Date.now() < this.#nextScanAt) return;
    this.#pending = true;
    try {
      // Preserve the cursor between bounded ticks so old bound events cannot
      // hide later pending ones, without rereading a whole large history.
      for (let page = 0; page < 4; page += 1) {
        const response = await this.list(context.projectId, { cursor: this.#cursor, limit: 20, assessmentState: "pending" });
        if (!this.#ready(context, epoch)) return;
        if (!exact(response, ["projectId", "items", "nextCursor"]) || response.projectId !== context.projectId
          || !Array.isArray(response.items) || response.items.length > 20 || !cursor(response.nextCursor)
          || (response.nextCursor !== null && response.nextCursor === this.#cursor)
          || new Set(response.items.map(item => item?.id)).size !== response.items.length) throw new Error("章节结构检查列表无效");
        const items = response.items.map(item => validateChapterStructureChange(item, context.projectId));
        const pending = items.find(item => item.assessment.state === "pending" && !this.#attempted.has(`${context.projectId}\0${item.id}`));
        if (pending) { await this.#consume(context, epoch, pending); return; }
        this.#cursor = response.nextCursor;
        if (this.#cursor === null) { this.#nextScanAt = Date.now() + 10_000; return; }
      }
    } catch (error) {
      if (this.#current(context, epoch)) {
        this.#nextScanAt = Date.now() + 10_000;
        await this.#notify(this.failed, context, null, error);
      }
    } finally { this.#pending = false; }
  }
  async retry(projectId, id) {
    const context = this.#context; const epoch = this.#epoch;
    if (!identifier(projectId) || !identifier(id) || !context || context.projectId !== projectId
      || this.#pending || !this.#ready(context, epoch)) throw new Error("当前不能启动章节结构检查");
    this.#pending = true;
    try {
      const change = validateChapterStructureChange(await this.read(projectId, id), projectId, id);
      if (!this.#ready(context, epoch)) return;
      if (change.assessment.state !== "pending") throw new Error("检查已绑定原任务或无需执行，请读取记录并从原任务恢复");
      this.#attempted.delete(`${projectId}\0${id}`);
      await this.#consume(context, epoch, change);
    } finally { this.#pending = false; }
  }
}
