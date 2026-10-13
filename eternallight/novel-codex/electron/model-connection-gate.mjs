// @ts-check
export class ModelConnectionGate {
  #busy = false;
  constructor(executing) { this.executing = executing; }
  get busy() { return this.#busy; }
  assertExecutionAllowed() {
    if (this.#busy) throw new Error("模型连接正在验证或切换，请完成后再执行任务");
  }
  async run(operation) {
    this.assertExecutionAllowed();
    if (this.executing()) throw new Error("已有创作任务正在执行，请结束后再验证或切换模型");
    this.#busy = true;
    try { return await operation(); }
    finally { this.#busy = false; }
  }
}
