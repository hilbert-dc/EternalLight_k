import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

export const nativeEnginePath = fileURLToPath(new URL(`../native/bin/${process.platform}-${process.arch}/engine.node`, import.meta.url)).replace(/([\\/])app\.asar([\\/])/, "$1app.asar.unpacked$2");
let binding;
export function nativeBinding() { return binding ??= createRequire(import.meta.url)(nativeEnginePath); }
export class UnsupportedProtocolStateError extends Error {
  constructor(message) { super(message); this.name = "UnsupportedProtocolStateError"; this.code = "unsupported_protocol_state"; }
}
export function normalizeNativeError(error) {
  return error.message.startsWith("unsupported_protocol_state:") ? new UnsupportedProtocolStateError(error.message.slice("unsupported_protocol_state:".length)) : error;
}
export function nativeCall(method, input) {
  nativeBinding();
  try { return JSON.parse(binding.invoke(method, JSON.stringify(input ?? null))); }
  catch (error) {
    throw normalizeNativeError(error);
  }
}
