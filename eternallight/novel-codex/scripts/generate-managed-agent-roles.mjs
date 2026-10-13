import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { NOVEL_AGENT_ROLES, renderNovelAgentRole, renderNovelRoleCatalog } from "../electron/novel-agent-roles.mjs";

// One generator for the installed profiles and Native's discovery resource.
// The JSON is a build artifact, never another editable permission authority.
const check = process.argv.slice(2).includes("--check");
if (process.argv.slice(2).some(argument => argument !== "--check")) throw new Error("Use only --check or no arguments");
const artifacts = [
  ...NOVEL_AGENT_ROLES.map(role => [new URL(`../codex-runtime/agents/${role.id}.toml`, import.meta.url), renderNovelAgentRole(role)]),
  [new URL("../shared/novel-agent-roles.json", import.meta.url), renderNovelRoleCatalog()],
];
for (const [url, content] of artifacts) {
  if (check) {
    if (readFileSync(url, "utf8") !== content) throw new Error(`Managed role artifact differs from the single source: ${fileURLToPath(url)}`);
  } else {
    mkdirSync(new URL(".", url), { recursive: true });
    writeFileSync(url, content, { mode: 0o644 });
  }
}
