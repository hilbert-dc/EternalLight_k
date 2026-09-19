import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { modelInputTokenBudget } from "../shared/author-input-budget.mjs";

const CODEX_ENVIRONMENT_KEYS = [
  "CODEX_HOME",
  "CODEX_SQLITE_HOME",
  "CODEX_API_KEY",
  "CODEX_ACCESS_TOKEN",
  "CODEX_APP_SERVER_COMMAND",
  "DEEPSEEK_API_KEY",
  "NOVEL_CODEX_CLI",
  "NOVEL_CODEX_RUNTIME_URL",
  "NOVEL_CODEX_CANDIDATE_TOKEN",
  "NOVEL_CODEX_TASK_CAPABILITY",
  "NOVEL_CODEX_AUTHOR_TOKEN",
  "OPENAI_API_KEY",
  "OPENAI_BASE_URL",
];

const MODEL_CATALOG_PATH = fileURLToPath(new URL("../codex-runtime/model-catalog.json", import.meta.url));
const CODEX_MODEL_CATALOG_PATH = fileURLToPath(new URL("../codex-runtime/codex-model-catalog.json", import.meta.url));
const CLI_POLICY_PATH = fileURLToPath(new URL("../codex-runtime/supported-cli.json", import.meta.url));
const MAX_MODEL_MESSAGE_BYTES = 60_000;
const MAX_MODEL_MESSAGE_TOKENS = 9_000;
const MAX_MODEL_INPUT_BYTES = 9_500;
const NOVEL_MCP_ENVIRONMENT_KEYS = ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"];
const DEFAULT_MCP_SERVER_PATH = fileURLToPath(new URL("../runtime/mcp_server.py", import.meta.url));

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

function readJsonManifest(path, label) {
  try {
    const manifest = JSON.parse(readFileSync(path, "utf8"));
    if (manifest?.version !== 1) throw new Error("unsupported version");
    return manifest;
  } catch {
    throw new Error(`${label} 缺失或无效`);
  }
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

const CLI_POLICY = readJsonManifest(CLI_POLICY_PATH, "CLI 支持清单");
if (CLI_POLICY.modelCatalogSha256 !== sha256(MODEL_CATALOG_PATH)
  || CLI_POLICY.codexModelCatalogSha256 !== sha256(CODEX_MODEL_CATALOG_PATH)) {
  throw new Error("运行时模型目录与固定指纹不一致");
}
const MODEL_CATALOG = readJsonManifest(MODEL_CATALOG_PATH, "模型目录");

function cliVersion(path) {
  const result = spawnSync(path, ["--version"], { encoding: "utf8", timeout: 15_000, windowsHide: true });
  if (result.error || result.status !== 0) return null;
  const version = result.stdout.trim();
  return version && !/[\r\n]/.test(version) ? version : null;
}

function cliMatchesPolicy(path, sourcePolicy, platform, architecture) {
  const expectedDigest = sourcePolicy.digests?.[`${platform}-${architecture}`];
  if (!/^[a-f0-9]{64}$/.test(expectedDigest ?? "") || sha256(path) !== expectedDigest) return false;
  const version = cliVersion(path);
  return version !== null && Array.isArray(sourcePolicy.versions) && sourcePolicy.versions.includes(version);
}

export function codexExecutableName(platform = process.platform) {
  return platform === "win32" ? "codex.exe" : "codex";
}

export function createCodexEnvironment(baseEnvironment, {
  candidateToken,
  codexHome,
  provider = "deepseek",
  runtimeUrl,
  sqliteHome,
}) {
  const environment = { ...baseEnvironment };
  for (const key of CODEX_ENVIRONMENT_KEYS) delete environment[key];
  if (provider === "deepseek" && typeof baseEnvironment.DEEPSEEK_API_KEY === "string" && baseEnvironment.DEEPSEEK_API_KEY.trim()) {
    environment.DEEPSEEK_API_KEY = baseEnvironment.DEEPSEEK_API_KEY;
  }
  if (typeof runtimeUrl === "string" && runtimeUrl.trim()) environment.NOVEL_CODEX_RUNTIME_URL = runtimeUrl.trim();
  if (typeof candidateToken === "string" && candidateToken) environment.NOVEL_CODEX_CANDIDATE_TOKEN = candidateToken;
  return {
    ...environment,
    CODEX_HOME: codexHome,
    CODEX_SQLITE_HOME: sqliteHome,
  };
}

export function providerConfigurationError(providerConfig, baseEnvironment) {
  const apiKey = baseEnvironment[providerConfig.apiKeyEnv];
  return typeof apiKey === "string" && apiKey.trim()
    ? null
    : `缺少 ${providerConfig.apiKeyEnv}，${providerConfig.provider} 请求将无法认证`;
}

export function resolveCodexProvider({ model, operation = null, provider }) {
  const resolvedProvider = provider?.trim().toLowerCase() || MODEL_CATALOG.default.provider;
  const resolvedModel = model?.trim() || MODEL_CATALOG.models.find((candidate) => candidate.provider === resolvedProvider)?.model || null;
  const entry = MODEL_CATALOG.models.find((candidate) => candidate.provider === resolvedProvider && candidate.model === resolvedModel);
  if (!entry) throw new Error(`未审核的 Codex provider/model：${resolvedProvider}/${resolvedModel ?? "<missing>"}`);
  if (operation !== null && !entry.operations.includes(operation)) {
    throw new Error(`${resolvedProvider}/${resolvedModel} 未获准执行 ${operation}`);
  }
  return {
    provider: resolvedProvider,
    model: resolvedModel,
    apiKeyEnv: "DEEPSEEK_API_KEY",
    operations: [...entry.operations],
  };
}

export function assertProviderOperation(providerConfig, operation) {
  return resolveCodexProvider({ provider: providerConfig?.provider, model: providerConfig?.model, operation });
}

export function buildCodexConfig({
  modelCatalogPath,
  mcpServerCommand,
  mcpServerPath = DEFAULT_MCP_SERVER_PATH,
  model,
  platform = process.platform,
  provider,
}) {
  const resolved = resolveCodexProvider({ model, provider });
  const mcpCommand = mcpServerCommand?.trim() || process.env.NOVEL_CODEX_PYTHON?.trim() || (platform === "win32" ? "python" : "python3");
  const lines = [
    "# Managed by Novel Codex. Do not use the ChatGPT desktop auth state.",
    `model = ${JSON.stringify(resolved.model)}`,
    ...(resolved.provider === "deepseek" ? ['model_provider = "deepseek"'] : []),
    `model_catalog_json = ${JSON.stringify(modelCatalogPath ?? CODEX_MODEL_CATALOG_PATH)}`,
    'web_search = "disabled"',
    'approval_policy = "never"',
    'sandbox_mode = "read-only"',
    'cli_auth_credentials_store = "ephemeral"',
    "",
    "[features]",
    "collab = false",
    "multi_agent = false",
    "multi_agent_v2 = false",
    "tool_search = false",
    "memory_tool = false",
    "connectors = false",
    "computer_use = false",
    "in_app_browser = false",
    "image_generation = false",
    "imagegenext = false",
    "js_repl = false",
    "",
    "[shell_environment_policy]",
    'inherit = "core"',
    "ignore_default_excludes = false",
    'exclude = ["NOVEL_CODEX_*", "DEEPSEEK_API_KEY", "OPENAI_API_KEY", "OPENAI_ACCESS_TOKEN", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN"]',
  ];
  if (resolved.provider === "deepseek") {
    lines.push(
      "",
      "[model_providers.deepseek]",
      'name = "DeepSeek"',
      'base_url = "https://api.deepseek.com"',
      'wire_api = "responses"',
      'env_key = "DEEPSEEK_API_KEY"',
    );
  }
  lines.push(
    "",
    "[mcp_servers.novel_workspace]",
    `command = ${JSON.stringify(mcpCommand)}`,
    `args = [${JSON.stringify(mcpServerPath)}]`,
    `env_vars = [${NOVEL_MCP_ENVIRONMENT_KEYS.map((key) => JSON.stringify(key)).join(", ")}]`,
  );
  return `${lines.join("\n")}\n`;
}

export function createRuntimeEnvironment(baseEnvironment, projectRoot) {
  const environment = { ...baseEnvironment };
  for (const key of CODEX_ENVIRONMENT_KEYS) delete environment[key];
  return { ...environment, PYTHONPATH: projectRoot };
}

export function loadNovelCodexSkills(projectRoot) {
  const skillsRoot = join(projectRoot, "codex-runtime", "skills");
  if (!existsSync(skillsRoot)) return "";
  const skillFiles = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.isFile() && entry.name === "SKILL.md") skillFiles.push(path);
    }
  };
  visit(skillsRoot);
  return skillFiles
    .map((path) => readFileSync(path, "utf8").trim())
    .filter(Boolean)
    .join("\n\n---\n\n");
}

export function composeNovelCodexPrompt(skillText, prompt, taskInstructions = "") {
  const normalizedPrompt = prompt.trim();
  const message = `${skillText.trim() ? `Novel Codex 专用工作约束（只对当前应用生效）：\n${skillText}\n\n` : ""}${taskInstructions ? `${taskInstructions}\n\n` : ""}用户当前指令：\n${normalizedPrompt}`;
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
  const codexHome = join(userDataPath, "codex-home");
  const sqliteHome = join(codexHome, "sqlite");
  const runtimeData = join(userDataPath, "creative-runtime");
  const runtimeDb = join(runtimeData, "novel-codex-v2.sqlite3");
  for (const directory of [codexHome, sqliteHome, runtimeData]) mkdirSync(directory, { recursive: true });
  return { codexHome, sqliteHome, runtimeData, runtimeDb };
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
      || policy.codexModelCatalogSha256 !== CLI_POLICY.codexModelCatalogSha256
      || sha256(join(manifestRoot, "codex-runtime", "model-catalog.json")) !== policy.modelCatalogSha256
      || sha256(join(manifestRoot, "codex-runtime", "codex-model-catalog.json")) !== policy.codexModelCatalogSha256) {
      throw new Error("CLI 支持清单的协议版本无效");
    }
  } catch (error) {
    return { path: null, source: "missing", reason: error.message };
  }
  const executable = codexExecutableName(platform);
  const candidates = [];
  if (!isPackaged && explicitPath?.trim()) candidates.push({ path: explicitPath.trim(), source: "explicit" });
  if (isPackaged && resourcesPath) {
    candidates.push({ path: join(resourcesPath, "codex-runtime", "bin", executable), source: "bundled" });
  }
  if (!isPackaged) {
    candidates.push({ path: join(projectRoot, "codex-runtime", "bin", executable), source: "project-runtime" });
    candidates.push({ path: join(projectRoot, "..", "..", "codex-rs", "target", "debug", executable), source: "workspace-build" });
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
      if (!cliMatchesPolicy(path, sourcePolicy, platform, architecture)) {
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
      ? "Novel Codex CLI 的平台 SHA-256 或版本不在固定支持清单中"
      : rejectedPolicy
      ? "CLI 来源未获 supported-cli.json 授权"
      : rejectedDesktopBinary
      ? "已拒绝使用 ChatGPT 桌面版附带的 Codex CLI，请配置 Novel Codex 专用 CLI"
      : "未找到 Novel Codex 专用 Codex CLI",
  };
}
