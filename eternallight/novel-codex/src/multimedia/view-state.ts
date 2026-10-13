export function readMediaView(key: string): Record<string, unknown> {
  try { const value: unknown = JSON.parse(localStorage.getItem(key) ?? "null"); return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {}; } catch { return {}; }
}
export function saveMediaView(key: string, value: object) {
  try { localStorage.setItem(key, JSON.stringify(value)); } catch { /* A view bookmark never affects the saved work. */ }
}
