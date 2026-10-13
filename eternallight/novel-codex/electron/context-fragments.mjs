// @ts-check
// Core's AdditionalContext{User,Developer}Fragment truncates each value at
// 1,000 approximate tokens (4,000 UTF-8 bytes). Respect that transport boundary
// without deleting content or changing its trust level. Whole-turn admission
// remains the responsibility of assertNovelTurnBudget.
const FRAGMENT_BYTES = 3_500;

/** @param {Record<string, {kind: string, value: string}>} context */
export function boundedContextFragments(context) {
  const result = {};
  for (const [key, entry] of Object.entries(context)) {
    if (!/^[a-zA-Z0-9_-]{1,60}$/.test(key) || !entry || !["application", "untrusted"].includes(entry.kind)
      || typeof entry.value !== "string" || Object.keys(entry).length !== 2) throw new Error("Invalid application context fragment");
    const parts = [];
    let part = "", bytes = 0;
    for (const character of entry.value) {
      const size = Buffer.byteLength(character, "utf8");
      if (bytes + size > FRAGMENT_BYTES) { parts.push(part); part = ""; bytes = 0; }
      part += character; bytes += size;
    }
    parts.push(part);
    for (let index = 0; index < parts.length; index++) {
      const name = parts.length === 1 ? key : `${key}_part_${String(index + 1).padStart(4, "0")}`;
      if (Object.hasOwn(result, name) || name !== key && Object.hasOwn(context, name)) throw new Error("Context fragment key collision");
      result[name] = { kind: entry.kind, value: parts[index] };
    }
  }
  return result;
}
