import { nativeBinding, nativeCall } from "../native.mjs";

export const CONFORMANCE_FAILURE_CATEGORIES = Object.freeze([
  "tool-not-exposed",
  "tool-not-selected",
  "tool-call-as-text",
  "conversion-error",
  "execution-error",
  "identity-rejected",
  "argument-rejected",
  "transport-result-missing",
  "result-consumption-failed",
  "lifecycle-error",
  "budget-or-timeout",
  "inconclusive",
]);

export class BoundedExecutionTrace {
  #native;
  #clock;
  constructor({ clock = Date.now, ...options } = {}) {
    this.#native = new (nativeBinding().NativeExecutionTrace)(JSON.stringify(options));
    this.#clock = clock;
  }
  record(source, type, details = {}) {
    const result = JSON.parse(this.#native.record(JSON.stringify({ source, type, details, now: this.#clock() })));
    return result === null ? null : Object.freeze({ ...result, details: Object.freeze(result.details) });
  }
  snapshot() {
    const result = JSON.parse(this.#native.snapshot());
    return Object.freeze({ ...result, events: Object.freeze(result.events.map(event => Object.freeze({ ...event, details: Object.freeze(event.details) }))) });
  }
}

export const classifyConformanceFailure = (trace, options = {}) => nativeCall("trace.classify", { trace, options });
