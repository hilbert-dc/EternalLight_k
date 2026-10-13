// @ts-check
// Disposable presentation data; never used to authorize or resume execution.
export class ExecutionEventJournal {
  constructor({ compact = false } = {}) {
    this.compact = compact;
    this.cursor = 0;
    this.discarded = 0;
    this.entries = [];
    this.bytes = 0;
  }
  startTurn() { this.entries=[]; this.bytes=0; this.discarded=0; }
  append(event) {
    const sequence = event.hostExecution.sequence;
    if (!Number.isSafeInteger(sequence) || sequence <= this.cursor) return;
    if (sequence !== this.cursor + 1) this.discarded = sequence - 1;
    this.cursor = sequence;
    let value = structuredClone(event);
    const prior = this.entries.at(-1);
    if (this.compact && prior && value.method === 'item/agentMessage/delta'
      && prior.value.method === value.method && prior.value.params?.itemId === value.params?.itemId
      && prior.value.params?.turnId === value.params?.turnId && prior.value.params?.threadId === value.params?.threadId
      && typeof value.params?.delta === 'string' && typeof prior.value.params?.delta === 'string') {
      this.entries.pop(); this.bytes -= prior.bytes;
      value.params.delta = prior.value.params.delta + value.params.delta;
    }
    const bytes = new TextEncoder().encode(JSON.stringify(value)).length;
    this.entries.push({ value, bytes }); this.bytes += bytes;
    while (this.entries.length > 512 || this.bytes > 2 * 1024 * 1024) {
      const removed = this.entries.shift(); this.bytes -= removed.bytes;
      this.discarded = Math.max(this.discarded, removed.value.hostExecution.sequence);
    }
  }
  snapshot() {
    return {cursor:this.cursor, events:this.entries.map(entry => structuredClone(entry.value)), truncated:this.discarded > 0};
  }
  after(cursor) {
    if (cursor < this.discarded) throw Error('读取期间进度过多或事件不连续，请重新打开此对话核对运行状态。');
    return this.entries.filter(entry => entry.value.hostExecution.sequence > cursor).map(entry => structuredClone(entry.value));
  }
}
