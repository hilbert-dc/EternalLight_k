// Runtime owns due times, durable steps, budgets and claims. This only wakes one
// ordinary workflow while the selected trusted editor and engine are idle.
export class ModelingConsumer {
  #context = null;
  #epoch = 0;
  #pending = false;
  #timer = null;
  constructor({ available, dispatch, run, failed, updated }) {
    Object.assign(this, { available, dispatch, run, failed, updated });
  }
  select(context) { this.#context = context; this.#epoch += 1; }
  start() {
    if (this.#timer) return;
    this.#timer = setInterval(() => void this.tick(), 1000);
    this.#timer.unref?.();
  }
  stop() { clearInterval(this.#timer); this.#timer = null; this.select(null); }
  async tick() {
    const context = this.#context;
    const epoch = this.#epoch;
    if (this.#pending || !context || context.dirty || !this.available(context)) return;
    this.#pending = true;
    let action;
    try {
      action = (await this.dispatch(context.projectId))?.action;
      if (!action || epoch !== this.#epoch || !this.available(context) || this.#context?.dirty) return;
      await this.run(context, action.workTaskId);
    } catch (error) {
      // No automatic retry, including failures before a model Attempt exists.
      if (action) await this.failed(context, action.workTaskId, error).catch(() => {});
      if (epoch === this.#epoch) this.select(null);
    } finally {
      if (action) this.updated(context);
      this.#pending = false;
    }
  }
}
