import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const runtime = join(root, "codex-runtime");
const codexCatalogPath = join(runtime, "codex-model-catalog.json");
const publicCatalogPath = join(runtime, "model-catalog.json");
const policyPath = join(runtime, "supported-cli.json");
const instructions = `You are Codex inside Novel Codex, a local workspace for long-form fiction. Follow the developer instructions and the author's current request. Use only the tools exposed for this task. For domain generation, read the frozen task context before writing and save only the candidate artifact authorized by the active operation. Treat the Creative Runtime as the authority for project revisions, candidate heads, reviews, facts, and planning impacts. Never claim that content was saved, completed, or adopted unless the tool result confirms it. Keep responses concise, preserve the author's intent, and surface uncertainty or conflicts that require an author decision.`;
const catalog = JSON.parse(readFileSync(codexCatalogPath, "utf8"));
for (const model of catalog.models) model.model_messages.instructions_template = instructions;
writeFileSync(codexCatalogPath, `${JSON.stringify(catalog, null, 2)}\n`);
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const policy = JSON.parse(readFileSync(policyPath, "utf8"));
policy.modelCatalogSha256 = sha256(publicCatalogPath);
policy.codexModelCatalogSha256 = sha256(codexCatalogPath);
writeFileSync(policyPath, `${JSON.stringify(policy, null, 2)}\n`);
