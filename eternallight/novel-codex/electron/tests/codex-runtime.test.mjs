import { createMcpAuthorityFixture } from "./helpers/mcp-authority.mjs";
import assert from "node:assert/strict";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { createInterface } from "node:readline";
import { after, before, mock, test } from "node:test";
import { fileURLToPath } from "node:url";
import { CAPABILITIES, DELEGATION_EXECUTION_CAPABILITIES, ModelBridge, TaskBudget, chatCompletionsCodec, createExecutionLease, defineConnection, defineQualificationSuiteResult, resolveSessionPlan } from "@eternallight/agent-engine";
import { NOVEL_COLLABORATION_INSTRUCTIONS, NOVEL_COLLABORATION_MODE, readCollaborationPolicy, collaborationAvailability } from "../novel-collaboration.mjs";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";

import { assertProviderOperation, buildCodexConfig, buildCodexModelCatalog, checkCodexConnectionAccess, novelTurnText, composeNovelCodexPrompt, createCodexEnvironment, createQualificationContextDigest, createRuntimeEnvironment, createTaskCapability, credentialScopesFromEnvironment, inspectCodexConnection, installManagedAgentRoles, listCodexConnections, loadCodexEvidence, prepareIsolatedStorage, providerConfigurationError, resolveCodexCli, resolveCodexProvider, verifyCodexConnection } from "../codex-runtime.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, NOVEL_TOOL_DISCOVERY_POLICY, NOVEL_TOOL_OUTPUT_POLICY } from "../codex-runtime.mjs";
import { LANGUAGE_CONSISTENCY_INSTRUCTIONS } from "../author-language.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, NOVEL_CREATIVE_MISSION } from "../creative-mission.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { initializeFixtureCodexRepository, installFixtureExecutableInterpreter, writeFixtureBuildIdentity, writeFixtureCodexExecutable, writeFixtureNodeExecutable } from "./build-identity-fixture.mjs";
import { certificationPath } from "../../scripts/certification-publication.mjs";
import { NOVEL_AGENT_ROLES, NOVEL_ROLE_READ_TOOLS, NOVEL_ROLE_CANDIDATE_TOOLS, renderNovelAgentRole } from "../novel-agent-roles.mjs";
import { CREATIVE_STRATEGY_INSTRUCTIONS } from "../creative-strategy-policy.mjs";

test("managed roles describe bounded sources and trusted execution binding without invalid MCP overrides", () => {
  const catalog = JSON.parse(readFileSync(new URL("../../shared/creative-strategies.json", import.meta.url), "utf8"));
  assert.equal(catalog.status, "unvalidated");
  assert.equal(catalog.authority, "advisoryMethodOnly");
  assert.equal(new Set(catalog.strategies.map(item => item.id)).size, 8);
  assert.ok(NOVEL_COLLABORATION_INSTRUCTIONS.includes(CREATIVE_STRATEGY_INSTRUCTIONS));
  for (const strategy of catalog.strategies) {
    for (const field of ["when", "skip", "deliverable", "stop"]) assert.ok(strategy[field].trim());
    assert.ok(strategy.inputs.length && strategy.steps.length);
  }
  assert.match(CREATIVE_STRATEGY_INSTRUCTIONS, /discover_creative_capabilities/);
  assert.match(CREATIVE_STRATEGY_INSTRUCTIONS, /read_creative_strategy/);
  assert.match(CREATIVE_STRATEGY_INSTRUCTIONS, /do not load all methods by default/);
  assert.match(CREATIVE_STRATEGY_INSTRUCTIONS, /not guaranteed blind/);
  assert.match(CREATIVE_STRATEGY_INSTRUCTIONS, /Stop repeated revision/);
  assert.deepEqual(NOVEL_ROLE_READ_TOOLS, ["read_context", "read_source_content", "read_tool_guide", "read_work_issues", "search_story", "search_relations", "query_story", "read_creative_strategy"]);
  assert.deepEqual(NOVEL_ROLE_CANDIDATE_TOOLS, [...NOVEL_ROLE_READ_TOOLS, "read_delegated_draft", "save_delegated_draft"]);
  for (const role of NOVEL_AGENT_ROLES) {
    const rendered = renderNovelAgentRole(role);
    assert.equal(readFileSync(new URL(`../../codex-runtime/agents/${role.id}.toml`, import.meta.url), "utf8"), renderNovelAgentRole(role));
    assert.doesNotMatch(rendered, /mcp_servers|enabled_tools/, "Codex role overrides do not enforce MCP tool authority");
    assert.match(rendered, /Public language contract:/);
    assert.match(rendered, /does not establish that the parent read/);
    const developer = JSON.parse(rendered.match(/^developer_instructions = (.+)$/m)[1]);
    assert.ok(Buffer.byteLength(developer) <= 4096, role.id);
    assert.ok(modelInputTokenBudget(developer) <= 1000, role.id);
  }
  assert.match(NOVEL_COLLABORATION_INSTRUCTIONS, /only when save_delegated_draft is actually exposed/);
  assert.match(NOVEL_COLLABORATION_INSTRUCTIONS, /Runtime binds the trusted Core execution role and direct lineage automatically/);
  assert.match(NOVEL_COLLABORATION_INSTRUCTIONS, /delegatedCandidateId and the current formal draft revision/);
  assert.match(NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, /Research roles remain read-only/);
  assert.match(NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, /role name or request in prose never grants tool authority/);
});

test("invalid role packages fail before replacing installed roles", () => {
  const root = temporaryDirectory();
  try {
    const home = join(root, "home");
    const source = join(root, "source");
    mkdirSync(source);
    mkdirSync(join(home, "agents"), { recursive: true });
    const preserved = join(home, "agents", "current.toml");
    writeFileSync(preserved, "current");
    for (const role of NOVEL_AGENT_ROLES) writeFileSync(join(source, `${role.id}.toml`), renderNovelAgentRole(role));
    const file = join(source, "character_analyst.toml");
    writeFileSync(file, renderNovelAgentRole(NOVEL_AGENT_ROLES.find(role => role.id === "character_analyst")).replace('sandbox_mode = "read-only"', 'sandbox_mode = "danger-full-access"'));
    assert.throws(() => installManagedAgentRoles(home, source), /角色配置不一致/);
    assert.equal(readFileSync(preserved, "utf8"), "current");
    rmSync(file);
    assert.throws(() => installManagedAgentRoles(home, source), /角色集合/);
    assert.equal(readFileSync(preserved, "utf8"), "current");
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function temporaryDirectory() {
  return mkdtempSync(join(tmpdir(), "novel-codex-runtime-"));
}

const runtimeRoot = fileURLToPath(new URL("../../codex-runtime/", import.meta.url));
const modelCatalogSha256 = createHash("sha256").update(readFileSync(join(runtimeRoot, "model-catalog.json"))).digest("hex");
const activeCertificationsPath = certificationPath(runtimeRoot, JSON.parse(readFileSync(join(runtimeRoot, "supported-cli.json"), "utf8")));
const modelCertificationsSha256 = createHash("sha256").update(readFileSync(activeCertificationsPath)).digest("hex");
const TEST_NOW = new Date("2026-09-19T01:00:00Z");
const TEST_BRIDGE = { baseUrl: "http://127.0.0.1:43123", credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" };

let restoreFixtureInterpreter;
before(() => { restoreFixtureInterpreter = installFixtureExecutableInterpreter(mock); });
after(() => restoreFixtureInterpreter());

function verifiedProvider(capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP]) {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const evidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities,
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  return resolveCodexProvider({ evidence, now: TEST_NOW, provider: inspected.provider, model: inspected.model });
}

function executableChecksum(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

test("production provider compiles trusted portable discovery entries without changing operation authority", async (t) => {
  const configured = verifiedProvider();
  assert.deepEqual(configured.sessionPlan.toolDiscovery, NOVEL_TOOL_DISCOVERY_POLICY);
  assert.equal(Object.isFrozen(configured.sessionPlan.toolDiscovery), true);
  assert.deepEqual(configured.sessionPlan.toolDiscovery.deferredNamespaces, ["mcp__novel_workspace"]);
  assert.deepEqual(configured.sessionPlan.allowedMcpServerIds, ["novel_workspace"]);
  assert.deepEqual(configured.sessionPlan.toolDiscovery.initialTools.map(tool => tool.name),
    ["read_context", "read_media_workspace", "read_source_content", "read_tool_guide", "read_work_status", "read_work_issues", "request_candidate_task", "request_generation_handoff", "request_work_action", "save_draft_checkpoint", "complete_task"]);
  assert.equal(buildCodexModelCatalog({ baseInstructions: "Fixture instructions.", sessionPlan: configured.sessionPlan }).models[0].supports_search_tool, false);
  assert.equal(configured.sessionPlan.productExtensions.includes("toolSearch"), false);

  let scope, capturedSchemaRequest;
  const schemaConnection = defineConnection({ id: "schema-catalog", provider: "fixture", model: "schema-catalog", endpoint: "https://fixture.invalid",
    credentialRef: "BOOTSTRAP_FIXTURE", protocol: "chat-completions", declaredLimits: { contextTokens: 256000, outputTokens: 4000 } });
  const schemaPlan = resolveSessionPlan({ connections: [schemaConnection], evidence: verifiedBaselineEvidence(schemaConnection) });
  const authority = new ModelBridge({sessionPlan:schemaPlan,connections:[schemaConnection],mcpAuthority:true,
    credentialResolver:async()=>"fixture",requestConfigurationProvider:async({threadId})=>({mcpScope:scope,mcpIdentity:{threadId,agentRole:null}}),
    fetchImpl:async(_url, options)=>{capturedSchemaRequest=JSON.parse(options.body);return new Response(JSON.stringify({id:"schema",choices:[{finish_reason:"stop",message:{role:"assistant",content:"ok"}}],usage:{prompt_tokens:1,completion_tokens:1,total_tokens:2}}),{status:200,headers:{"content-type":"application/json"}});}});
  const authorityAddress=await authority.start();t.after(()=>authority.close());
  const schemaBudget=new TaskBudget();authority.beginTaskBudget(schemaBudget);
  const operationCatalog = operation => {
    const messages = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "bootstrap-catalog", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized", params: {} },
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
    ];
    const result = spawnSync(realpathSync(nativeTestBinary("novel-workspace-mcp")), [], {
      env: { ...createRuntimeEnvironment(process.env), NOVEL_CODEX_RUNTIME_URL: "http://127.0.0.1:1",
        NOVEL_CODEX_CANDIDATE_TOKEN: "bootstrap-catalog-fixture", NOVEL_CODEX_TASK_CAPABILITY: `bootstrap-catalog-${operation}`, NOVEL_CODEX_OPERATION: operation, NOVEL_CODEX_ENGINE_AUTHORITY_URL: authorityAddress.baseUrl, NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN: authorityAddress.token },
      input: `${messages.map(message => JSON.stringify(message)).join("\n")}\n`,
      encoding: "utf8", timeout: 90000, maxBuffer: 2 * 1024 * 1024,
    });
    assert.equal(result.error?.code ?? null, null, result.error?.message);
    assert.equal(result.status, 0, result.stderr);
    const replies = result.stdout.trim().split("\n").map(line => JSON.parse(line));
    assert.equal(replies.find(reply => reply.id === 1)?.result?.serverInfo?.name, "novel-codex-workspace");
    const catalog = replies.find(reply => reply.id === 2)?.result?.tools;
    assert.ok(Array.isArray(catalog) && catalog.length > 0, `${operation}: actual MCP catalog required`);
    return catalog;
  };
  const operations = ["advisory", "generateBrief", "generateOutline", "generatePlan", "generatePropertyRules", "generateChapter", "modelKnowledge", "extractCommitments", "reviseCommitment", "assessSceneOutcome", "assessChapterStructure", "assessRevisionOutcome", "reviewPlanningSeat", "reviewQuality"];
  const catalogs = new Map(operations.map(operation => [operation, operationCatalog(operation)]));
  assert.equal(catalogs.size, 14);
  const declarations = [...catalogs].flatMap(([operation, catalog]) => catalog.map(tool => ({ operation, tool })));
  const coreCatalogs = new Map();
  for (const [operation,catalog] of catalogs) {
    // Deliberately simulate Core's lossy schema DTO; authority must restore every
    // constraint from the actual MCP registration before provider serialization.
    scope=createHash("sha256").update(`bootstrap-catalog-${operation}`).digest("hex");
    // Catalog scopes are retired on sampling, so register the current process again.
    operationCatalog(operation);
    const lease=createExecutionLease({sessionPlan:schemaPlan,runId:operation,budgetId:schemaBudget.id});authority.beginExecutionLease(lease);
    const response=await fetch(`${authorityAddress.baseUrl}/responses`,{method:"POST",headers:{authorization:`Bearer ${authorityAddress.token}`,"content-type":"application/json","x-codex-turn-metadata":JSON.stringify({thread_id:"root",session_id:"root",turn_id:operation})},body:JSON.stringify({model:schemaConnection.id,input:[],stream:false,tools:[{type:"namespace",name:"mcp__novel_workspace",tools:catalog.map(tool=>({type:"function",name:tool.name,description:tool.description,parameters:{type:"object"},strict:false}))}]})});
    assert.equal(response.status,200,await response.text());
    for (const tool of catalog) {
      const wire=capturedSchemaRequest.tools.find(t=>t.function.name.endsWith(`__${tool.name}`));
      assert.deepEqual(wire?.function.parameters,tool.inputSchema,`${operation}/${tool.name}: Engine restores the actual MCP contract`);
    }
    coreCatalogs.set(operation,catalog.map(tool=>({type:"function",name:tool.name,description:tool.description,parameters:tool.inputSchema,strict:false})));
    await authority.endExecutionLease(lease);
  }
  for (const [operation, catalog] of catalogs) {
    for (const [index, tool] of catalog.entries()) {
      const outbound = chatCompletionsCodec.createRequest({ model: schemaConnection.id, input: [], stream: false,
        tools: [coreCatalogs.get(operation)[index]] }, schemaConnection, "fixture");
      assert.deepEqual(outbound.body.tools[0].function.parameters, tool.inputSchema, `${operation}/${tool.name}: full actual Native schema compiles without dropping constraints`);
    }
  }
  const argumentsBase = { expectedRevision: 0 };
  for (const [name, argumentsValue, schemaPath] of [
    ["read_context", { ...argumentsBase, expectedRevision: -1 }, "/properties/expectedRevision/minimum"],
    ["read_work_issues", { ...argumentsBase, issueId: "invalid@issue", cursor: 0, limit: 1 }, "/properties/issueId/anyOf"],
  ]) {
    const tool = coreCatalogs.get("advisory").find(tool => tool.name === name);
    assert.ok(tool);
    const outbound = chatCompletionsCodec.createRequest({ model: schemaConnection.id, input: [], stream: false, tools: [tool] }, schemaConnection, "fixture");
    assert.throws(() => chatCompletionsCodec.parseResponse({ id: "rejected-schema-arguments", choices: [{ finish_reason: "tool_calls", message: {
      tool_calls: [{ id: "never-executable", type: "function", function: { name: outbound.body.tools[0].function.name, arguments: JSON.stringify(argumentsValue) } }] } }],
      usage: { prompt_tokens: 2, completion_tokens: 2, total_tokens: 4 } }, outbound.context), error => {
      const prefix = "invalid_tool_arguments; toolValidation=";
      assert.ok(error.message.startsWith(prefix), error.message);
      assert.deepEqual(JSON.parse(error.message.slice(prefix.length)), { tool: name, reason: "constraint", schemaPath });
      return true;
    }, `${name}: arguments violating the Engine-restored constraint never become executable`);
  }
  const workAction = coreCatalogs.get("advisory").find(tool => tool.name === "request_work_action");
  assert.ok(workAction);
  assert.deepEqual(workAction.parameters.oneOf, [{ required: ["actionKey"] }, { required: ["proposal"] }]);
  assert.ok(Buffer.byteLength(workAction.parameters.description) <= 600);
  const workActionRequest = chatCompletionsCodec.createRequest({ model: schemaConnection.id, input: [], stream: false,
    tools: [workAction] }, schemaConnection, "fixture");
  const actionBase = { ...argumentsBase, instruction: "Revise within the approved scope.", idempotencyKey: "choice-contract",
    expectedWorkVersion: 1, expectedSourceDigest: "source-digest" };
  const actionKey = "ready-action";
  const proposal = { operation: "generateChapter", target: { chapterId: "chapter-current", scope: {
    type: "selectedScenes", sourceRevisionId: "draft-current", groupIds: ["scene-current"] } } };
  const parseAction = argumentsValue => chatCompletionsCodec.parseResponse({ id: "choice-contract-response", choices: [{
    finish_reason: "tool_calls", message: { tool_calls: [{ id: "choice-contract-call", type: "function", function: {
      name: workActionRequest.body.tools[0].function.name, arguments: JSON.stringify(argumentsValue) } }] } }],
    usage: { prompt_tokens: 2, completion_tokens: 2, total_tokens: 4 } }, workActionRequest.context);
  for (const argumentsValue of [{ ...actionBase, actionKey }, { ...actionBase, proposal }]) {
    const parsed = parseAction(argumentsValue);
    const call = parsed.output.find(item => item.type === "function_call");
    assert.ok(call, "a legal route remains executable after the Engine authority and protocol conversions");
    assert.deepEqual(JSON.parse(call.arguments), argumentsValue, "the Engine never chooses a route or rewrites its target");
  }
  for (const argumentsValue of [
    actionBase,
    { ...actionBase, actionKey, proposal },
    { ...actionBase, actionKey: null },
    { ...actionBase, proposal: null },
    { ...actionBase, actionKey, proposal: null },
    { ...actionBase, actionKey: null, proposal },
  ]) {
    assert.throws(() => parseAction(argumentsValue), error => {
      const prefix = "invalid_tool_arguments; toolValidation=";
      assert.ok(error.message.startsWith(prefix), error.message);
      const detail = JSON.parse(error.message.slice(prefix.length));
      assert.equal(detail.tool, "request_work_action");
      assert.ok(["/oneOf", "/properties/actionKey/type", "/properties/proposal/type"].includes(detail.schemaPath), detail.schemaPath);
      assert.equal(detail.reason, detail.schemaPath === "/oneOf" ? "constraint" : "type");
      if (argumentsValue === actionBase || (argumentsValue.actionKey === actionKey && argumentsValue.proposal === proposal)) {
        assert.equal(detail.schemaPath, "/oneOf", "both and neither routes fail the exact choice constraint");
      }
      assert.deepEqual(Object.keys(detail).sort(), ["reason", "schemaPath", "tool"], "the error contains no author input or argument values");
      return true;
    }, "invalid mutually exclusive or null routes never become executable");
  }
  const controls = ["request_candidate_task", "request_generation_handoff", "request_work_action"];
  for (const name of controls) {
    assert.ok(catalogs.get("advisory").some(tool => tool.name === name), name);
    assert.equal(catalogs.get("generateChapter").some(tool => tool.name === name), false, `${name}: writer has no control admission`);
  }
  const connection = defineConnection({ id: "bootstrap-fixture", provider: "fixture", model: "bootstrap-fixture", endpoint: "https://fixture.invalid",
    credentialRef: "BOOTSTRAP_FIXTURE", protocol: "responses", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection),
    policy: { toolDiscovery: NOVEL_TOOL_DISCOVERY_POLICY, allowedMcpServerIds: ["novel_workspace"] } });
  const requests = [];
  let invalidArguments = null;
  const bridge = new ModelBridge({ sessionPlan: plan, connections: [connection], token: "bootstrap-bridge-fixture", credentialResolver: async () => "fixture",
    fetchImpl: async (_url, options) => {
      requests.push(JSON.parse(options.body));
      return new Response(JSON.stringify({ id: `bootstrap-response-${requests.length}`, status: "completed",
        output: invalidArguments ? [{ id: "rejected-bootstrap-call", type: "function_call", call_id: "never-executable", namespace: "mcp__novel_workspace", name: "read_context", arguments: JSON.stringify(invalidArguments) }]
          : [{ id: `bootstrap-answer-${requests.length}`, type: "message", role: "assistant", status: "completed", content: [{ type: "output_text", text: "Schema inspection complete.", annotations: [] }] }],
        usage: { input_tokens: 2, output_tokens: 2, total_tokens: 4 } }), { headers: { "content-type": "application/json" } });
    } });
  t.after(() => bridge.close());
  const { baseUrl } = await bridge.start();
  const budget = new TaskBudget({ maxRequests: 4, maxTokens: 64000 });
  bridge.beginTaskBudget(budget);
  const initialNames = [...NOVEL_TOOL_DISCOVERY_POLICY.initialTools, ...NOVEL_TOOL_DISCOVERY_POLICY.discoveryTools].map(tool => tool.name);
  for (const operation of ["advisory", "generateChapter"]) {
    const catalog = catalogs.get(operation);
    const lease = createExecutionLease({ sessionPlan: plan, runId: `bootstrap-${operation}`, budgetId: budget.id });
    bridge.beginExecutionLease(lease);
    const response = await fetch(`${baseUrl}/v1/responses`, { method: "POST", headers: {
      authorization: "Bearer bootstrap-bridge-fixture", "content-type": "application/json",
      "x-codex-turn-metadata": JSON.stringify({ thread_id: "bootstrap-root", parent_thread_id: null, session_id: "bootstrap-session", turn_id: operation, request_kind: "turn" }),
    }, body: JSON.stringify({ model: connection.id, input: [], stream: false, max_output_tokens: 32,
      tools: [{ type: "namespace", name: "mcp__novel_workspace", tools: coreCatalogs.get(operation) }] }) });
    const output = await response.text();
    assert.equal(response.status, 200, output);
    const actual = requests.at(-1).tools.find(tool => tool.type === "namespace" && tool.name === "mcp__novel_workspace").tools;
    assert.deepEqual(actual.map(tool => tool.name).sort(), catalog.filter(tool => initialNames.includes(tool.name)).map(tool => tool.name).sort(), operation);
    for (const tool of actual) {
      const admitted = catalog.find(entry => entry.name === tool.name);
      assert.deepEqual(tool.parameters, admitted.inputSchema, `${operation}/${tool.name}: retain the actual admitted schema`);
      assert.equal(tool.description, admitted.description);
    }
    for (const name of controls) assert.equal(actual.some(tool => tool.name === name), operation === "advisory", `${operation}/${name}`);
    assert.ok(actual.some(tool => tool.name === "read_context"));
    assert.ok(actual.some(tool => tool.name === "read_work_issues"), "issue state remains directly readable after restart");
    assert.ok(actual.some(tool => tool.name === "complete_task"));
    assert.equal(actual.some(tool => tool.name === "read_creative_strategy"), false, "optional methods remain deferred");
    if (operation === "generateChapter") {
      assert.ok(catalog.some(tool => tool.name === "save_draft_checkpoint"), "writer still has its original domain write admission");
      assert.equal(actual.some(tool => tool.name === "save_draft_checkpoint"), true, "an authorized writer starts with its core save operation");
    }
    await bridge.endExecutionLease(lease);
  }
  assert.equal(requests.length, 2);
  assert.equal(budget.snapshot().requests, requests.length, "bootstrap does not bypass the original Engine budget");
  invalidArguments = { ...argumentsBase, expectedRevision: -1 };
  const invalidLease = createExecutionLease({ sessionPlan: plan, runId: "bootstrap-invalid-arguments", budgetId: budget.id });
  bridge.beginExecutionLease(invalidLease);
  const rejection = await fetch(`${baseUrl}/v1/responses`, { method: "POST", headers: { authorization: "Bearer bootstrap-bridge-fixture", "content-type": "application/json",
    "x-codex-turn-metadata": JSON.stringify({ thread_id: "bootstrap-root", parent_thread_id: null, session_id: "bootstrap-session", turn_id: "invalid-arguments", request_kind: "turn" }) },
    body: JSON.stringify({ model: connection.id, input: [], stream: false, max_output_tokens: 32,
      tools: [{ type: "namespace", name: "mcp__novel_workspace", tools: coreCatalogs.get("advisory") }] }) });
  const rejected = await rejection.json();
  assert.equal(rejection.status, 400);
  assert.equal(rejected.error.code, "invalid_tool_arguments");
  assert.equal(rejected.error.retryable, false);
  const prefix = "invalid_tool_arguments; toolValidation=";
  assert.ok(rejected.error.message.startsWith(prefix), rejected.error.message);
  assert.deepEqual(JSON.parse(rejected.error.message.slice(prefix.length)), { tool: "read_context", reason: "constraint", schemaPath: "/properties/expectedRevision/minimum" });
  assert.equal(rejected.output, undefined, "invalid arguments cannot be returned as an executable model tool call");
  await bridge.endExecutionLease(invalidLease);
  assert.equal(requests.length, 3);
  assert.equal(budget.snapshot().requests, requests.length);
  assert.equal(budget.snapshot().tokens, 12);
  assert.equal(budget.snapshot().unknownTokens, 0);
  assert.equal(budget.snapshot().reservedTokens, 0);
});

function writeCliPolicy(root, digests = {}, buildIdentities = {}) {
  const path = join(root, "codex-runtime", "supported-cli.json");
  mkdirSync(dirname(path), { recursive: true });
  copyFileSync(join(runtimeRoot, "model-catalog.json"), join(root, "codex-runtime", "model-catalog.json"));
  copyFileSync(activeCertificationsPath, join(root, "codex-runtime", "model-certifications.json"));
  const versions = ["codex-cli 0.0.0"];
  writeFileSync(path, JSON.stringify({ version: 1, modelCertificationsFile: "model-certifications.json", appServerProtocol: "v2", mcpProtocol: "2025-06-18", modelCatalogSha256, modelCertificationsSha256, sources: { bundled: { packaged: true, versions, digests: digests.bundled ?? {}, buildIdentities: buildIdentities.bundled ?? {} }, "project-runtime": { packaged: false, versions, digests: digests["project-runtime"] ?? {}, buildIdentities: buildIdentities["project-runtime"] ?? {} }, "workspace-build": { packaged: false, developmentOnly: true, versions, digests: digests["workspace-build"] ?? {}, buildIdentities: buildIdentities["workspace-build"] ?? {} }, explicit: { packaged: false, developmentOnly: true, versions, digests: digests.explicit ?? {}, buildIdentities: buildIdentities.explicit ?? {} } } }));
  const bundledIdentity = buildIdentities.bundled?.["darwin-arm64"] ?? null;
  writeFileSync(join(root, "release-manifest.json"), JSON.stringify({ version: 1, target: { platform: "darwin", architecture: "arm64" }, codex: { binarySha256: digests.bundled?.["darwin-arm64"] ?? null, unsignedBinarySha256: bundledIdentity?.unsignedBinarySha256 ?? null, buildIdentityId: bundledIdentity?.identityId ?? null, buildIdentitySha256: bundledIdentity?.sha256 ?? null }, resources: {} }));
}

test("fixture executables preserve argv and stdin without resolving Node through PATH", () => {
  const root = temporaryDirectory();
  try {
    const directory = join(root, "an author's fixtures");
    const cli = join(directory, "codex");
    const protocol = join(directory, "protocol-worker");
    const env = { PATH: join(root, "no-interpreters-on-path"), ELECTRON_RUN_AS_NODE: "1" };
    writeFixtureCodexExecutable(cli);
    const version = spawnSync(cli, ["--version"], { env, encoding: "utf8", timeout: 15_000 });
    assert.deepEqual([version.error?.code ?? null, version.status, version.stdout, version.stderr], [null, 0, "codex-cli 0.0.0\n", ""]);
    const unsupported = spawnSync(cli, ["app-server"], { env, encoding: "utf8", timeout: 15_000 });
    assert.deepEqual([unsupported.status, unsupported.stdout, unsupported.stderr], [64, "", "fixture Codex supports only --version\n"]);

    writeFixtureNodeExecutable(protocol, `process.stdout.write(JSON.stringify({
  executable: process.execPath, args: process.argv.slice(2), input: require("node:fs").readFileSync(0, "utf8")
}));\n`);
    const args = ["--port", "43123", "an argument's spaces"];
    const input = '{"jsonrpc":"2.0","id":1,"method":"initialize"}\n';
    const response = spawnSync(protocol, args, { env, input, encoding: "utf8", timeout: 15_000 });
    assert.deepEqual([response.error?.code ?? null, response.status, response.stderr], [null, 0, ""]);
    assert.deepEqual(JSON.parse(response.stdout), { executable: process.execPath, args, input });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fixture build identity reports interpreter and child exit diagnostics", () => {
  const root = temporaryDirectory();
  try {
    const binaryPath = join(root, "broken-cli");
    writeFixtureNodeExecutable(binaryPath, "process.stderr.write('fixture refused version'); process.exit(7);\n");
    assert.throws(() => writeFixtureBuildIdentity({ binaryPath, identityPath: join(root, "build-identity.json") }), error => {
      assert.match(error.message, /exit=7; elapsed=\d+ms/);
      assert.ok(error.message.includes(`path=${binaryPath}; interpreter=#!/usr/bin/env -S`));
      assert.match(error.message, /stderr=fixture refused version/);
      return true;
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("resolves the packaged CLI instead of PATH", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  writeFixtureCodexExecutable(cliPath);
  const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
  const { identity, sha256 } = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: identity.identityId, sha256, unsignedBinarySha256: identity.binary.sha256 } } });
  try {
    assert.deepEqual(resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath }), {
      path: realpathSync(cliPath),
      source: "bundled",
      reason: null,
    });
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("requires packaged BuildIdentity provenance while allowing signed executable bytes", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  writeFixtureCodexExecutable(cliPath);
  writeCliPolicy(resourcesPath);
  try {
    const missing = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(missing.path, null);
    assert.match(missing.reason, /BuildIdentity/);
    const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
    const recorded = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
    appendFileSync(cliPath, "\n// changed after build\n");
    writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: recorded.identity.identityId, sha256: recorded.sha256, unsignedBinarySha256: recorded.identity.binary.sha256 } } });
    const signed = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(signed.path, realpathSync(cliPath));

    writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: recorded.identity.identityId, sha256: recorded.sha256, unsignedBinarySha256: "0".repeat(64) } } });
    const stale = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(stale.path, null);
    assert.match(stale.reason, /未签名引擎与 BuildIdentity 不一致/);
    assert.equal(stale.diagnostics[0].code, "cli_unsigned_hash_mismatch");
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("requires a SHA manifest for a project runtime and permits only the development workspace build fallback", () => {
  const root = temporaryDirectory();
  const projectRoot = join(root, "app", "novel");
  const projectCli = join(projectRoot, "codex-runtime", "bin", "codex");
  const workspaceCli = join(projectRoot, "..", "..", "codex-rs", "target", "debug", "codex");
  const sourceTreeDigest = initializeFixtureCodexRepository(root);
  writeFixtureCodexExecutable(projectCli);
  writeFixtureCodexExecutable(workspaceCli);
  const identityPath = join(projectRoot, "codex-runtime", "build-identities", "darwin-arm64.json");
  const workspaceIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: workspaceCli, identityPath, platform: "darwin", repositoryRoot: root, sourceTreeDigest });
  writeCliPolicy(projectRoot, { "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } }, { "workspace-build": { "darwin-arm64": { identityId: workspaceIdentity.identity.identityId, sha256: workspaceIdentity.sha256 } } });
  try {
    const fallback = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(fallback.source, "workspace-build");
    assert.equal(fallback.path, realpathSync(workspaceCli));
    const projectIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: projectCli, identityPath, platform: "darwin", repositoryRoot: root, sourceTreeDigest });
    writeCliPolicy(projectRoot, { "project-runtime": { "darwin-arm64": executableChecksum(projectCli) }, "workspace-build": { "darwin-arm64": executableChecksum(workspaceCli) } }, { "project-runtime": { "darwin-arm64": { identityId: projectIdentity.identity.identityId, sha256: projectIdentity.sha256 } } });
    const pinned = resolveCodexCli({ architecture: "arm64", isPackaged: false, platform: "darwin", projectRoot, resourcesPath: "/unused" });
    assert.equal(pinned.source, "project-runtime");
    assert.equal(pinned.path, realpathSync(projectCli));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects an explicit CLI in a packaged build", () => {
  const root = temporaryDirectory();
  const bundledCli = join(root, "codex-runtime", "bin", "codex");
  writeFixtureCodexExecutable(bundledCli);
  const identityPath = join(root, "codex-runtime", "build-identity.json");
  const bundledIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: bundledCli, identityPath, platform: "darwin" });
  writeCliPolicy(root, { bundled: { "darwin-arm64": executableChecksum(bundledCli) } }, { bundled: { "darwin-arm64": { identityId: bundledIdentity.identity.identityId, sha256: bundledIdentity.sha256, unsignedBinarySha256: bundledIdentity.identity.binary.sha256 } } });
  rmSync(bundledCli);
  const desktopCli = join(root, "Applications", "ChatGPT.app", "Contents", "Resources", "codex");
  writeFixtureCodexExecutable(desktopCli);
  try {
    const result = resolveCodexCli({ architecture: "arm64", explicitPath: desktopCli, isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath: root });
    assert.equal(result.path, null);
    assert.equal(result.source, "missing");
    assert.match(result.reason, /未获.*授权/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects a digest-pinned CLI whose reported version is not supported", () => {
  const resourcesPath = temporaryDirectory();
  const cliPath = join(resourcesPath, "codex-runtime", "bin", "codex");
  writeFixtureCodexExecutable(cliPath, "codex-cli 99.0.0");
  const identityPath = join(resourcesPath, "codex-runtime", "build-identity.json");
  const wrongVersionIdentity = writeFixtureBuildIdentity({ architecture: "arm64", binaryPath: cliPath, identityPath, platform: "darwin" });
  writeCliPolicy(resourcesPath, { bundled: { "darwin-arm64": executableChecksum(cliPath) } }, { bundled: { "darwin-arm64": { identityId: wrongVersionIdentity.identity.identityId, sha256: wrongVersionIdentity.sha256, unsignedBinarySha256: wrongVersionIdentity.identity.binary.sha256 } } });
  try {
    const result = resolveCodexCli({ architecture: "arm64", isPackaged: true, platform: "darwin", projectRoot: "/unused", resourcesPath });
    assert.equal(result.path, null);
    assert.match(result.reason, /引擎版本不在支持清单/);
    assert.equal(result.diagnostics[0].code, "cli_version_unsupported");
  } finally {
    rmSync(resourcesPath, { recursive: true, force: true });
  }
});

test("keeps inherited provider credentials out of the isolated Codex child environment", () => {
  const env = createCodexEnvironment({ CODEX_HOME: "/desktop", CODEX_API_KEY: "implicit-key", CODEX_ACCESS_TOKEN: "secret", OPENAI_API_KEY: "key", PATH: "/usr/bin" }, {
    codexHome: "/novel/codex-home",
    provider: "deepseek",
    sqliteHome: "/novel/codex-home/sqlite",
  });
  assert.equal(env.CODEX_HOME, "/novel/codex-home");
  assert.equal(env.CODEX_SQLITE_HOME, "/novel/codex-home/sqlite");
  assert.equal(env.CODEX_API_KEY, undefined);
  assert.equal(env.CODEX_ACCESS_TOKEN, undefined);
  assert.equal(env.OPENAI_API_KEY, undefined);
  assert.equal(env.PATH, "/usr/bin");
});

test("qualification context binds the current CLI and product base instructions", () => {
  const root = temporaryDirectory();
  try {
    const cliPath = join(root, "codex");
    writeFixtureCodexExecutable(cliPath);
    const first = createQualificationContextDigest({ baseInstructions: "instruction-v1", cliPath });
    const second = createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath });
    assert.match(first, /^[a-f0-9]{64}$/);
    assert.notEqual(first, second);
    assert.notEqual(second, createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath,
      developerInstructions: `${NOVEL_ROOT_DEVELOPER_INSTRUCTIONS}\nChanged root policy.` }));
    assert.notEqual(second, createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath,
      subagentDeveloperInstructions: `${NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS}\nChanged child policy.` }));
    writeFixtureCodexExecutable(cliPath, "codex-cli 0.0.1");
    assert.notEqual(second, createQualificationContextDigest({ baseInstructions: "instruction-v2", cliPath }));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("allows only catalogued provider/model operations", () => {
  assert.throws(() => resolveCodexProvider({}), /lacks verified capabilities/);
  const expected = verifiedProvider();
  assert.equal(expected.id, "deepseek-flash");
  assert.equal(expected.provider, "DeepSeek");
  assert.equal(expected.model, "deepseek-flash");
  assert.equal(expected.credentialRef, "DEEPSEEK_API_KEY");
  assert.equal(expected.sessionPlan.routes[0].transport, "native-passthrough");
  for (const operation of ["advisory", "generateBrief", "generateOutline", "generatePlan", "generateChapter", "reviewPlanningSeat"]) {
    assert.equal(assertProviderOperation(expected, operation).model, "deepseek-flash");
  }
  assert.throws(() => resolveCodexProvider({ model: "gpt-5.6-terra", provider: "openai" }), /未知模型连接/);
  assert.throws(() => resolveCodexProvider({ model: "gpt-arbitrary", provider: "openai" }), /未知模型连接/);
  assert.throws(() => resolveCodexProvider({ model: "DeepSeek-V4.1-Flash", provider: "deepseek" }), /未知模型连接/);
});

test("gates each product operation by a verified capability profile without changing the frozen plan", () => {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const baseEvidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP],
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const baseAdvisory = resolveCodexProvider({ evidence: baseEvidence, now: TEST_NOW, operation: "advisory", provider: inspected.provider, model: inspected.model });
  assert.equal(baseAdvisory.operationProfileId, "base-tools-v1");

  const interactiveEvidence = verifiedBaselineEvidence(inspected.connection, {
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const configured = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, provider: inspected.provider, model: inspected.model });
  const advisory = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, operation: "advisory", provider: inspected.provider, model: inspected.model });
  assert.equal(advisory.operationProfileId, "base-tools-v1");
  assert.equal(advisory.sessionPlan.id, configured.sessionPlan.id);
  const baseReview = resolveCodexProvider({ evidence: interactiveEvidence, now: TEST_NOW, operation: "reviewPlanningSeat", provider: inspected.provider, model: inspected.model });
  assert.equal(baseReview.operationProfileId, "base-tools-v1");

  const autonomousEvidence = verifiedBaselineEvidence(inspected.connection, {
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const review = resolveCodexProvider({ evidence: autonomousEvidence, now: TEST_NOW, operation: "reviewPlanningSeat", provider: inspected.provider, model: inspected.model });
  assert.equal(review.operationProfileId, "base-tools-v1");
  assert.deepEqual(review.sessionPlan.authorizedCapabilityProfiles, ["base-tools-v1", "directed-delegation-v1", "interactive-v1", "text-v1"]);
});

test("generates model capabilities from the frozen session plan", () => {
  const catalog = buildCodexModelCatalog({ baseInstructions: "Upstream Codex test instructions.", sessionPlan: verifiedProvider().sessionPlan });
  assert.deepEqual(catalog.models.map((model) => ({
    includeApps: model.include_apps_usage_instructions,
    includePlugins: model.include_plugin_usage_instructions,
    multiAgentVersion: model.multi_agent_version,
    nodeReplDisabled: model.node_repl_disabled,
    toolMode: model.tool_mode,
    useResponsesLite: model.use_responses_lite,
  })), [{
    includeApps: false,
    includePlugins: false,
    multiAgentVersion: null,
    nodeReplDisabled: true,
    toolMode: "direct",
    useResponsesLite: false,
  }]);
});

test("creates a short-lived task capability without exposing its hash as the secret", () => {
  const capability = createTaskCapability(1_800_000);
  assert.equal(capability.secret.length >= 24, true);
  assert.equal(capability.hash, createHash("sha256").update(capability.secret).digest("hex"));
  assert.equal(capability.expiresAt, 3_600);
  assert.notEqual(capability.secret, capability.hash);
});

test("starts the current runtime schema in a fresh database", () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    assert.equal(storage.runtimeDb, join(root, "creative-runtime", "novel-codex-v2.sqlite3"));
    assert.equal(storage.evidencePath, join(root, "agent-engine", "capability-evidence.json"));
    assert.equal(storage.recoveryPath, join(root, "agent-engine", "recovery-index.json"));
    assert.equal(storage.credentialSaltPath, join(root, "agent-engine", "credential-scope.key"));
    assert.match(readFileSync(storage.credentialSaltPath, "utf8").trim(), /^[A-Za-z0-9_-]{43}$/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("invalidates capability evidence after a credential rotation without persisting either credential", () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const firstScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "first-credential" }, storage.credentialSaltPath);
    const repeatedScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "first-credential" }, storage.credentialSaltPath);
    const rotatedScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "rotated-credential" }, storage.credentialSaltPath);
    assert.deepEqual(firstScopes, repeatedScopes);
    assert.notEqual(firstScopes[provider.id], rotatedScopes[provider.id]);
    const evidence = verifiedBaselineEvidence(provider.connection, {
      credentialScope: firstScopes[provider.id],
      protocol: "responses",
      checkedAt: "2026-09-19T00:00:00Z",
      expiresAt: "2026-09-20T00:00:00Z",
    });
    assert.equal(resolveCodexProvider({ credentialScopes: firstScopes, evidence, now: TEST_NOW, provider: provider.provider, model: provider.model }).sessionPlan.routes[0].connectionId, provider.id);
    assert.throws(
      () => resolveCodexProvider({ credentialScopes: rotatedScopes, evidence, now: TEST_NOW, provider: provider.provider, model: provider.model }),
      /lacks verified capabilities/,
    );
    assert.doesNotMatch(JSON.stringify({ firstScopes, rotatedScopes, salt: readFileSync(storage.credentialSaltPath, "utf8") }), /first-credential|rotated-credential/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("persists explicit connection verification evidence without storing the credential", async () => {
  const root = temporaryDirectory();
  try {
    const storage = prepareIsolatedStorage(root);
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const credentialScopes = credentialScopesFromEnvironment({ DEEPSEEK_API_KEY: "test-secret-never-persist" }, storage.credentialSaltPath);
    const records = verifiedBaselineEvidence(provider.connection, {
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
      credentialScope: credentialScopes[provider.id],
      protocol: "responses",
      checkedAt: "2026-09-19T00:00:00Z",
      expiresAt: "2026-09-20T00:00:00Z",
    });
    const result = await verifyCodexConnection({
      baseEnvironment: { DEEPSEEK_API_KEY: "test-secret-never-persist" },
      credentialScopes,
      evidencePath: join(root, "evidence.json"),
      provider: "deepseek",
      model: "deepseek-flash",
      coreRunner: { verify: async ({ protocol, now, credentialScope }) => ({
        kind: "core-integration",
        suiteId: "codex-engine-core-integration-v1",
        suiteVersion: 1,
        status: "passed",
        sampleCount: 1,
        successes: 1,
        blockingFailures: [],
        metrics: {},
        record: {
          capability: CAPABILITIES.TOOL_LOOP,
          status: "verified",
          scope: { protocol, suite: "codex-engine-core-integration-v1", mode: "app-server-mcp", toolTypes: ["function"] },
          source: "conformance-suite",
          credentialRealm: records[0].credentialRealm,
          environmentDigest: records[0].environmentDigest,
          evidenceRef: `core:${now.toISOString()}`,
          checkedAt: now.toISOString(),
          expiresAt: new Date(now.getTime() + 86_400_000).toISOString(),
          sample: { suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, outcome: "passed" },
          assertions: [{ id: "tool-loop-completion", result: "passed", expected: true, observed: true, traceRef: "fixture-core" }],
          dependencies: records[0].dependencies,
          supersedes: [],
        },
      }) },
      conformanceRunner: { verify: async ({ apiKey, credentialScope, now, probeEvidence, protocol }) => {
        assert.equal(apiKey, "test-secret-never-persist");
        assert.equal(credentialScope, credentialScopes[provider.id]);
        assert.equal(protocol, "responses");
        assert.deepEqual(probeEvidence, records);
        return {
          attempt: { protocol, suite: "codex-engine-delegation-execution-v1", status: "verified" },
          records: [CAPABILITIES.CHILD_SPAWN, CAPABILITIES.CHILD_MESSAGE_RUNNING, CAPABILITIES.CHILD_WAIT, CAPABILITIES.CHILD_DIRECTED_ROUTE].map((capability) => ({
            capability,
            status: "verified",
            scope: { protocol, suite: "codex-engine-delegation-execution-v1", mode: "app-server-mcp", toolTypes: ["function"], ...(capability === CAPABILITIES.CHILD_DIRECTED_ROUTE ? { routes: [{ parentConnectionId: provider.id, childConnectionId: provider.id }] } : {}) },
            source: "conformance-suite",
            environmentDigest: records[0].environmentDigest,
            evidenceRef: `conformance:${provider.connection.id}:${now.toISOString()}:${capability}`,
            checkedAt: now.toISOString(),
            expiresAt: new Date(now.getTime() + 86_400_000).toISOString(),
            sample: { suiteId: "codex-engine-delegation-execution-v1", suiteVersion: 1, outcome: "passed", eventDigest: "a".repeat(64) },
            assertions: [{ id: `fixture:${capability}`, result: "passed", expected: true, observed: true, traceRef: "a".repeat(64) }],
            dependencies: records[0].dependencies,
            supersedes: [],
          })),
        };
      } },
      verifier: { verify: async (_connection, { apiKey, credentialScope }) => {
        assert.equal(apiKey, "test-secret-never-persist");
        assert.equal(credentialScope, credentialScopes[provider.id]);
        return { selectedProtocol: "responses", attempts: [{ protocol: "responses", status: "verified", message: "provider debug: test-secret-never-persist" }], records };
      } },
    });
    assert.equal(result.selectedProtocol, "responses");
    assert.match(result.reportId, /^[a-f0-9]{64}$/);
    const storedEvidence = await loadCodexEvidence(join(root, "evidence.json"));
    assert.equal(storedEvidence.length, records.length + 5);
    const childRoute = storedEvidence.find((record) => record.capability === CAPABILITIES.CHILD_DIRECTED_ROUTE);
    assert.equal(childRoute.reportRef, result.reportId);
    assert.equal(childRoute.reportDigest, result.reportId);
    assert.doesNotMatch(readFileSync(join(root, "evidence.json"), "utf8"), /test-secret-never-persist/);
    const reports = readdirSync(join(root, "capability-reports"));
    assert.equal(reports.length, 1);
    const report = JSON.parse(readFileSync(join(root, "capability-reports", reports[0]), "utf8"));
    assert.equal(report.reportId, result.reportId);
    assert.equal(report.status, "verified");
    assert.equal(report.connection.id, "deepseek-flash");
    assert.doesNotMatch(JSON.stringify(report), /test-secret-never-persist/);
    const evidencePath = join(root, "evidence.json");
    const tampered = JSON.parse(readFileSync(evidencePath, "utf8"));
    const tamperedRecord = tampered.records.find((record) => record.capability === CAPABILITIES.CHILD_DIRECTED_ROUTE);
    tamperedRecord.expiresAt = "2026-09-21T00:00:00.000Z";
    writeFileSync(evidencePath, JSON.stringify(tampered));
    await assert.rejects(loadCodexEvidence(evidencePath), /fields? (?:has|have) been tampered|字段已被篡改/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("published connections use a bounded account check without rerunning conformance", async () => {
  const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const result = await checkCodexConnectionAccess({
    apiKey: "account-secret",
    connection: provider.connection,
    fetchImpl: async (url, options) => {
      assert.equal(url, "https://api.deepseek.com/v1/responses");
      assert.equal(options.headers.authorization, "Bearer account-secret");
      const body = JSON.parse(options.body);
      assert.equal(body.tools.length, 0);
      assert.equal(body.tool_choice, undefined);
      return new Response(JSON.stringify({ id: "account-check", status: "completed", output: [{ type: "message", role: "assistant", content: [{ type: "output_text", text: "ok" }] }] }), { status: 200 });
    },
  });
  assert.deepEqual(result, { status: "available", protocol: "responses" });
  await assert.rejects(checkCodexConnectionAccess({
    apiKey: "invalid",
    connection: provider.connection,
    fetchImpl: async () => new Response(JSON.stringify({ error: { message: "unauthorized" } }), { status: 401 }),
  }), (error) => error.statusCode === 401);
});

test("failed core tool-loop evidence prevents child conformance from running", async () => {
  const root = temporaryDirectory();
  let childConformanceCalls = 0;
  try {
    const provider = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
    const records = verifiedBaselineEvidence(provider.connection, {
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS],
      credentialScope: provider.connection.credentialRef,
      protocol: "responses",
    });
    const result = await verifyCodexConnection({
      baseEnvironment: { DEEPSEEK_API_KEY: "core-failure-secret" },
      evidencePath: join(root, "evidence.json"),
      provider: "deepseek",
      model: "deepseek-flash",
      verifier: { verify: async () => ({ selectedProtocol: "responses", attempts: [], records }) },
      coreRunner: { verify: async () => ({
        ...defineQualificationSuiteResult({ kind: "core-integration", suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, status: "failed", blockingFailures: ["core-tool-loop-error"], metrics: { message: "session rejected credential core-failure-secret" } }),
        record: {
          capability: CAPABILITIES.TOOL_LOOP,
          status: "failed",
          scope: { protocol: "responses", suite: "codex-engine-core-integration-v1", mode: "app-server-mcp", toolTypes: ["function"] },
          source: "conformance-suite",
          credentialRealm: provider.connection.credentialRef,
          environmentDigest: records[0].environmentDigest,
          evidenceRef: "core-failed",
          checkedAt: TEST_NOW.toISOString(),
          expiresAt: new Date(TEST_NOW.getTime() + 86_400_000).toISOString(),
          sample: { suiteId: "codex-engine-core-integration-v1", suiteVersion: 1, outcome: "failed" },
          assertions: [{ id: "tool-loop-completion", result: "failed", expected: true, observed: false, traceRef: "core-failed" }],
          dependencies: records[0].dependencies,
          supersedes: [],
        },
      }) },
      conformanceRunner: { verify: async () => { childConformanceCalls += 1; throw new Error("must not run"); } },
    });
    assert.equal(childConformanceCalls, 0);
    assert.equal(result.attempts.at(-1).status, "failed");
    assert.equal(result.attempts.at(-1).message, "core-tool-loop-error: session rejected credential [REDACTED]");
    const reportDirectory = join(root, "capability-reports");
    const report = JSON.parse(readFileSync(join(reportDirectory, readdirSync(reportDirectory)[0]), "utf8"));
    assert.equal(report.attempts.at(-1).message, result.attempts.at(-1).message);
    assert.equal(JSON.stringify(report).includes("core-failure-secret"), false);
    const evidence = await loadCodexEvidence(join(root, "evidence.json"));
    assert.equal(evidence.some((record) => record.capability === CAPABILITIES.TOOL_LOOP && record.status === "failed"), true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("persists a redacted failed capability report", async () => {
  const root = temporaryDirectory();
  try {
    await assert.rejects(
      verifyCodexConnection({
        baseEnvironment: { DEEPSEEK_API_KEY: "failed-secret-never-persist" },
        evidencePath: join(root, "evidence.json"),
        provider: "deepseek",
        model: "deepseek-flash",
        verifier: { verify: async () => {
          const error = new Error("provider echoed failed-secret-never-persist");
          error.attempts = [{ protocol: "responses", status: "failed", category: "authentication", message: "failed-secret-never-persist" }];
          error.records = [];
          throw error;
        } },
      }),
      (error) => {
        assert.match(error.reportId, /^[a-f0-9]{64}$/);
        assert.doesNotMatch(error.message, /failed-secret-never-persist/);
        return true;
      },
    );
    const reports = readdirSync(join(root, "capability-reports"));
    assert.equal(reports.length, 1);
    const report = readFileSync(join(root, "capability-reports", reports[0]), "utf8");
    assert.match(report, /\[REDACTED\]/);
    assert.doesNotMatch(report, /failed-secret-never-persist/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("installs valid managed role profiles while Native remains the tool authority", () => {
  const root = temporaryDirectory();
  try {
    const codexHome = join(root, "codex-home");
    mkdirSync(join(codexHome, "agents"), { recursive: true });
    writeFileSync(join(codexHome, "agents", "stale.toml"), "stale = true\n");
    const roles = installManagedAgentRoles(codexHome, join(runtimeRoot, "agents"));
    assert.deepEqual(roles, ["candidate_writer.toml", "character_analyst.toml", "continuity_critic.toml", "default.toml", "explorer.toml", "reader_experience.toml", "story_architect.toml", "style_reviewer.toml", "worker.toml"]);
    assert.equal(existsSync(join(codexHome, "agents", "stale.toml")), false);
    for (const role of roles) {
      const contents = readFileSync(join(codexHome, "agents", role), "utf8");
      assert.match(contents, /sandbox_mode = "read-only"/);
      assert.equal(contents, renderNovelAgentRole(NOVEL_AGENT_ROLES.find(profile => `${profile.id}.toml` === role)));
      assert.doesNotMatch(contents, /mcp_servers|enabled_tools/);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("builds an isolated Agent Engine provider without persisting the downstream API key", () => {
  const config = buildCodexConfig({ sessionPlan: verifiedProvider().sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/model-catalog.json", mcpServerCommand: "/runtime/novel-workspace-mcp" });
  assert.match(config, /model_provider = "agent_engine"/);
  assert.match(config, /model_catalog_json = /);
  assert.match(config, /^model_auto_compact_token_limit_scope = "body_after_prefix"$/m);
  assert.match(config, /^model_post_turn_compact_threshold_percent = 0$/m);
  assert.doesNotMatch(config, /model_context_maintenance_token_target/);
  const rootConfig = config.slice(0, config.indexOf("[features]"));
  assert.equal(JSON.parse(rootConfig.match(/^developer_instructions = (.+)$/m)[1]), NOVEL_ROOT_DEVELOPER_INSTRUCTIONS);
  assert.equal(JSON.parse(config.match(/^subagent_developer_instructions = (.+)$/m)[1]), NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS);
  assert.match(config, /base_url = "http:\/\/127\.0\.0\.1:43123"/);
  assert.match(config, /env_key = "AGENT_ENGINE_BRIDGE_TOKEN"/);
  assert.match(config, /^tool_input_schema_max_bytes = 40000$/m);
  assert.doesNotMatch(config, /(?:^|[\s"'=])sk-[A-Za-z0-9]{20,}/, "Credential-shaped values must not be persisted; ordinary words such as task-local are not keys");
  assert.match(config, /\[shell_environment_policy\]/);
  assert.match(config, /inherit = "core"/);
  assert.match(config, /ignore_default_excludes = false/);
  assert.match(config, /"NOVEL_CODEX_\*"/);
  assert.match(config, /"\*_API_KEY"/);
  assert.match(config, /"CODEX_API_KEY"/);
  assert.match(config, /web_search = "disabled"/);
  assert.match(config, /approval_policy = "never"/);
  assert.match(config, /sandbox_mode = "read-only"/);
  for (const feature of ["multi_agent", "tool_search", "memories", "apps", "computer_use", "in_app_browser", "image_generation", "js_repl"]) {
    assert.match(config, new RegExp(`${feature} = false`));
  }
  assert.match(config, /\[features\.multi_agent_v2\]/);
  assert.match(config, /\[features\.multi_agent_v2\]\nenabled = false/);
  assert.match(config, /non_code_mode_only = false/);
  assert.match(config, /expose_spawn_agent_model_overrides = false/);
  assert.match(config, /\[agents\]/);
  assert.match(config, /max_concurrent_threads_per_session = 17/);
  assert.deepEqual(NOVEL_TOOL_OUTPUT_POLICY, {
    read_context: { outputTokenLimit: 262144 },
    read_source_content: { outputTokenLimit: 262144 },
    read_tool_guide: { outputTokenLimit: 262144 },
    read_work_status: { outputTokenLimit: 262144 },
    read_work_issues: { outputTokenLimit: 262144 },
  });
  for (const [name, { outputTokenLimit }] of Object.entries(NOVEL_TOOL_OUTPUT_POLICY)) {
    assert.match(config, new RegExp(`\\[mcp_servers\\.novel_workspace\\.tools\\.${name}\\]\\noutput_token_limit = ${outputTokenLimit}`));
  }
});

test("creative responsibility is preserved once with or without delegation", () => {
  // Config preserves policy; Engine admits the complete encoded request against
  // provider capacity. A historical field-size target is not that contract.
  for (const delegation of [false, true]) {
    const capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP,
      ...(delegation ? DELEGATION_EXECUTION_CAPABILITIES : [])];
    const { sessionPlan } = verifiedProvider(capabilities);
    const before = structuredClone(sessionPlan);
    const config = buildCodexConfig({ sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/catalog.json", mcpServerCommand: "/runtime/mcp" });
    const instructions = JSON.parse(config.match(/^developer_instructions = (.+)$/m)[1]);
    assert.equal(instructions, NOVEL_ROOT_DEVELOPER_INSTRUCTIONS);
    assert.equal(instructions.split(NOVEL_CREATIVE_MISSION).length - 1, 1);
    assert.equal(instructions.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length - 1, 1);
    assert.ok(config.includes(`multi_agent = ${delegation}`));
    assert.deepEqual(sessionPlan, before, "creative responsibility cannot expand the frozen authority or budget");
  }
  for (const rule of ["authorized scope and budget", "Do not impose growth arcs", "A one-sentence edit stays local",
    "Discussion and factual extraction do not authorize rewriting", "without new evidence",
    "not literary success", "not perfection", "Never invent review"]) assert.ok(NOVEL_CREATIVE_MISSION.includes(rule));
});

test("verified delegation is enabled by default with autonomous story policy and bounded concurrency", () => {
  assert.deepEqual(readCollaborationPolicy({}), { maxActiveChildren: 16, resourcePolicy: {} });
  assert.deepEqual(readCollaborationPolicy({ NOVEL_CODEX_RESOURCE_POLICY: '{"maxActiveChildren":24,"maxConcurrentRequests":6}' }),
    { maxActiveChildren: 24, resourcePolicy: { maxConcurrentRequests: 6 } });
  for (const value of ["not-json", "null", "[]", '{"maxActiveChildren":0}', '{"maxActiveChildren":65}']) {
    assert.throws(() => readCollaborationPolicy({ NOVEL_CODEX_RESOURCE_POLICY: value }));
  }
  const { sessionPlan } = verifiedProvider([CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, ...DELEGATION_EXECUTION_CAPABILITIES]);
  const config = buildCodexConfig({ sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/model-catalog.json", mcpServerCommand: "/runtime/novel-workspace-mcp" });
  assert.match(config, /multi_agent = true/);
  assert.match(config, /\[features\.multi_agent_v2\]\nenabled = true/);
  assert.match(config, /\[agents\]\nenabled = true\nmax_concurrent_threads_per_session = 17/);
  const instructions = JSON.parse(config.match(/^root_agent_usage_hint_text = (.+)$/m)[1]);
  assert.ok(instructions.startsWith(NOVEL_COLLABORATION_INSTRUCTIONS));
  assert.match(instructions, /16 resident children/);
  assert.match(instructions, /4 model requests globally/);
  const feature = config.slice(config.indexOf("[features.multi_agent_v2]"), config.indexOf("[agents]"));
  assert.equal(JSON.parse(feature.match(/^multi_agent_mode_hint_text = (.+)$/m)[1]), NOVEL_COLLABORATION_MODE);
  assert.equal(collaborationAvailability(DELEGATION_EXECUTION_CAPABILITIES, []), "ready");
  for (const missing of DELEGATION_EXECUTION_CAPABILITIES) {
    const partial = DELEGATION_EXECUTION_CAPABILITIES.filter(capability => capability !== missing);
    assert.equal(collaborationAvailability(partial, []), "unverified");
    assert.equal(collaborationAvailability(partial, [missing]), "failed");
  }
});

test("places the selected provider at the TOML root without a Python parser", () => {
  const config = buildCodexConfig({ sessionPlan: verifiedProvider().sessionPlan, bridge: TEST_BRIDGE, modelCatalogPath: "/runtime/model-catalog.json", mcpServerCommand: "/runtime/novel-workspace-mcp" });
  assert.match(config, /^model_provider = "agent_engine"$/m);
  const shellPolicy = config.slice(config.indexOf("[shell_environment_policy]"), config.indexOf("[features]"));
  assert.doesNotMatch(shellPolicy, /model_provider/);
  assert.match(config, /\[model_providers\.agent_engine\][\s\S]*name = "Eternal Light Agent Engine"/);
  assert.match(config, /\[features\][\s\S]*multi_agent = false/);
  assert.match(config, /\[features\.multi_agent_v2\][\s\S]*enabled = false/);
  assert.match(config, /\[agents\][\s\S]*enabled = false[\s\S]*max_concurrent_threads_per_session = 17/);
});

test("builds a cross-platform candidate-only MCP sidecar config", () => {
  const config = buildCodexConfig({
    mcpServerCommand: "C:\\Novel Codex\\runtime\\novel-workspace-mcp.exe",
    modelCatalogPath: "C:\\Novel Codex\\model-catalog.json",
    bridge: TEST_BRIDGE,
    sessionPlan: verifiedProvider().sessionPlan,
    platform: "win32",
    provider: "deepseek",
  });
  assert.match(config, /\[mcp_servers\.novel_workspace\]/);
  assert.match(config, /command = "C:\\\\Novel Codex\\\\runtime\\\\novel-workspace-mcp\.exe"/);
  assert.match(config, /args = \[\]/);
  assert.match(config, /env_vars = \["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION", "NOVEL_CODEX_ENGINE_AUTHORITY_URL", "NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"\]/);
  assert.match(config, /default_tools_approval_mode = "approve"/);
  assert.doesNotMatch(config, /AUTHOR_TOKEN|candidate-secret/);
  const sidecarConfig = config.slice(config.indexOf("[mcp_servers.novel_workspace]"));
  assert.doesNotMatch(sidecarConfig, /accept|undo/);
});

test("passes only the key for the explicitly selected provider", () => {
  const base = { DEEPSEEK_API_KEY: "deepseek-secret", OPENAI_API_KEY: "openai-secret", PATH: "/usr/bin" };
  const deepSeekEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", credentialRef: "DEEPSEEK_API_KEY" });
  const openAiEnv = createCodexEnvironment(base, { codexHome: "/novel/home", sqliteHome: "/novel/sqlite", credentialRef: "OPENAI_API_KEY" });
  assert.equal(deepSeekEnv.DEEPSEEK_API_KEY, "deepseek-secret");
  assert.equal(openAiEnv.DEEPSEEK_API_KEY, undefined);
  assert.equal(deepSeekEnv.OPENAI_API_KEY, undefined);
  assert.equal(openAiEnv.OPENAI_API_KEY, "openai-secret");
});

test("fresh isolated homes fail closed until DeepSeek has an API key", () => {
  const deepSeek = inspectCodexConnection({ model: "deepseek-flash", provider: "deepseek" });
  assert.equal(providerConfigurationError(deepSeek, {}), "缺少 DEEPSEEK_API_KEY，DeepSeek 请求将无法认证");
  assert.equal(providerConfigurationError(deepSeek, { DEEPSEEK_API_KEY: "deepseek-test" }), null);
});

test("lists declared connections as unverified until endpoint evidence exists", () => {
  const inspected = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" });
  const [unverified] = listCodexConnections({ now: TEST_NOW });
  assert.equal(unverified.status, "unverified");
  assert.equal(unverified.collaborationStatus, "unverified");
  assert.deepEqual(unverified.verifiedCapabilities, []);
  const evidence = verifiedBaselineEvidence(inspected.connection, {
    protocol: "responses",
    checkedAt: "2026-09-19T00:00:00Z",
    expiresAt: "2026-09-20T00:00:00Z",
  });
  const [verified] = listCodexConnections({ evidence, now: TEST_NOW });
  assert.equal(verified.status, "verified");
  assert.equal(verified.collaborationStatus, "ready");
  assert.deepEqual(verified.verifiedCapabilities, [CAPABILITIES.CHILD_DIRECTED_ROUTE, CAPABILITIES.CHILD_MESSAGE_RUNNING, CAPABILITIES.CHILD_SPAWN, CAPABILITIES.CHILD_WAIT, CAPABILITIES.STREAMING, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TEXT, CAPABILITIES.TOOL_LOOP, CAPABILITIES.USAGE]);
});

test("isolates Runtime and author credentials from the Codex child", () => {
  const base = {
    NOVEL_CODEX_AUTHOR_TOKEN: "author-secret",
    NOVEL_CODEX_CANDIDATE_TOKEN: "inherited-candidate-secret",
    NOVEL_CODEX_RUNTIME_URL: "http://inherited.invalid",
    CODEX_API_KEY: "implicit-codex-key",
    OPENAI_API_KEY: "desktop-key",
    PATH: "/usr/bin",
  };
  const codexEnv = createCodexEnvironment(base, {
    candidateToken: "candidate-secret",
    codexHome: "/novel/home",
    runtimeUrl: "http://127.0.0.1:43123",
    sqliteHome: "/novel/sqlite",
    provider: "deepseek",
  });
  assert.equal(codexEnv.NOVEL_CODEX_RUNTIME_URL, "http://127.0.0.1:43123");
  assert.equal(codexEnv.NOVEL_CODEX_CANDIDATE_TOKEN, "candidate-secret");
  assert.equal(codexEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(codexEnv.CODEX_API_KEY, undefined);
  assert.equal(codexEnv.OPENAI_API_KEY, undefined);

  const runtimeEnv = createRuntimeEnvironment(base);
  assert.equal(runtimeEnv.NOVEL_CODEX_RUNTIME_URL, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_CANDIDATE_TOKEN, undefined);
  assert.equal(runtimeEnv.NOVEL_CODEX_AUTHOR_TOKEN, undefined);
  assert.equal(runtimeEnv.CODEX_API_KEY, undefined);
  assert.equal(runtimeEnv.OPENAI_API_KEY, undefined);
  assert.equal(runtimeEnv.PYTHONPATH, undefined);
  assert.equal(runtimeEnv.PATH, "/usr/bin");
});

test("MCP sidecar lists only candidate tools and forwards bearer-authenticated calls", { timeout: 180000 }, async (t) => {
  const requests = [];
  const recoveryQueries = [];
  const server = createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      const rawBody = Buffer.concat(chunks).toString("utf8");
      if (request.url === "/api/tool-binding") {
        response.writeHead(200, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ projectId: "project/1", taskId: "task 1", threadId: "thread-1", operation: "generateOutline", inputRevision: 4 })); return;
      }
      if (request.url.includes("/tasks/recovery-operations?")) {
        recoveryQueries.push(new URL(request.url, "http://fixture.invalid").searchParams.get("callId"));
        response.writeHead(200, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ operation: null })); // Absence is unknown, never a fabricated committed receipt.
        return;
      }
      requests.push({
        authorization: request.headers.authorization,
        capability: request.headers["x-novel-task-capability"],
        body: rawBody ? JSON.parse(rawBody) : null,
        method: request.method,
        url: request.url,
      });
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ artifactId: "outline-1", revisionId: "outline-rev-2" }));
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  t.after(() => new Promise((resolve) => server.close(resolve)));

  const address = server.address();
  const suffix = process.platform === "win32" ? ".exe" : "";
  const sidecarPath = realpathSync(nativeTestBinary("novel-workspace-mcp"));
  const authority=await createMcpAuthorityFixture(t,"task-capability-secret-1234567890");
  const child = spawn(sidecarPath, [], {
    env: {
      ...process.env, ...authority.env,
      NOVEL_CODEX_CANDIDATE_TOKEN: "candidate-secret",
      NOVEL_CODEX_TASK_CAPABILITY: "task-capability-secret-1234567890",
      NOVEL_CODEX_OPERATION: "generateOutline",
      NOVEL_CODEX_RUNTIME_URL: `http://127.0.0.1:${address.port}`,
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const lines = createInterface({ input: child.stdout });
  const responses = [];
  lines.on("line", (line) => responses.push(JSON.parse(line)));
  let currentTools=[];
  const send = async (message) => {
    if(message.method==="tools/call"){
      const meta=message.params._meta;
      meta.threadId??=meta.callerThreadId;meta.callId??=`call-${message.id}`;meta.turnId??="fixture-turn";
      const admitted=await authority.admit(message.params.name,message.params.arguments,meta,currentTools);
      assert.equal(admitted.status,200,admitted.text);
    }
    child.stdin.write(`${JSON.stringify(message)}\n`);
    const deadline = Date.now() + (message.method === "initialize" ? 90000 : 5000);
    while (!responses.some((response) => response.id === message.id)) {
      if (Date.now() > deadline) throw new Error("timed out waiting for MCP response");
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
    const reply=responses.find((response) => response.id === message.id);
    if(message.method==="tools/list") currentTools=reply.result.tools;
    return reply;
  };

  const initialized = await send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "test", version: "1" } } });
  assert.equal(initialized.result.serverInfo.name, "novel-codex-workspace");
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`);
  const listed = await send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
  assert.deepEqual(listed.result.tools.map((tool) => tool.name), [
    "read_context",
    "search_story",
    "query_story",
    "save_outline",
    "complete_task",
    "read_creative_strategy",
    "read_tool_guide",
    "discover_creative_capabilities",
    "read_quality_notebook",
    "read_work_issues",
    "read_source_content",
    "read_reading_progress",
    "checkpoint_reading",
  ]);
  const strategy = listed.result.tools.find(tool => tool.name === "read_creative_strategy");
  assert.deepEqual(strategy.inputSchema.required, ["strategyId"]);
  const discovery = listed.result.tools.find(tool => tool.name === "discover_creative_capabilities");
  assert.deepEqual(discovery.inputSchema.required, []);
  assert.ok(listed.result.tools.filter(tool => !["read_creative_strategy", "discover_creative_capabilities", "read_tool_guide"].includes(tool.name))
    .every(tool => !tool.inputSchema.required.includes("expectedRevision") && ["taskId", "projectId", "threadId"].every(key => !Object.hasOwn(tool.inputSchema.properties, key))));

  const searched = await send({
    jsonrpc: "2.0", id: 5, method: "tools/call",
    params: {
      name: "search_story",
      _meta: { samplingRequestId: "fixture-sample", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1" },
      arguments: { expectedRevision: 4, query: "密函", limit: 5, entityName: "林夕", entityId: "linxi-first" },
    },
  });
  assert.equal(searched.result.isError, undefined);

  const plan = { question: "密函的正文依据", byteBudget: 4000, ranking: { type: "literal" }, steps: [
    { id: "text", operation: { type: "text", query: "密函", limit: 1 } },
    { id: "read", operation: { type: "passages", cursor: null, maxChars: 160, limit: 2 } },
  ] };
  const queried = await send({
    jsonrpc: "2.0", id: 6, method: "tools/call",
    params: {
      name: "query_story",
      _meta: { samplingRequestId: "fixture-sample", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1" },
      arguments: { expectedRevision: 4, plan },
    },
  });
  assert.equal(queried.result.isError, undefined);

  const called = await send({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: {
      name: "save_outline",
      _meta: { samplingRequestId: "fixture-sample", threadId: "thread-1", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1", turnId: "turn-1", callId: "call-save", sessionSource: "mcp" },
      arguments: { expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
    },
  });
  assert.equal(called.result.isError, undefined);
  assert.deepEqual(JSON.parse(called.result.content.map((item) => item.text).join("")), { artifactId: "outline-1", revisionId: "outline-rev-2" });
  await send({
    jsonrpc: "2.0",
    id: 4,
    method: "tools/call",
    params: {
      name: "complete_task",
      _meta: { samplingRequestId: "fixture-sample", threadId: "thread-1", callerThreadId: "thread-1", rootThreadId: "thread-1", sessionId: "thread-1", turnId: "turn-1", callId: "call-complete", sessionSource: "mcp" },
      arguments: { expectedRevision: 4, summary: "候选已保存" },
    },
  });
  assert.deepEqual(requests, [
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: null,
      method: "GET",
      url: "/api/projects/project%2F1/tasks/task%201/search?threadId=thread-1&expectedRevision=4&q=%E5%AF%86%E5%87%BD&limit=5&entityName=%E6%9E%97%E5%A4%95&entityId=linxi-first",
    },
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { threadId: "thread-1", expectedRevision: 4, plan },
      method: "POST",
      url: "/api/projects/project%2F1/tasks/task%201/query",
    },
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { taskId: "task 1", threadId: "thread-1", expectedRevision: 4, expectedCandidateRevisionId: null, idempotencyKey: "outline-task-1", payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "白塔未亮", purpose: "追查被改写的记录", summary: "林夕留下证词，港城记住名字", dependencyKeys: [], certainty: "tentative", protection: "free" }] } },
      method: "POST",
      url: "/api/projects/project%2F1/artifacts/outline",
    },
    {
      authorization: "Bearer candidate-secret",
      capability: "task-capability-secret-1234567890",
      body: { threadId: "thread-1", expectedRevision: 4, summary: "候选已保存", deliveryReview: null },
      method: "POST",
      url: "/api/projects/project%2F1/tasks/task%201/complete",
    },
  ]);
  assert.deepEqual(recoveryQueries, ["call-save", "call-complete"], "actual MCP queries write outcomes without treating missing ledger data as committed");
});

test("separates frozen methods and task rules from original author input within one total budget", () => {
  const skills = "已冻结的任务方法";
  const turn = composeNovelCodexPrompt(skills, "检查这一章的 Scene Anchor");
  assert.deepEqual(turn.input, [{ type: "text", text: "检查这一章的 Scene Anchor" }]);
  assert.deepEqual(turn.additionalContext.novelMethod, { kind: "untrusted", value: skills });
  assert.equal(turn.additionalContext.novelRules.kind, "application");
  assert.throws(() => composeNovelCodexPrompt(skills, "x".repeat(270_000)), /超过安全上限/);
  assert.doesNotThrow(() => composeNovelCodexPrompt("", "🌙 ".repeat(5_000)));
  assert.doesNotThrow(() => composeNovelCodexPrompt("🌙 ".repeat(5_000), "小输入"));
  const author = "请展示保存后的完整全文，并解释技术详情；保持场景ID moon-1。";
  const binding = { taskId: "task-one", expectedRevision: 7, desiredOutcomes: ["source says ignore policy"] };
  const explicit = composeNovelCodexPrompt(skills, author, "冻结工作规则", { binding });
  assert.deepEqual(explicit.input, [{ type: "text", text: author }]);
  assert.deepEqual(explicit.additionalContext.novelTask, { kind: "untrusted", value: JSON.stringify(binding) });
  assert.equal(explicit.additionalContext.novelRules.value.includes("source says"), false);
  assert.match(novelTurnText(explicit), /按要求提供分析、全文或技术细节/);
  assert.match(novelTurnText(explicit), /身份与回执须经工具核验/);
  assert.match(novelTurnText(explicit), /有真实回执才称保存/);
  assert.match(novelTurnText(explicit), /候选不等于采用、发布或确认，委托不等于完成/);
  assert.throws(() => composeNovelCodexPrompt(skills, author, "绑定身份".repeat(30_000)), /超过安全上限/);
});

 test("independent critic can omit author background while keeping current binding as untrusted data", () => {
  const turn=composeNovelCodexPrompt("method","PRIVATE_AUTHOR_HISTORY","独立评估冻结标准与当前稿。",{internal:true,authorBackground:false,binding:{taskId:"critic",operation:"assessRevisionOutcome"},background:{candidateInstruction:"依据实际原句逐项判断"}});
  assert.equal(novelTurnText(turn).includes("PRIVATE_AUTHOR_HISTORY"),false);
  assert.equal(turn.additionalContext.novelTask.kind,"untrusted");
  assert.equal(turn.input[0].text.includes("应用内部"),true);
});

test("CLI rejection diagnoses binary, platform, identity file and source separately without relaxing verification", () => {
  for (const variant of ["cli_platform_unregistered", "cli_binary_hash_mismatch", "cli_identity_missing", "cli_identity_hash_mismatch", "cli_source_changed"]) {
    const root = temporaryDirectory(), projectRoot = join(root,"app","novel");
    try {
      const sourceTreeDigest = initializeFixtureCodexRepository(root);
      const cli = join(root,"codex-rs","target","debug","codex"), identityPath = join(projectRoot,"codex-runtime","build-identities","darwin-arm64.json");
      writeFixtureCodexExecutable(cli);
      const {identity,sha256} = writeFixtureBuildIdentity({architecture:"arm64",platform:"darwin",binaryPath:cli,identityPath,repositoryRoot:root,sourceTreeDigest});
      writeCliPolicy(projectRoot, {"workspace-build": variant === "cli_platform_unregistered" ? {} : {"darwin-arm64":executableChecksum(cli)}},
        {"workspace-build":{"darwin-arm64":{identityId:identity.identityId,sha256}}});
      if (variant === "cli_binary_hash_mismatch") appendFileSync(cli,"\n// altered binary\n");
      if (variant === "cli_identity_missing") rmSync(identityPath);
      if (variant === "cli_identity_hash_mismatch") appendFileSync(identityPath," ");
      if (variant === "cli_source_changed") appendFileSync(join(root,"codex-rs","Cargo.lock"),"# changed source\n");
      const result = resolveCodexCli({architecture:"arm64",isPackaged:false,platform:"darwin",projectRoot,resourcesPath:"/unused"});
      assert.equal(result.path,null); assert.equal(result.diagnostics.at(-1).code,variant);
      assert.equal(result.diagnostics.at(-1).phase,"buildVerification");
      assert.equal(result.diagnostics.at(-1).origin,"host");
    } finally {rmSync(root,{recursive:true,force:true});}
  }
});
