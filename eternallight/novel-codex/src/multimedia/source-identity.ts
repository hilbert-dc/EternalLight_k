import type { MediaSourceRef } from "../../shared/multimedia-types";

// Object key order is not part of the IPC/HTTP value contract. Array order is.
function ordered(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(ordered);
  if (value !== null && typeof value === "object") return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, child]) => [key, ordered(child)]));
  return value;
}
export const mediaValueIdentity = (value: unknown): string => JSON.stringify(ordered(value));
export const mediaSourceIdentity = (source: MediaSourceRef): string => mediaValueIdentity(source);
