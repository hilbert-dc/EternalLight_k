import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { NOVEL_AGENT_ROLES, novelRoleDeveloperInstructions, renderNovelAgentRole, renderNovelRoleCatalog } from "../novel-agent-roles.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";

test("Native discovery and installed roles share actual instructions within bounded context", () => {
  const artifact = readFileSync(new URL("../../shared/novel-agent-roles.json", import.meta.url), "utf8");
  assert.equal(artifact, renderNovelRoleCatalog());
  const catalog = JSON.parse(artifact);
  assert.equal(catalog.authority, "roleMethodOnly");
  assert.match(catalog.guidance, /not expertise certificates, a mandatory team, or tool authority/);
  assert.deepEqual(catalog.roles.map(role => role.id), NOVEL_AGENT_ROLES.map(role => role.id));
  for (const [index, role] of NOVEL_AGENT_ROLES.entries()) {
    const entry = catalog.roles[index];
    const profile = renderNovelAgentRole(role);
    const profileInstructions = JSON.parse(profile.match(/^developer_instructions = (.+)$/m)[1]);
    assert.equal(entry.instructions, profileInstructions);
    assert.equal(entry.instructions, novelRoleDeveloperInstructions(role));
    assert.equal(entry.description, role.description);
    assert.equal(entry.topics, role.topics);
    assert.match(entry.topics, /[\p{Script=Han}]/u);
    assert.deepEqual(entry.instructionsRef, { tool: "discover_creative_capabilities", roleId: role.id });
    assert.equal(entry.permissionMode, role.id === "candidate_writer" ? "isolatedCandidateOnlyWithNativeGrant" : "readOnlyAnalysis");
    assert.ok(Buffer.byteLength(JSON.stringify(entry)) <= 4096, `${role.id} discovery page`);
    assert.ok(Buffer.byteLength(profile) <= 4096, `${role.id} profile`);
    assert.ok(modelInputTokenBudget(entry.instructions) <= 1000, `${role.id} developer budget`);
    assert.match(entry.instructions, /Public language contract:/);
    assert.match(entry.instructions, /does not establish that the parent read/);
  }
  assert.throws(() => novelRoleDeveloperInstructions({ id: "candidate_writer" }), /未知创作协作角色/);
});

test("the single generator writes both artifacts and --check rejects profile or discovery drift", () => {
  const root = mkdtempSync(join(tmpdir(), "managed-role-generator-"));
  try {
    mkdirSync(join(root, "scripts"));
    mkdirSync(join(root, "electron"));
    copyFileSync(new URL("../../scripts/generate-managed-agent-roles.mjs", import.meta.url), join(root, "scripts", "generate-managed-agent-roles.mjs"));
    copyFileSync(new URL("../novel-agent-roles.mjs", import.meta.url), join(root, "electron", "novel-agent-roles.mjs"));
    copyFileSync(new URL("../author-language.mjs", import.meta.url), join(root, "electron", "author-language.mjs"));
    const run = (...args) => spawnSync(process.execPath, [join(root, "scripts", "generate-managed-agent-roles.mjs"), ...args], { encoding: "utf8" });
    assert.equal(run().status, 0);
    assert.equal(run("--check").status, 0);
    assert.equal(readFileSync(join(root, "shared", "novel-agent-roles.json"), "utf8"), renderNovelRoleCatalog());
    for (const role of NOVEL_AGENT_ROLES) assert.equal(readFileSync(join(root, "codex-runtime", "agents", `${role.id}.toml`), "utf8"), renderNovelAgentRole(role));
    const candidateProfile = join(root, "codex-runtime", "agents", "candidate_writer.toml");
    writeFileSync(candidateProfile, "developer_instructions = \"unauthorized drift\"\n");
    const profileDrift = run("--check");
    assert.notEqual(profileDrift.status, 0);
    assert.match(profileDrift.stderr, /Managed role artifact differs.*candidate_writer\.toml/);
    assert.equal(readFileSync(candidateProfile, "utf8"), "developer_instructions = \"unauthorized drift\"\n", "check must not silently repair drift");
    assert.equal(run().status, 0);
    writeFileSync(join(root, "shared", "novel-agent-roles.json"), "{}\n");
    const catalogDrift = run("--check");
    assert.notEqual(catalogDrift.status, 0);
    assert.match(catalogDrift.stderr, /Managed role artifact differs.*novel-agent-roles\.json/);
    assert.notEqual(run("--unsupported").status, 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
