export type AuthorProfile = { name: string; email: string; passwordSalt: string; passwordHash: string };
export type AuthorSession = { profile: AuthorProfile | null; signedIn: boolean };
export type AuthorSessionApi = {
  read: () => Promise<AuthorSession>;
  save: (profile: AuthorProfile) => Promise<AuthorSession>;
  signOut: () => Promise<AuthorSession>;
  clear: () => Promise<AuthorSession>;
};

declare global { interface Window { novelAuthorSession?: AuthorSessionApi } }

export async function hashPassword(password: string, salt: string) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`${salt}:${password}`));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function authorInitial(name: string) {
  const trimmed = name.trim();
  return trimmed ? trimmed.slice(0, 1).toUpperCase() : "作";
}
