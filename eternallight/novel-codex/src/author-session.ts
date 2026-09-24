const PROFILE_KEY = "novel-codex:author";
const SESSION_KEY = "novel-codex:signed-in";

export type AuthorProfile = { name: string; email: string; passwordSalt: string; passwordHash: string };

export function loadAuthor(): AuthorProfile | null {
  try {
    const parsed = JSON.parse(localStorage.getItem(PROFILE_KEY) ?? "") as Partial<AuthorProfile>;
    if (!parsed.name?.trim() || !parsed.email?.trim() || !parsed.passwordSalt || !parsed.passwordHash) return null;
    return { name: parsed.name.trim(), email: parsed.email.trim(), passwordSalt: parsed.passwordSalt, passwordHash: parsed.passwordHash };
  } catch {
    return null;
  }
}

export function saveAuthor(profile: AuthorProfile) {
  localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
  localStorage.setItem(SESSION_KEY, "1");
}

export function markSignedOut() {
  localStorage.removeItem(SESSION_KEY);
}

export function clearAuthor() {
  localStorage.removeItem(PROFILE_KEY);
  localStorage.removeItem(SESSION_KEY);
}

export function hasActiveSession() {
  return localStorage.getItem(SESSION_KEY) === "1" && loadAuthor() !== null;
}

export async function hashPassword(password: string, salt: string) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${salt}:${password}`));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function authorInitial(name: string) {
  const trimmed = name.trim();
  return trimmed ? trimmed.slice(0, 1).toUpperCase() : "作";
}
