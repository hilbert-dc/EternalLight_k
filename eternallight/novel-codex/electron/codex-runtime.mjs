import { chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash, createHmac, randomBytes, randomUUID } from "node:crypto";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CAPABILITIES,
  CAPABILITY_PROFILES,
  DELEGATION_EXECUTION_CAPABILITIES,
  CapabilityEvidenceStore,
  CapabilityReportStore,
  CapabilityVerifier,
  TrustedEvidenceRepository,
  activeEvidence,
  compileCodexConfig,
  compileCodexModelCatalog,
  connectionDefinitionDigest,
  connectionEnvironmentDigest,
  contractHash,
  defineCapabilityReport,
  defineEvidence,
  isQualifiedEvidence,
  materializeCertificationEvidence,
  parseModelCertificationDocument,
  defineConnection,
  digest,
  getCodec,
  resolveProtocolSelections,
  resolveCapabilityProfile,
  resolveSessionPlan,
  stableJson,
  summarizeEvidence,
  nativeEnginePath,
} from "@eternallight/agent-engine";
import { modelInputTokenBudget } from "../shared/author-input-budget.mjs";
import { readBuildIdentity, sha256File, verifyBuildIdentity } from "../scripts/build-provenance.mjs";
import { OPERATION_REQUIREMENTS, PRODUCT_OPERATION_IDS, operationRequirement } from "./operation-requirements.mjs";
import { certificationPath } from "../scripts/certification-publication.mjs";
import { LANGUAGE_CONSISTENCY_INSTRUCTIONS } from "./author-language.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "./creative-mission.mjs";
import { NOVEL_COLLABORATION_INSTRUCTIONS, NOVEL_COLLABORATION_MODE, readCollaborationPolicy, collaborationAvailability, collaborationInstructions } from "./novel-collaboration.mjs";
import { NOVEL_AGENT_ROLES, renderNovelAgentRole } from "./novel-agent-roles.mjs";

export const NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS = `You are a read-only subagent for 小说工坊. Analyze only the assigned scope, ground findings in frozen story context, separate facts from inference, and return concise evidence to the parent. Never write or adopt story data.\n\n${LANGUAGE_CONSISTENCY_INSTRUCTIONS}`;

const MODEL_CATALOG_PATH = fileURLToPath(new URL("../codex-runtime/model-catalog.json", import.meta.url));
const CLI_POLICY_PATH = fileURLToPath(new URL("../codex-runtime/supported-cli.json", import.meta.url));
// Product framing budget, separate from the engine's token and byte admission caps.
const MAX_MODEL_MESSAGE_BYTES = 38_000;
const MAX_MODEL_MESSAGE_TOKENS = 9_000;
const MAX_MODEL_INPUT_BYTES = 9_500;
const NOVEL_MCP_ENVIRONMENT_KEYS = ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"];
const CHILD_ENVIRONMENT_KEYS = [
  "PATH",
  "PATHEXT",
  "SystemRoot",
  "WINDIR",
  "ComSpec",
  "TEMP",
  "TMP",
  "TMPDIR",
  "LANG",
  "LC_ALL",
  "SSL_CERT_FILE",
  "SSL_CERT_DIR",
  "HTTP_PROXY",
  "HTTPS_PROXY",
  "NO_PROXY",
];

function isolatedProcessEnvironment(baseEnvironment) {
  const environment = {};
  for (const key of CHILD_ENVIRONMENT_KEYS) {
    if (typeof baseEnvironment[key] === "string" && baseEnvironment[key]) environment[key] = baseEnvironment[key];
  }
  for (const [key, value] of Object.entries(baseEnvironment)) {
    if (/^LC_[A-Z0-9_]+$/.test(key) && typeof value === "string") environment[key] = value;
  }
  return environment;
}

export function createTaskCapability(now = Date.now()) {
  const secret = randomBytes(32).toString("base64url");
  return {
    secret,
    hash: createHash("sha256").update(secret, "utf8").digest("hex"),
    expiresAt: Math.floor(now / 1000) + 30 * 60,
  };
}

function canonicalPath(candidate) {
  try {
    return realpathSync(candidate);
  } catch {
    return resolve(candidate);
  }
}

function isChatGptDesktopBinary(candidate, platform) {
  const normalized = canonicalPath(candidate).replaceAll("\\", "/").toLowerCase();
  if (platform === "darwin") return normalized.includes("/applications/chatgpt.app/");
  return normalized.includes("/chatgpt.app/") || normalized.includes("/chatgpt/");
}

function isExecutableFile(candidate) {
  try {
    return statSync(candidate).isFile();
  } catch {
    return false;
  }
}

function readJsonManifest(path, label, versions = [1]) {
  try {
    const manifest = JSON.parse(readFileSync(path, "utf8"));
    if (!versions.includes(manifest?.version)) throw new Error("unsupported version");
    return manifest;
  } catch {
    throw new Error(`${label} 缺失或无效`);
  }
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const CLI_POLICY = readJsonManifest(CLI_POLICY_PATH, "CLI 支持清单");
const MODEL_CERTIFICATIONS_PATH = certificationPath(dirname(CLI_POLICY_PATH), CLI_POLICY);
if (CLI_POLICY.modelCatalogSha256 !== sha256(MODEL_CATALOG_PATH)) {
  throw new Error("运行时模型目录与固定指纹不一致");
}
if (CLI_POLICY.modelCertificationsSha256 !== sha256(MODEL_CERTIFICATIONS_PATH)) {
  throw new Error("运行时模型认证与固定指纹不一致");
}
const MODEL_CATALOG = readJsonManifest(MODEL_CATALOG_PATH, "模型目录");
const MODEL_CERTIFICATIONS = parseModelCertificationDocument(readJsonManifest(MODEL_CERTIFICATIONS_PATH, "模型认证", [2]));

function sourceTreeDigest(directory) {
  const files = [];
  const visit = (current) => {
    for (const entry of readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile()) files.push(`${path.slice(directory.length + 1).replaceAll("\\", "/")}:${sha256(path)}`);
    }
  };
  visit(directory);
  return createHash("sha256").update(files.join("\n"), "utf8").digest("hex");
}

export function createQualificationContextDigest({ baseInstructions, cliPath,
  developerInstructions = NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
  subagentDeveloperInstructions = NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS }) {
  if (typeof baseInstructions !== "string" || !baseInstructions.trim()) throw new Error("资格上下文缺少创作引擎基础指令");
  if (typeof developerInstructions !== "string" || !developerInstructions.trim()
    || typeof subagentDeveloperInstructions !== "string" || !subagentDeveloperInstructions.trim()) throw new Error("资格上下文缺少当前产品开发者指令");
  if (!isExecutableFile(cliPath)) throw new Error("资格上下文缺少可验证的内置创作引擎");
  const agentEngineEntry = fileURLToPath(import.meta.resolve("@eternallight/agent-engine"));
  return digest({
    agentEngineSource: sourceTreeDigest(dirname(agentEngineEntry)),
    agentEngineQualification: sourceTreeDigest(join(dirname(agentEngineEntry), "..", "qualification")),
    nativeEngine: sha256File(nativeEnginePath),
    baseInstructions: createHash("sha256").update(baseInstructions, "utf8").digest("hex"),
    developerInstructions: createHash("sha256").update(developerInstructions, "utf8").digest("hex"),
    subagentDeveloperInstructions: createHash("sha256").update(subagentDeveloperInstructions, "utf8").digest("hex"),
    creativeStrategies: sha256(fileURLToPath(new URL("../shared/creative-strategies.json", import.meta.url))),
    collaborationInstructions: digest({ implementation: sha256(fileURLToPath(new URL("./novel-collaboration.mjs", import.meta.url))), roles: sha256(fileURLToPath(new URL("./novel-agent-roles.mjs", import.meta.url))), instructions: NOVEL_COLLABORATION_INSTRUCTIONS, mode: NOVEL_COLLABORATION_MODE, resources: readCollaborationPolicy() }),
    cli: sha256File(cliPath),
    conformanceMcp: sha256(fileURLToPath(new URL("./conformance-mcp.mjs", import.meta.url))),
    conformanceRunner: sha256(fileURLToPath(new URL("./codex-conformance-runner.mjs", import.meta.url))),
    coreConformanceMcp: sha256(fileURLToPath(new URL("./core-conformance-mcp.mjs", import.meta.url))),
    coreIntegrationRunner: sha256(fileURLToPath(new URL("./codex-core-integration-runner.mjs", import.meta.url))),
    engineContract: contractHash(),
  });
}

function findConnectionEntry({ model, provider }) {
  const requestedProvider = provider?.trim().toLowerCase() || null;
  const requestedModel = model?.trim() || null;
  const entry = MODEL_CATALOG.connections.find((candidate) => {
    if (requestedProvider && candidate.provider.toLowerCase() !== requestedProvider && candidate.id !== requestedProvider) return false;
    if (requestedModel && candidate.model !== requestedModel && candidate.id !== requestedModel) return false;
    return requestedProvider || requestedModel ? true : candidate.id === MODEL_CATALOG.defaultConnectionId;
  });
  if (!entry) throw new Error(`未知模型连接：${requestedProvider ?? "<default>"}/${requestedModel ?? "<default>"}`);
  return entry;
}

export function inspectCodexConnection({ model, provider }) {
  const entry = findConnectionEntry({ model, provider });
  const connection = defineConnection(entry);
  return {
    id: connection.id,
    provider: connection.provider,
    model: connection.model,
    endpoint: connection.endpoint,
    protocol: connection.protocol,
    credentialRef: connection.credentialRef,
    capabilitySupport: { ...connection.capabilitySupport },
    declaredCapabilities: Object.entries(connection.capabilitySupport).filter(([, mode]) => ["native", "lossless-adapted"].includes(mode)).map(([capability]) => capability).sort(),
    operations: [...PRODUCT_OPERATION_IDS],
    operationRequirements: Object.freeze(Object.fromEntries(PRODUCT_OPERATION_IDS.map((operation) => [operation, operationRequirement(operation)]))),
    connection,
  };
}

function publishedCertifications(connection, qualificationContextDigest, now = new Date()) {
  return MODEL_CERTIFICATIONS.certifications.filter((candidate) => candidate.connectionId === connection.id)
    .filter((certification) => certification.qualificationContextDigest === qualificationContextDigest && Date.parse(certification.expiresAt) > now.getTime())
    .map((certification) => {
      if (certification.connectionDigest !== connectionDefinitionDigest(connection)) throw new Error(`模型认证与连接定义不匹配：${connection.id}`);
      if (certification.protocol !== connection.protocol || certification.codecVersion !== getCodec(connection.protocol).version) throw new Error(`模型认证与协议适配器不匹配：${connection.id}`);
      if (certification.engineContractHash !== contractHash()) throw new Error(`模型认证与引擎契约不匹配：${connection.id}`);
      return certification;
    });
}

export function publishedCertificationEvidence({ credentialScopes = {}, now = new Date(), qualificationContextDigest = "default" } = {}) {
  return MODEL_CATALOG.connections.flatMap((entry) => {
    const connection = defineConnection(entry);
    return publishedCertifications(connection, qualificationContextDigest, now).flatMap((certification) => materializeCertificationEvidence(certification, {
      connection,
      credentialScope: credentialScopes[connection.id] ?? connection.credentialRef,
      now,
      qualificationContextDigest,
    }));
  });
}

function combinedRuntimeEvidence({ credentialScopes, evidence, now, qualificationContextDigest }) {
  return [...publishedCertificationEvidence({ credentialScopes, now, qualificationContextDigest }), ...evidence];
}

function cliVersion(path) {
  const result = spawnSync(path, ["--version"], { encoding: "utf8", timeout: 15_000, windowsHide: true });
  if (result.error || result.status !== 0) return null;
  const version = result.stdout.trim();
  return version && !/[\r\n]/.test(version) ? version : null;
}

function cliMatchesPolicy(path, sourcePolicy, platform, architecture, identityPath, repositoryRoot = null) {
  const expectedDigest = sourcePolicy.digests?.[`${platform}-${architecture}`];
  if (!/^[a-f0-9]{64}$/.test(expectedDigest ?? "") || sha256(path) !== expectedDigest) return false;
  const expectedIdentity = sourcePolicy.buildIdentities?.[`${platform}-${architecture}`];
  if (!expectedIdentity || sha256File(identityPath) !== expectedIdentity.sha256) return false;
  const identity = readBuildIdentity(identityPath);
  if (identity.identityId !== expectedIdentity.identityId) return false;
  if (sourcePolicy.packaged) {
    if (identity.binary?.sha256 !== expectedIdentity.unsignedBinarySha256) return false;
  } else {
    verifyBuildIdentity(identity, path, { repositoryRoot });
  }
  const version = cliVersion(path);
  return version !== null && Array.isArray(sourcePolicy.versions) && sourcePolicy.versions.includes(version);
}

export function codexExecutableName(platform = process.platform) {
  return platform === "win32" ? "codex.exe" : "codex";
}

export function resolveRuntimeBinaries({ architecture = process.arch, platform = process.platform, resourcesPath }) {
  const policy = readJsonManifest(join(resourcesPath, "codex-runtime", "supported-cli.json"), "发布运行时支持清单");
  const release = readJsonManifest(join(resourcesPath, "release-manifest.json"), "发布来源清单");
  if (release.target?.platform !== platform || release.target?.architecture !== architecture) throw new Error("发布来源清单与当前平台不匹配");
  const expected = policy.runtimeBinaries?.[`${platform}-${architecture}`];
  const creativeRuntime = join(resourcesPath, "runtime", "bin", platform === "win32" ? "creative-runtime.exe" : "creative-runtime");
  const novelWorkspaceMcp = join(resourcesPath, "runtime", "bin", platform === "win32" ? "novel-workspace-mcp.exe" : "novel-workspace-mcp");
  for (const [label, path, checksum] of [
    ["Creative Runtime", creativeRuntime, expected?.creativeRuntime],
    ["Novel Workspace MCP", novelWorkspaceMcp, expected?.novelWorkspaceMcp],
  ]) {
    if (!isExecutableFile(path) || !/^[a-f0-9]{64}$/.test(checksum ?? "") || sha256(path) !== checksum) throw new Error(`${label} 发布文件缺失或校验失败`);
  }
  if (release.resources?.creativeRuntimeSha256 !== expected.creativeRuntime
    || release.resources?.novelWorkspaceMcpSha256 !== expected.novelWorkspaceMcp) {
    throw new Error("发布来源清单与 Runtime 摘要不匹配");
  }
  return Object.freeze({ creativeRuntime: canonicalPath(creativeRuntime), novelWorkspaceMcp: canonicalPath(novelWorkspaceMcp) });
}

export function createCodexEnvironment(baseEnvironment, {
  bridgeCredential = null,
  candidateToken,
  codexHome,
  credentialRef = null,
  runtimeUrl,
  sqliteHome,
}) {
  const environment = isolatedProcessEnvironment(baseEnvironment);
  if (typeof credentialRef === "string" && credentialRef && typeof baseEnvironment[credentialRef] === "string" && baseEnvironment[credentialRef].trim()) {
    environment[credentialRef] = baseEnvironment[credentialRef].trim();
  }
  if (bridgeCredential?.ref && bridgeCredential?.value) environment[bridgeCredential.ref] = bridgeCredential.value;
  if (typeof runtimeUrl === "string" && runtimeUrl.trim()) environment.NOVEL_CODEX_RUNTIME_URL = runtimeUrl.trim();
  if (typeof candidateToken === "string" && candidateToken) environment.NOVEL_CODEX_CANDIDATE_TOKEN = candidateToken;
  return {
    ...environment,
    CODEX_HOME: codexHome,
    CODEX_SQLITE_HOME: sqliteHome,
  };
}

export function providerConfigurationError(providerConfig, baseEnvironment) {
  const apiKey = baseEnvironment[providerConfig.credentialRef];
  return typeof apiKey === "string" && apiKey.trim()
    ? null
    : `缺少 ${providerConfig.credentialRef}，${providerConfig.provider} 请求将无法认证`;
}

export function resolveCodexProvider({ credentialScopes = {}, evidence = [], model, now = new Date(), operation = null, provider, qualificationContextDigest = "default", minimumEvidenceValidityMs = 0 }) {
  if (!Number.isSafeInteger(minimumEvidenceValidityMs) || minimumEvidenceValidityMs < 0) throw new Error("模型证据有效期要求无效");
  const inspected = inspectCodexConnection({ model, provider });
  const entry = findConnectionEntry({ model, provider });
  const qualifiedEvidence = combinedRuntimeEvidence({ credentialScopes, evidence, now, qualificationContextDigest })
    .filter(record => record.status !== "verified" || Date.parse(record.expiresAt) > now.getTime() + minimumEvidenceValidityMs);
  if (operation !== null && !inspected.operations.includes(operation)) {
    throw new Error(`${inspected.id} 未获准执行 ${operation}`);
  }
  const requirement = operationRequirement(operation);
  const operationProfile = resolveCapabilityProfile(requirement.capabilityProfileId);
  const connections = [];
  for (const entry of MODEL_CATALOG.connections) {
    const connection = defineConnection(entry);
    try {
      resolveSessionPlan({
        connections: [connection],
        evidence: qualifiedEvidence,
        now,
        policy: { capabilityProfileId: "base-tools-v1", credentialScopes, defaultConnectionId: connection.id, allowedChildConnectionIds: [connection.id], forceBridge: true, qualificationContextDigest, requiredCapabilities: [], productExtensions: DELEGATION_EXECUTION_CAPABILITIES, allowedMcpServerIds: ["novel_workspace"] },
      });
      connections.push(connection);
    } catch (error) {
      if (connection.id === inspected.id) throw error;
    }
  }
  const sessionPlan = resolveSessionPlan({
    connections,
    evidence: qualifiedEvidence,
    now,
    policy: {
      credentialScopes,
      capabilityProfileId: "base-tools-v1",
      defaultConnectionId: inspected.id,
      allowedChildConnectionIds: connections.map((connection) => connection.id),
      forceBridge: true,
      ...readCollaborationPolicy(),
      qualificationContextDigest,
      requiredCapabilities: [],
      productExtensions: DELEGATION_EXECUTION_CAPABILITIES,
      allowedMcpServerIds: ["novel_workspace"],
    },
  });
  if (!sessionPlan.authorizedCapabilityProfiles.includes(operationProfile.id)) {
    throw new Error(`${inspected.id} 缺少 ${operation ?? "默认"} 所需能力画像：${operationProfile.id}`);
  }
  return { ...inspected, connections, sessionPlan, operationProfileId: operationProfile.id, evidence: qualifiedEvidence };
}

export function assertProviderOperation(providerConfig, operation) {
  if (!providerConfig?.operations?.includes(operation)) throw new Error(`${providerConfig?.id ?? "当前连接"} 未获准执行 ${operation}`);
  return providerConfig;
}

export function listCodexConnections({ credentialScopes = {}, evidence = [], now = new Date(), qualificationContextDigest = "default" } = {}) {
  const qualifiedEvidence = combinedRuntimeEvidence({ credentialScopes, evidence, now, qualificationContextDigest });
  return MODEL_CATALOG.connections.map((entry) => {
    const connection = defineConnection(entry);
    const selectedProtocol = connection.protocol === "auto"
      ? resolveProtocolSelections([connection], qualifiedEvidence, { credentialScopes, now, qualificationContextDigest })[connection.id] ?? null
      : connection.protocol;
    let summary = new Map();
    if (selectedProtocol) {
      const environmentDigest = connectionEnvironmentDigest(connection, { codecVersion: getCodec(selectedProtocol).version, credentialScope: credentialScopes[connection.id] ?? connection.credentialRef, engineContractHash: contractHash(), protocol: selectedProtocol, qualificationContextDigest });
      summary = summarizeEvidence(activeEvidence(qualifiedEvidence, { environmentDigest, now }));
    }
    const verifiedCapabilities = [...summary].filter(([, record]) => isQualifiedEvidence(record)).map(([capability]) => capability).sort();
    const failedCapabilities = [...summary].filter(([, record]) => record.status === "failed").map(([capability]) => capability).sort();
    const required = operationRequirement(null).requiredCapabilities;
    const status = required.some((capability) => failedCapabilities.includes(capability))
      ? "failed"
      : required.every((capability) => verifiedCapabilities.includes(capability)) ? "verified" : "unverified";
    const credentialScope = credentialScopes[connection.id] ?? `missing:${connection.credentialRef}`;
    const verifiedProfiles = Object.values(CAPABILITY_PROFILES)
      .filter((profile) => profile.requiredCapabilities.every((capability) => verifiedCapabilities.includes(capability)))
      .map((profile) => profile.id)
      .sort();
    return {
      id: entry.id,
      provider: entry.provider,
      model: entry.model,
      protocol: entry.protocol,
      selectedProtocol,
      declaredCapabilities: Object.entries(connection.capabilitySupport).filter(([, mode]) => ["native", "lossless-adapted"].includes(mode)).map(([capability]) => capability).sort(),
      capabilitySupport: { ...connection.capabilitySupport },
      verifiedCapabilities,
      failedCapabilities,
      collaborationStatus: collaborationAvailability(verifiedCapabilities, failedCapabilities),
      operations: [...PRODUCT_OPERATION_IDS],
      operationRequirements: Object.fromEntries(PRODUCT_OPERATION_IDS.map((operation) => [operation, OPERATION_REQUIREMENTS[operation]])),
      verifiedProfiles,
      status,
      certificationIds: publishedCertifications(connection, qualificationContextDigest, now).map((certification) => certification.certificationId),
      accountStatus: credentialScope.startsWith("missing:") ? "missing-credential" : "configured",
      isDefault: entry.id === MODEL_CATALOG.defaultConnectionId,
    };
  });
}

export function buildCodexConfig({
  bridge = null,
  modelCatalogPath,
  mcpServerCommand,
  mcpServerPath = null,
  mcpServerArgs = null,
  model,
  platform = process.platform,
  provider,
  sessionPlan = null,
}) {
  if (!sessionPlan) throw new Error("生成创作引擎配置前必须提供已验证的 SessionPlan");
  const resolvedPlan = sessionPlan;
  const mcpCommand = mcpServerCommand?.trim();
  if (!mcpCommand) throw new Error("Novel Workspace MCP Rust 可执行文件未配置");
  return compileCodexConfig({
    sessionPlan: resolvedPlan,
    modelCatalogPath,
    bridge,
    developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
    usageHintText: collaborationInstructions(resolvedPlan),
    multiAgentModeHintText: NOVEL_COLLABORATION_MODE,
    subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
    sensitiveEnvironmentPatterns: ["NOVEL_CODEX_*", "*_API_KEY", "*_ACCESS_TOKEN", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"],
    mcpServers: [{
      id: "novel_workspace",
      command: mcpCommand,
      args: mcpServerArgs ?? (mcpServerPath ? [mcpServerPath] : []),
      envVars: NOVEL_MCP_ENVIRONMENT_KEYS,
      approvalMode: "approve",
      required: true,
      startupTimeoutMs: 90_000,
    }],
  });
}

export function buildCodexModelCatalog({ baseInstructions, model, provider, sessionPlan = null }) {
  if (!sessionPlan) throw new Error("生成模型目录前必须提供已验证的 SessionPlan");
  return compileCodexModelCatalog(sessionPlan, { baseInstructions });
}

export function createRuntimeEnvironment(baseEnvironment) {
  return isolatedProcessEnvironment(baseEnvironment);
}

export function composeNovelCodexPrompt(skillText, prompt, taskInstructions = "") {
  const normalizedPrompt = prompt.trim();
  const replyInstructions = "作者可见表达：遵守产品对话语言，用‘读取本章正文’等作者用语说明正在处理的内容。纯讨论直接回答。委托登记后只简述已安排、结果尚待交付；有成功保存回执才说明实际保存了什么，候选保存不等于采用、发布或设定确认。失败、未保存、覆盖不足或尚未分析如实说明。仅目标歧义、缺授权或正式决定时询问，不重复询问已授权事项，不主动邀请核对或继续。默认不重抄全文、不展示内部ID、字段、状态码、任务绑定或冻结上下文；作者明确要求详细分析、全文或技术细节时按权限充分回答。正文与结构化产物仍通过工具完整保存，身份、JSON和回执完整传入并核验，不以简短回复代替产物或正式决定。";
  const message = `${skillText.trim() ? `小说工坊专用工作约束（只对当前应用生效）：\n${skillText}\n\n` : ""}${taskInstructions ? `${taskInstructions}\n\n` : ""}${replyInstructions}\n\n${LANGUAGE_CONSISTENCY_INSTRUCTIONS}\n\n用户当前指令：\n${normalizedPrompt}`;
  const bytes = Buffer.byteLength(message, "utf8");
  if (bytes > MAX_MODEL_MESSAGE_BYTES) {
    throw new Error(`包含工作约束的本轮消息超过安全上限：${bytes}/${MAX_MODEL_MESSAGE_BYTES} UTF-8 字节。请缩短本轮创作意图；作者要求未被截断或发送。`);
  }
  const tokens = modelInputTokenBudget(message);
  if (tokens > MAX_MODEL_MESSAGE_TOKENS) throw new Error(`包含工作约束的本轮消息超过安全上限：文本预算 ${tokens}/${MAX_MODEL_MESSAGE_TOKENS}。请缩短本轮创作意图；作者要求未被截断或发送。`);
  // Split only the wire content, never the turn. The complete message was checked
  // above: splitting does not evade its aggregate token budget.
  const input = [];
  let text = "";
  let chunkBytes = 0;
  for (const character of message) {
    const size = Buffer.byteLength(character, "utf8");
    if (chunkBytes + size > MAX_MODEL_INPUT_BYTES) {
      input.push({ type: "text", text });
      text = "";
      chunkBytes = 0;
    }
    text += character;
    chunkBytes += size;
  }
  if (text) input.push({ type: "text", text });
  return input;
}

export function prepareIsolatedStorage(userDataPath) {
  const workspaceHome = join(userDataPath, "workspace");
  const codexHome = join(userDataPath, "codex-home");
  const sqliteHome = join(codexHome, "sqlite");
  const runtimeData = join(userDataPath, "creative-runtime");
  const runtimeDb = join(runtimeData, "novel-codex-v2.sqlite3");
  const engineData = join(userDataPath, "agent-engine");
  const evidencePath = join(engineData, "capability-evidence.json");
  const recoveryPath = join(engineData, "recovery-index.json");
  const budgetDb = join(engineData, "engine-state.sqlite3");
  const credentialSaltPath = join(engineData, "credential-scope.key");
  for (const directory of [workspaceHome, codexHome, sqliteHome, runtimeData, engineData]) mkdirSync(directory, { recursive: true });
  if (!existsSync(credentialSaltPath)) {
    try {
      writeFileSync(credentialSaltPath, `${randomBytes(32).toString("base64url")}\n`, { encoding: "utf8", flag: "wx", mode: 0o600 });
    } catch (error) {
      if (error?.code !== "EEXIST") throw error;
    }
  }
  chmodSync(credentialSaltPath, 0o600);
  return { workspaceHome, codexHome, sqliteHome, runtimeData, runtimeDb, engineData, evidencePath, recoveryPath, budgetDb, credentialSaltPath };
}

export function credentialScopesFromEnvironment(baseEnvironment, credentialSaltPath) {
  const salt = readFileSync(credentialSaltPath, "utf8").trim();
  if (!/^[A-Za-z0-9_-]{43}$/.test(salt)) throw new Error("凭据作用域密钥缺失或无效");
  return Object.freeze(Object.fromEntries(MODEL_CATALOG.connections.map((entry) => {
    const credential = baseEnvironment[entry.credentialRef];
    const scope = typeof credential === "string" && credential.trim()
      ? `credential-v1:${createHmac("sha256", salt).update(`${entry.id}\0${credential.trim()}`, "utf8").digest("hex")}`
      : `missing:${entry.credentialRef}`;
    return [entry.id, scope];
  })));
}

export async function loadCodexEvidence(path) {
  return (await loadCodexEvidenceSnapshot(path)).records;
}

export async function loadCodexEvidenceSnapshot(path) {
  const records = await new CapabilityEvidenceStore(path).read();
  const reports = new CapabilityReportStore(join(dirname(path), "capability-reports"));
  const repository = await TrustedEvidenceRepository.load({ records, reportStore: reports });
  return repository.resolveSnapshot();
}

export async function recordRuntimeQualificationFailure({ baselineEvidence = [], evidencePath, environmentDigest, protocol, statusCode }) {
  const store = new CapabilityEvidenceStore(evidencePath);
  const existing = await store.read();
  const now = new Date();
  const active = [...baselineEvidence, ...existing].filter((record) => record.environmentDigest === environmentDigest
    && record.scope?.protocol === protocol
    && record.status === "verified"
    && Date.parse(record.expiresAt) > now.getTime());
  if (active.length === 0) return { recorded: false };
  const failed = active.map((record) => defineEvidence({
    ...record,
    status: "failed",
    source: "probe",
    evidenceRef: `runtime-auth:${randomUUID()}:${record.capability}`,
    checkedAt: now.toISOString(),
    expiresAt: new Date(now.getTime() + 60 * 60 * 1000).toISOString(),
    sample: { category: "authentication", statusCode },
    reportRef: null,
    reportDigest: null,
    supersedes: [],
  }));
  await store.append(failed);
  return { recorded: true, records: failed.length };
}

export async function checkCodexConnectionAccess({ apiKey, connection, fetchImpl = globalThis.fetch }) {
  if (typeof fetchImpl !== "function") throw new Error("当前环境不支持模型连接检查");
  const codec = getCodec(connection.protocol);
  const prepared = codec.createRequest({
    model: connection.id,
    instructions: "Return a short acknowledgement.",
    input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "connection-check" }] }],
    tools: [],
    max_output_tokens: Math.min(16, connection.declaredLimits.outputTokens),
    stream: false,
  }, connection, apiKey);
  const controller = new AbortController();
  let timer;
  const expired = new Promise((_, reject) => { timer = setTimeout(() => {
    const error = new Error("模型连接验证超过 55 秒，请检查网络后重试");
    controller.abort(error); reject(error);
  }, 55_000); });
  try { return await Promise.race([expired, (async () => {
  const response = await fetchImpl(prepared.url, { method: "POST", headers: prepared.headers, body: JSON.stringify(prepared.body), signal: controller.signal });
  if (!response.ok) {
    const error = new Error(`模型账号检查失败：HTTP ${response.status}`);
    error.statusCode = response.status;
    throw error;
  }
  let payload;
  try {
    payload = await response.json();
  } catch {
    if (controller.signal.aborted) throw controller.signal.reason;
    throw new Error("模型账号检查返回了无效 JSON");
  }
  codec.parseResponse(payload, prepared.context);
  return { status: "available", protocol: connection.protocol };
  })()]); } finally { clearTimeout(timer); }
}

function redactVerificationValue(value, secret) {
  if (typeof value === "string") return secret ? value.replaceAll(secret, "[REDACTED]") : value;
  if (Array.isArray(value)) return value.map((item) => redactVerificationValue(item, secret));
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, redactVerificationValue(item, secret)]));
}

const EVIDENCE_SOURCE_PRIORITY = Object.freeze({ metadata: 0, probe: 1, "conformance-suite": 2 });

function supersededEvidence(existing, record) {
  return existing
    .filter((candidate) => candidate.environmentDigest === record.environmentDigest
      && candidate.capability === record.capability
      && stableJson(candidate.scope) === stableJson(record.scope)
      && EVIDENCE_SOURCE_PRIORITY[candidate.source] <= EVIDENCE_SOURCE_PRIORITY[record.source]
      && Date.parse(candidate.checkedAt) <= Date.parse(record.checkedAt))
    .map((candidate) => candidate.evidenceRef);
}

export async function verifyCodexConnection({ baseEnvironment, conformanceRunner = null, coreRunner = null, credentialScopes = {}, evidencePath, fetchImpl = globalThis.fetch, model, provider, qualificationContextDigest = "default", verifier = new CapabilityVerifier() }) {
  const entry = findConnectionEntry({ model, provider });
  const connection = defineConnection(entry);
  const apiKey = baseEnvironment[connection.credentialRef];
  if (typeof apiKey !== "string" || !apiKey.trim()) throw new Error(`缺少 ${connection.credentialRef}，无法验证连接`);
  const credentialScope = credentialScopes[connection.id] ?? connection.credentialRef;
  const store = new CapabilityEvidenceStore(evidencePath);
  const reportStore = new CapabilityReportStore(join(dirname(evidencePath), "capability-reports"));
  const startedAt = new Date();
  const certifications = publishedCertifications(connection, qualificationContextDigest, startedAt);
  if (certifications.length) {
    const baselineEvidence = certifications.flatMap((certification) => materializeCertificationEvidence(certification, { connection, credentialScope, now: startedAt, qualificationContextDigest }));
    try {
      const access = await checkCodexConnectionAccess({ apiKey: apiKey.trim(), connection, fetchImpl });
      return {
        certificationIds: certifications.map((certification) => certification.certificationId),
        reportId: certifications[0].reportDigest,
        selectedProtocol: connection.protocol,
        attempts: [{ protocol: connection.protocol, suite: "account-access-v1", status: "verified" }],
        records: [],
        access,
      };
    } catch (error) {
      if ([401, 403].includes(error?.statusCode)) {
        await recordRuntimeQualificationFailure({ baselineEvidence, evidencePath, environmentDigest: baselineEvidence[0].environmentDigest, protocol: connection.protocol, statusCode: error.statusCode });
      }
      throw error;
    }
  }
  try {
    const probe = await verifier.verify(connection, { apiKey: apiKey.trim(), credentialScope, now: startedAt, qualificationContextDigest });
    let unredactedResult = probe;
    let corePassed = true;
    if (coreRunner) {
      const core = await coreRunner.verify({
        apiKey: apiKey.trim(),
        connection,
        credentialScope,
        now: startedAt,
        probeEvidence: probe.records,
        protocol: probe.selectedProtocol,
        qualificationContextDigest,
      });
      const coreAttempt = { protocol: probe.selectedProtocol, suite: core.suiteId, status: core.status === "passed" ? "verified" : "failed", ...(core.blockingFailures?.length ? { message: core.blockingFailures.join(", ") } : {}) };
      corePassed = core.status === "passed";
      unredactedResult = { ...probe, attempts: [...probe.attempts, coreAttempt], records: [...probe.records, ...(core.record ? [core.record] : [])] };
    }
    if (conformanceRunner && corePassed) {
      const conformance = await conformanceRunner.verify({
        apiKey: apiKey.trim(),
        connection,
        credentialScope,
        now: startedAt,
        probeEvidence: probe.records,
        protocol: probe.selectedProtocol,
        qualificationContextDigest,
      });
      unredactedResult = {
        ...probe,
        attempts: [...unredactedResult.attempts, conformance.attempt],
        records: [...unredactedResult.records, ...conformance.records],
      };
    }
    const result = redactVerificationValue(unredactedResult, apiKey.trim());
    const existing = await store.read();
    const records = result.records.map((record) => ({
      ...record,
      supersedes: supersededEvidence(existing, record),
    }));
    const report = defineCapabilityReport({ connection, result: { ...result, records }, redactions: [apiKey.trim()], startedAt, completedAt: new Date() });
    await reportStore.write(report);
    const boundRecords = records.map((record) => ({ ...record, reportRef: report.reportId, reportDigest: report.reportId }));
    await store.append(boundRecords);
    return { reportId: report.reportId, selectedProtocol: result.selectedProtocol, attempts: result.attempts, records: boundRecords.map(({ capability, status, scope, checkedAt, expiresAt }) => ({ capability, status, scope, checkedAt, expiresAt })) };
  } catch (error) {
    const failure = error instanceof Error ? error : new Error(String(error));
    failure.message = redactVerificationValue(failure.message, apiKey.trim());
    if (Array.isArray(failure.attempts)) failure.attempts = redactVerificationValue(failure.attempts, apiKey.trim());
    if (Array.isArray(failure.records)) failure.records = redactVerificationValue(failure.records, apiKey.trim());
    if (Array.isArray(failure.records) && failure.records.length) await store.append(failure.records);
    const report = defineCapabilityReport({ connection, error: failure, redactions: [apiKey.trim()], startedAt, completedAt: new Date() });
    try {
      await reportStore.write(report);
      failure.reportId = report.reportId;
    } catch (reportError) {
      failure.reportError = reportError.message;
    }
    throw failure;
  }
}

export async function verifyCodexConnectionEdge({ baseEnvironment, conformanceRunner, credentialScopes = {}, evidencePath, parentModel, parentProvider, childModel, childProvider, qualificationContextDigest = "default", verifier = new CapabilityVerifier() }) {
  if (!conformanceRunner?.verifyDirectedEdge) throw new Error("当前认证器不支持有向子模型认证");
  const parent = defineConnection(findConnectionEntry({ model: parentModel, provider: parentProvider }));
  const child = defineConnection(findConnectionEntry({ model: childModel, provider: childProvider }));
  if (parent.id === child.id) throw new Error("相同连接请使用普通连接认证");
  const parentApiKey = baseEnvironment[parent.credentialRef];
  const childApiKey = baseEnvironment[child.credentialRef];
  if (typeof parentApiKey !== "string" || !parentApiKey.trim() || typeof childApiKey !== "string" || !childApiKey.trim()) throw new Error("父模型或子模型缺少认证凭据");
  const startedAt = new Date();
  const parentProbe = await verifier.verify(parent, { apiKey: parentApiKey.trim(), credentialScope: credentialScopes[parent.id] ?? parent.credentialRef, now: startedAt, qualificationContextDigest });
  const childProbe = await verifier.verify(child, { apiKey: childApiKey.trim(), credentialScope: credentialScopes[child.id] ?? child.credentialRef, now: startedAt, qualificationContextDigest });
  const edge = await conformanceRunner.verifyDirectedEdge({
    parentApiKey: parentApiKey.trim(),
    childApiKey: childApiKey.trim(),
    parentConnection: parent,
    childConnection: child,
    parentCredentialScope: credentialScopes[parent.id] ?? parent.credentialRef,
    childCredentialScope: credentialScopes[child.id] ?? child.credentialRef,
    parentProtocol: parentProbe.selectedProtocol,
    childProtocol: childProbe.selectedProtocol,
    parentProbeEvidence: parentProbe.records,
    childProbeEvidence: childProbe.records,
    qualificationContextDigest,
    now: startedAt,
  });
  const store = new CapabilityEvidenceStore(evidencePath);
  const existing = await store.read();
  const record = { ...edge.record, supersedes: supersededEvidence(existing, edge.record) };
  let result = redactVerificationValue({ selectedProtocol: parentProbe.selectedProtocol, attempts: [...parentProbe.attempts, ...childProbe.attempts, edge.attempt], records: [...parentProbe.records, ...childProbe.records, record] }, parentApiKey.trim());
  result = redactVerificationValue(result, childApiKey.trim());
  const report = defineCapabilityReport({ connection: parent, result, redactions: [parentApiKey.trim(), childApiKey.trim()], startedAt, completedAt: new Date() });
  const reportStore = new CapabilityReportStore(join(dirname(evidencePath), "capability-reports"));
  await reportStore.write(report);
  const bound = { ...record, reportRef: report.reportId, reportDigest: report.reportId };
  await store.append(result.records.map((record) => ({ ...record, reportRef: report.reportId, reportDigest: report.reportId })));
  return { reportId: report.reportId, parentConnectionId: parent.id, childConnectionId: child.id, selectedProtocols: { parent: parentProbe.selectedProtocol, child: childProbe.selectedProtocol }, attempt: edge.attempt, record: { capability: bound.capability, status: bound.status, scope: bound.scope, checkedAt: bound.checkedAt, expiresAt: bound.expiresAt } };
}

export function installManagedAgentRoles(codexHome, sourceRoot) {
  if (!existsSync(sourceRoot)) throw new Error("创作引擎子任务角色目录缺失");
  const roleFiles = readdirSync(sourceRoot, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /^[a-z][a-z0-9_-]*\.toml$/.test(entry.name))
    .map((entry) => entry.name)
    .sort();
  if (roleFiles.length === 0) throw new Error("创作引擎子任务角色为空");
  const expected = NOVEL_AGENT_ROLES.map(role => `${role.id}.toml`).sort();
  if (JSON.stringify(roleFiles) !== JSON.stringify(expected)) throw new Error("创作引擎子任务角色集合与当前目录不一致");
  // Validate every profile before touching the currently installed role directory.
  for (const role of NOVEL_AGENT_ROLES) {
    if (readFileSync(join(sourceRoot, `${role.id}.toml`), "utf8") !== renderNovelAgentRole(role)) {
      throw new Error(`创作引擎子任务角色配置不一致：${role.id}`);
    }
  }
  const destination = join(codexHome, "agents");
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true, mode: 0o700 });
  for (const name of roleFiles) {
    const target = join(destination, name);
    copyFileSync(join(sourceRoot, name), target);
    chmodSync(target, 0o600);
  }
  return roleFiles;
}

export function resolveCodexCli({
  architecture = process.arch,
  explicitPath,
  isPackaged,
  platform = process.platform,
  projectRoot,
  resourcesPath,
}) {
  let policy;
  try {
    const manifestRoot = isPackaged ? resourcesPath : projectRoot;
    policy = readJsonManifest(join(manifestRoot, "codex-runtime", "supported-cli.json"), "CLI 支持清单");
    if (policy.appServerProtocol !== "v2" || policy.mcpProtocol !== "2025-06-18"
      || policy.modelCatalogSha256 !== CLI_POLICY.modelCatalogSha256
      || policy.modelCertificationsSha256 !== CLI_POLICY.modelCertificationsSha256
      || sha256(join(manifestRoot, "codex-runtime", "model-catalog.json")) !== policy.modelCatalogSha256
      || sha256(certificationPath(join(manifestRoot, "codex-runtime"), policy)) !== policy.modelCertificationsSha256) {
      throw new Error("CLI 支持清单的协议版本无效");
    }
    if (isPackaged) {
      const release = readJsonManifest(join(resourcesPath, "release-manifest.json"), "发布来源清单");
      const expectedDigest = release.codex?.binarySha256;
      const bundledDigest = policy.sources?.bundled?.digests?.[`${platform}-${architecture}`] ?? null;
      if (release.target?.platform !== platform || release.target?.architecture !== architecture || expectedDigest !== bundledDigest) {
        throw new Error("发布来源清单与 CLI 支持清单不一致");
      }
      const expectedIdentity = policy.sources?.bundled?.buildIdentities?.[`${platform}-${architecture}`];
      if (release.codex?.buildIdentityId !== expectedIdentity?.identityId || release.codex?.buildIdentitySha256 !== expectedIdentity?.sha256) throw new Error("发布来源清单与 BuildIdentity 不一致");
      if (release.codex?.unsignedBinarySha256 !== expectedIdentity?.unsignedBinarySha256) throw new Error("发布来源清单与未签名构建来源不一致");
    }
  } catch (error) {
    return { path: null, source: "missing", reason: error.message };
  }
  const executable = codexExecutableName(platform);
  const candidates = [];
  const developmentIdentity = join(projectRoot, "codex-runtime", "build-identities", `${platform}-${architecture}.json`);
  if (!isPackaged && explicitPath?.trim()) candidates.push({ path: explicitPath.trim(), source: "explicit", identityPath: developmentIdentity });
  if (isPackaged && resourcesPath) {
    candidates.push({ path: join(resourcesPath, "codex-runtime", "bin", executable), source: "bundled", identityPath: join(resourcesPath, "codex-runtime", "build-identity.json") });
  }
  if (!isPackaged) {
    candidates.push({ path: join(projectRoot, "codex-runtime", "bin", executable), source: "project-runtime", identityPath: developmentIdentity });
    candidates.push({ path: join(projectRoot, "..", "..", "codex-rs", "target", "debug", executable), source: "workspace-build", identityPath: developmentIdentity });
  }

  let rejectedDesktopBinary = false;
  let rejectedIntegrity = false;
  let rejectedPolicy = Boolean(isPackaged && explicitPath?.trim());
  for (const candidate of candidates) {
    const sourcePolicy = policy.sources?.[candidate.source];
    if (!sourcePolicy || sourcePolicy.packaged !== isPackaged || (sourcePolicy.developmentOnly === true && isPackaged)) {
      rejectedPolicy = true;
      continue;
    }
    const path = canonicalPath(candidate.path);
    if (isChatGptDesktopBinary(path, platform)) {
      rejectedDesktopBinary = true;
      continue;
    }
    if (isExecutableFile(path)) {
      try {
        const repositoryRoot = !isPackaged ? resolve(projectRoot, "../..") : null;
        if (!cliMatchesPolicy(path, sourcePolicy, platform, architecture, candidate.identityPath, repositoryRoot)) throw new Error("identity mismatch");
      } catch {
        rejectedIntegrity = true;
        continue;
      }
      return { path, source: candidate.source, reason: null };
    }
  }

  return {
    path: null,
    source: "missing",
    reason: rejectedIntegrity
      ? "创作引擎的 SHA-256 或平台版本不在固定支持清单中"
      : rejectedPolicy
      ? "CLI 来源未获 supported-cli.json 授权"
      : rejectedDesktopBinary
      ? "已拒绝使用 ChatGPT 桌面版附带的引擎，请配置小说工坊专用创作引擎"
      : "未找到小说工坊专用创作引擎",
  };
}
