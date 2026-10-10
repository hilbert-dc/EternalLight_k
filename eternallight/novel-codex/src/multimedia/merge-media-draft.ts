import type { MultimediaDocument } from "../../shared/multimedia-types";

type Value = null | boolean | number | string | Value[] | { [key: string]: Value };
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const object = (value: unknown): value is Record<string, Value> => value !== null && typeof value === "object" && !Array.isArray(value);
const conflict = Symbol("conflict");
type Result = Value | undefined | typeof conflict;
const ids = (values: Value[]) => values.every(value => object(value) && typeof value.id === "string" && value.id.length > 0)
  && new Set(values.map(value => (value as Record<string, Value>).id)).size === values.length
  ? values.map(value => (value as Record<string, Value>).id as string) : null;

function merge(base: Value | undefined, local: Value | undefined, remote: Value | undefined): Result {
  if (equal(local, remote)) return local;
  if (equal(base, local)) return remote;
  if (equal(base, remote)) return local;
  if (Array.isArray(base) && Array.isArray(local) && Array.isArray(remote)) {
    const b = ids(base), l = ids(local), r = ids(remote);
    if (!b || !l || !r) return conflict;
    // Reordering, insertion and deletion may be taken from one side only.
    // Concurrent changes to the sequence need an author's decision.
    if (!equal(b, l) && !equal(b, r)) return conflict;
    const order = equal(b, l) ? r : l;
    const maps = [base, local, remote].map(values => new Map(values.map(value => [(value as Record<string, Value>).id, value])));
    const merged = new Map<string, Value>();
    for (const id of new Set([...b, ...l, ...r])) {
      const result = merge(maps[0].get(id), maps[1].get(id), maps[2].get(id));
      if (result === conflict) return conflict;
      if (result !== undefined) merged.set(id, result);
    }
    return order.map(id => merged.get(id)!);
  }
  if (object(base) && object(local) && object(remote)) {
    const result: Record<string, Value> = {};
    for (const key of new Set([...Object.keys(base), ...Object.keys(local), ...Object.keys(remote)])) {
      const value = merge(base[key], local[key], remote[key]);
      if (value === conflict) return conflict;
      if (value !== undefined) Object.defineProperty(result, key, { value, enumerable: true, writable: true, configurable: true });
    }
    return result;
  }
  return conflict;
}

// This produces an unsaved draft, never a save receipt or permission grant.
export function mergeMediaDraft(base: MultimediaDocument, local: MultimediaDocument, remote: MultimediaDocument): MultimediaDocument | null {
  if (base.kind !== local.kind || base.kind !== remote.kind) return null;
  const result = merge(base as unknown as Value, local as unknown as Value, remote as unknown as Value);
  return result === conflict || result === undefined ? null : structuredClone(result) as unknown as MultimediaDocument;
}
