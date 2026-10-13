import { nativeCall } from "../native.mjs";

/** JS ownership facade; registry identity, authorization and aliases live in Rust. */
export class ToolRegistry {
  constructor({ maxTools = 1024 } = {}) {
    if (!Number.isInteger(maxTools) || maxTools < 1) throw new Error("tool registry limit must be positive");
    this.maxTools = maxTools;
    this.entries = [];
  }
  register(input) {
    const result = nativeCall("tools.register", { ...input, entries: this.entries, maxTools: this.maxTools });
    this.entries = result.entries;
    return result.entry;
  }
  requireWireName(wireName, { executable = true } = {}) { return nativeCall("tools.require", { entries: this.entries, wireName, executable }); }
  wireName(namespace, name) { return nativeCall("tools.name", { entries: this.entries, namespace, name }); }
  snapshot() { return Object.freeze(nativeCall("tools.snapshot", { entries: this.entries }).map(Object.freeze)); }
}
export const TOOL_WIRE_NAME_PATTERN = /^[A-Za-z0-9_-]+$/;
