import { mcpRequestIdentity } from "./mcp-authority.mjs";
// @ts-check
import { join } from "node:path";
import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import { buildCodexConfig, buildCodexModelCatalog, createCodexEnvironment, credentialScopesFromEnvironment, installManagedAgentRoles, loadCodexEvidence, recordRuntimeQualificationFailure, resolveCodexProvider } from "./codex-runtime.mjs";
import { HostRecoveryJournal, ModelBridgeProcess, requestTaskCancellation } from "@eternallight/agent-engine";
import { executorValidatedTools } from "./executor-validation-policy.mjs";
import { requestReasoningConfiguration } from "./model-reasoning.mjs";
import { modelFailureDiagnostic } from "./model-failure-diagnostic.mjs";
import { createHash } from "node:crypto";
import { CodexAppServerAdapter } from "./codex-adapter.mjs";
import { app } from "electron";
import { createNovelRecoveryOperationContract } from "./novel-recovery-contract.mjs";
import { assertAgentAvailability } from "./agent-availability.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   readonly engineBaseInstructions: string,
 *   readonly runtimeAssetRoot: string,
 *   readonly bundledRuntime: {creativeRuntime: string, novelWorkspaceMcp: string} | null,
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly baseExecutionDomain: import("./execution-domains.mjs").ExecutionDomains["base"],
 *   readonly agentCli: ReturnType<typeof import("./codex-runtime.mjs").resolveCodexCli> | null,
 *   readonly modelConnectionGate: import("./model-connection-gate.mjs").ModelConnectionGate,
 *   readonly recoveryIndex: import("@eternallight/agent-engine").RecoveryIndex | null,
 *   readonly runtimeUrl: string,
 *   readonly runtimeCandidateToken: string,
 *   readonly recordAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["recordAgentEvent"],
 *   readonly sendAgentMessage: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["sendAgentMessage"],
 *   readonly interruptAgentTurn: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["interruptAgentTurn"],
 *   readonly engineQualificationContextDigest: string | null,
 * }} dependencies
 */
export function createModelTransport(dependencies) {
  function ensureCodexConfig(codexHome, providerConfig, bridge = null) {
    const configPath = join(codexHome, "config.toml");
    const modelCatalogPath = join(codexHome, "model-catalog.json");
    const catalogTemporaryPath = `${modelCatalogPath}.${process.pid}.tmp`;
    const configTemporaryPath = `${configPath}.${process.pid}.tmp`;
    writeFileSync(catalogTemporaryPath, `${JSON.stringify(buildCodexModelCatalog({ ...providerConfig, baseInstructions: dependencies.engineBaseInstructions }), null, 2)}\n`, { mode: 0o600 });
    renameSync(catalogTemporaryPath, modelCatalogPath);
    installManagedAgentRoles(codexHome, join(dependencies.runtimeAssetRoot, "codex-runtime", "agents"));
    writeFileSync(configTemporaryPath, buildCodexConfig({
      ...providerConfig,
      bridge,
      mcpServerCommand: dependencies.bundledRuntime.novelWorkspaceMcp,
      mcpServerPath: null,
      mcpServerArgs: [],
      modelCatalogPath,
    }), { encoding: "utf8", mode: 0o600 });
    renameSync(configTemporaryPath, configPath);
  }

  async function prepareModelTransport(providerConfig, storage = dependencies.executionContext().agentStorage, { signal = undefined, onInstance = (_instance) => {} } = {}) {
    signal?.throwIfAborted();
    const connections = providerConfig.connections ?? [providerConfig.connection];
    let bridgeToken = null;
    const instance = new ModelBridgeProcess({
      executorValidatedTools,
      mcpAuthority: true,
      sessionPlan: providerConfig.sessionPlan,
      connections,
      credentialResolver: async (reference) => process.env[reference] ?? null,
      requestConfigurationProvider: dependencies.executionDomains.bind(async ({ connectionId, threadId }) => {
        const domain = dependencies.executionContext();
        return { ...requestReasoningConfiguration(connections, domain.agentProviderConfig ?? providerConfig, connectionId),
          ...await mcpRequestIdentity(domain.codexAdapter, threadId) };
      }),
      responseObserver: observation => {
        const diagnostic = modelFailureDiagnostic(observation, [bridgeToken, ...connections.map(connection => process.env[connection?.credentialRef])]);
        if (diagnostic) console.error("[novel-codex] model request failed", JSON.stringify(diagnostic));
      },
      qualificationObserver: (failure) => recordRuntimeQualificationFailure({ ...failure, baselineEvidence: providerConfig.evidence, evidencePath: storage.evidencePath }),
    });
    onInstance(instance);
    const started = await instance.start();
    bridgeToken = started.token;
    signal?.throwIfAborted();
    return {
      instance,
      bridge: { baseUrl: started.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
      bridgeCredential: { ref: "AGENT_ENGINE_BRIDGE_TOKEN", value: started.token, authorityUrl: started.baseUrl },
    };
  }

  async function prepareExecutionDomain(domain) {
    const defaults = dependencies.baseExecutionDomain;
    if (!defaults.agentStorage || !defaults.agentProviderConfig || !dependencies.agentCli?.path) throw Error("创作引擎尚未就绪");
    dependencies.modelConnectionGate.assertExecutionAllowed();
    domain.agentProviderConfig = structuredClone(defaults.agentProviderConfig);
    const key = createHash("sha256").update(JSON.stringify([domain.projectId, domain.threadId ?? domain.requestId ?? domain.id])).digest("hex");
    const codexHome = join(defaults.agentStorage.engineData, "execution-homes", key);
    mkdirSync(codexHome, { recursive: true, mode: 0o700 });
    domain.agentStorage = { ...defaults.agentStorage, codexHome };
    domain.codexAdapter = new CodexAppServerAdapter({ command: null, cwd: domain.agentStorage.workspaceHome,
      languagePreferences: () => ({ preferredLanguages: app.getPreferredSystemLanguages() }) });
    domain.recoveryJournal = new HostRecoveryJournal(dependencies.recoveryIndex, createNovelRecoveryOperationContract({ getBinding: () => {
      const task = domain.activeAgentExecution?.task;
      return task ? { runId: task.id, projectId: task.projectId, threadId: task.threadId } : null;
    } }));
    const transport = await prepareModelTransport(domain.agentProviderConfig, domain.agentStorage, {
      onInstance: instance => { domain.modelBridge = instance; },
    });
    ensureCodexConfig(codexHome, domain.agentProviderConfig, transport.bridge);
    domain.codexAdapter.configure({ command: [dependencies.agentCli.path, "app-server"], commandSource: dependencies.agentCli.source,
      cwd: domain.agentStorage.workspaceHome, model: domain.agentProviderConfig.model,
      modelRoute: domain.agentProviderConfig.id, provider: domain.agentProviderConfig.provider,
      env: createCodexEnvironment(process.env, { ...domain.agentStorage, bridgeCredential: transport.bridgeCredential,
        credentialRef: transport.bridge ? null : domain.agentProviderConfig.credentialRef, runtimeUrl: dependencies.runtimeUrl, candidateToken: dependencies.runtimeCandidateToken }) });
    domain.codexAdapter.setCallbacks({ onRawEvent: dependencies.executionDomains.bind(dependencies.recordAgentEvent) });
  }

  async function disposeExecutionDomain(domain) {
    if (domain.settled && !domain.completionSent) {
      domain.completionSent = true;
      dependencies.sendAgentMessage("agent:event", {method: "agent/execution-ended", params: {projectId: domain.projectId,
        threadId: domain.threadId, requestId: domain.requestId, failure: domain.failure ?? null}});
    }
    if (domain.disposing) return domain.disposing;
    domain.disposing = (async () => {
      requestTaskCancellation(domain.activeAgentExecution);
      if (domain.activeAgentExecution) await dependencies.interruptAgentTurn(domain.activeAgentExecution).catch(() => {});
      domain.removeAgentWebContentsListener?.();
      domain.codexAdapter?.close();
      await domain.modelBridge?.close();
      await domain.recoveryJournal?.flush().catch(() => {});
    })();
    return domain.disposing;
  }

  async function assertCurrentAgentPlan(operation, providerConfig = dependencies.executionContext().agentProviderConfig) {
    return assertAgentAvailability(async () => {
      if (!providerConfig?.sessionPlan) throw new Error("Agent SessionPlan 尚未就绪");
      if (!dependencies.engineQualificationContextDigest) throw new Error("Agent QualificationContext 尚未就绪");
      const evidence = dependencies.executionContext().agentStorage
        ? await loadCodexEvidence(dependencies.executionContext().agentStorage.evidencePath)
        : providerConfig.evidence ?? [];
      const current = resolveCodexProvider({
        credentialScopes: dependencies.executionContext().agentStorage ? credentialScopesFromEnvironment(process.env, dependencies.executionContext().agentStorage.credentialSaltPath) : {},
        evidence,
        model: providerConfig.id,
        reasoningEffort: providerConfig.reasoning?.effort ?? null,
        operation,
        provider: providerConfig.provider,
        qualificationContextDigest: dependencies.engineQualificationContextDigest,
      });
      if (current.sessionPlan.id !== providerConfig.sessionPlan.id) {
        throw Object.assign(new Error("模型能力证据或路由计划已经变化，请重新选择模型后再开始任务"), { code: "SESSION_PLAN_STALE" });
      }
      return current.sessionPlan;
    });
  }

  return { ensureCodexConfig, prepareModelTransport, prepareExecutionDomain, disposeExecutionDomain, assertCurrentAgentPlan };
}
