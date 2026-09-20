import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export const DEEPSEEK_KEYCHAIN_ACCOUNT = "novel-codex";
export const DEEPSEEK_KEYCHAIN_SERVICE = "com.eternallight.novelcodex.deepseek";

const PROVIDER_CREDENTIAL_KEYS = [
  "CODEX_ACCESS_TOKEN",
  "CODEX_API_KEY",
  "DEEPSEEK_API_KEY",
  "OPENAI_ACCESS_TOKEN",
  "OPENAI_API_KEY",
];

export function loadProjectDevelopmentEnvironment(projectRoot, environment) {
  const resolved = { ...environment };
  if (typeof resolved.DEEPSEEK_API_KEY === "string" && resolved.DEEPSEEK_API_KEY.trim()) return resolved;
  const path = join(projectRoot, ".env.local");
  if (!existsSync(path)) return resolved;
  const contents = readFileSync(path, "utf8");
  if (Buffer.byteLength(contents, "utf8") > 16_384) throw new Error(".env.local 超过开发凭据大小限制");
  for (const line of contents.split(/\r?\n/)) {
    const match = /^DEEPSEEK_API_KEY=([^\s#]+)$/.exec(line.trim());
    if (match?.[1]) resolved.DEEPSEEK_API_KEY = match[1];
  }
  return resolved;
}

function readMacKeychainCredential() {
  try {
    return execFileSync(
      "/usr/bin/security",
      [
        "find-generic-password",
        "-a",
        DEEPSEEK_KEYCHAIN_ACCOUNT,
        "-s",
        DEEPSEEK_KEYCHAIN_SERVICE,
        "-w",
      ],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    ).trim();
  } catch {
    return "";
  }
}

export function resolveDevelopmentEnvironment(environment, {
  platform = process.platform,
  readCredential = readMacKeychainCredential,
} = {}) {
  const resolved = { ...environment };
  if (typeof resolved.DEEPSEEK_API_KEY === "string" && resolved.DEEPSEEK_API_KEY.trim()) return resolved;
  if (platform !== "darwin") return resolved;

  const credential = readCredential().trim();
  if (credential) resolved.DEEPSEEK_API_KEY = credential;
  return resolved;
}

export function withoutProviderCredentials(environment) {
  const sanitized = { ...environment };
  for (const key of PROVIDER_CREDENTIAL_KEYS) delete sanitized[key];
  return sanitized;
}
