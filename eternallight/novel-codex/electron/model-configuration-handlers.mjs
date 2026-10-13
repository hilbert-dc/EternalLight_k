// @ts-check
import { readAgentStatus } from "./agent-availability.mjs";
import { listModelConnections, prepareEvidenceReverification } from "./model-evidence-cache.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, createCodexEnvironment, credentialScopesFromEnvironment, inspectCodexConnection, loadCodexEvidence, providerConfigurationError, resolveCodexProvider, verifyCodexConnection, verifyCodexConnectionEdge, verifyCodexImageInput } from "./codex-runtime.mjs";
import { CodexConformanceRunner } from "./codex-conformance-runner.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS } from "./creative-mission.mjs";
import { CodexCoreIntegrationRunner } from "./codex-core-integration-runner.mjs";
import { selectModelReasoning } from "./model-reasoning.mjs";
import { saveModelSelection, withModelConfigurationRollback } from "./model-selection-store.mjs";
import { join } from "node:path";
import { CodexAppServerAdapter } from "./codex-adapter.mjs";
import { applyAgentConfiguration } from "./agent-configuration.mjs";

// Registered only through the trusted, execution-scoped IPC registry.
export function registerModelConfigurationHandlers(ipcMain, { app, executionContext, assertCurrentAgentPlan, workExecutionConsumer, refreshRecoveryReport, modelConnectionGate, prepareModelTransport, projectRoot, runtimeCandidateToken, ensureCodexConfig, state }) {
  ipcMain.handle("agent:status", async () => ({ ...await readAgentStatus({
    readStatus: () => executionContext().codexAdapter.status(),
    isActive: () => Boolean(executionContext().activeAgentExecution || executionContext().activeWorkFlow || executionContext().activeGenerationRunId),
    getOwner: () => executionContext().agentProviderConfig,
    check: () => assertCurrentAgentPlan(null),
  }), reasoningEffort: executionContext().agentProviderConfig?.reasoning?.effort ?? null, executionHealth: workExecutionConsumer.health() }));
  ipcMain.handle("agent:recovery-report", refreshRecoveryReport);
  ipcMain.handle("agent:list-connections", async (_event, provider, model, reasoningEffort) => ({
    connections: await listModelConnections({
      selection: provider && model ? { provider, model, reasoningEffort } : { provider: executionContext().agentProviderConfig?.provider, model: executionContext().agentProviderConfig?.id, reasoningEffort: executionContext().agentProviderConfig?.reasoning?.effort ?? null },
      credentialScopes: executionContext().agentStorage ? credentialScopesFromEnvironment(process.env, executionContext().agentStorage.credentialSaltPath) : {},
      evidencePath: executionContext().agentStorage.evidencePath,
      qualificationContextDigest: state.engineQualificationContextDigest ?? "unavailable",
    }),
  }));
  ipcMain.handle("agent:verify-connection", async (_event, provider, model, reasoningEffort = null) => modelConnectionGate.run(async () => {
    if (executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!executionContext().agentStorage || !state.agentCli?.path) throw new Error("创作引擎尚未就绪，无法执行真实能力认证");
    const connection = inspectCodexConnection({ provider, model, reasoningEffort });
    const credentialError = providerConfigurationError(connection, process.env);
    if (credentialError) throw new Error(credentialError);
    await prepareEvidenceReverification(executionContext().agentStorage.evidencePath);
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: state.engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      command: [state.agentCli.path, "app-server"],
      cwd: executionContext().agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    const coreRunner = new CodexCoreIntegrationRunner({
      baseEnvironment: process.env,
      baseInstructions: state.engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      command: [state.agentCli.path, "app-server"],
      cwd: executionContext().agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    return verifyCodexConnection({
      baseEnvironment: process.env,
      conformanceRunner,
      coreRunner,
      credentialScopes: credentialScopesFromEnvironment(process.env, executionContext().agentStorage.credentialSaltPath),
      evidencePath: executionContext().agentStorage.evidencePath,
      provider,
      qualificationContextDigest: state.engineQualificationContextDigest ?? "unavailable",
      model,
      reasoningEffort,
    });
  }));
  ipcMain.handle("agent:verify-image-input", async (_event, provider, model, reasoningEffort = null) => modelConnectionGate.run(async () => {
    if (!executionContext().agentStorage || !state.agentCli?.path) throw new Error("创作引擎尚未就绪，无法验证图片输入");
    const connection = inspectCodexConnection({ provider, model, reasoningEffort });
    const credentialError = providerConfigurationError(connection, process.env);
    if (credentialError) throw new Error(credentialError);
    return verifyCodexImageInput({ baseEnvironment: process.env,
      credentialScopes: credentialScopesFromEnvironment(process.env, executionContext().agentStorage.credentialSaltPath),
      evidencePath: executionContext().agentStorage.evidencePath, provider, model, reasoningEffort,
      qualificationContextDigest: state.engineQualificationContextDigest ?? "unavailable" });
  }));
  ipcMain.handle("agent:verify-connection-edge", async (_event, parentProvider, parentModel, childProvider, childModel) => modelConnectionGate.run(async () => {
    if (executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，不能验证连接");
    if (!executionContext().agentStorage || !state.agentCli?.path) throw new Error("创作引擎尚未就绪，无法执行真实能力认证");
    for (const [provider, model] of [[parentProvider, parentModel], [childProvider, childModel]]) {
      const credentialError = providerConfigurationError(inspectCodexConnection({ provider, model }), process.env);
      if (credentialError) throw new Error(credentialError);
    }
    await prepareEvidenceReverification(executionContext().agentStorage.evidencePath);
    const conformanceRunner = new CodexConformanceRunner({
      baseEnvironment: process.env,
      baseInstructions: state.engineBaseInstructions,
      developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS,
      subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,
      command: [state.agentCli.path, "app-server"],
      cwd: executionContext().agentStorage.workspaceHome,
      nodeCommand: process.execPath,
    });
    return verifyCodexConnectionEdge({
      baseEnvironment: process.env,
      conformanceRunner,
      credentialScopes: credentialScopesFromEnvironment(process.env, executionContext().agentStorage.credentialSaltPath),
      evidencePath: executionContext().agentStorage.evidencePath,
      parentProvider,
      parentModel,
      childProvider,
      childModel,
      qualificationContextDigest: state.engineQualificationContextDigest ?? "unavailable",
    });
  }));
  ipcMain.handle("agent:set-reasoning-effort", (_event, provider, model, effort) => {
    // Model changes remain serialized by the connection gate; reasoning changes
    // are safe during execution and never touch the active lease or request.
    modelConnectionGate.assertExecutionAllowed();
    executionContext().agentProviderConfig = selectModelReasoning({ current: executionContext().agentProviderConfig, provider, model, effort,
      persist: selection => saveModelSelection(join(app.getPath("userData"), "model-selection.json"), selection) });
    return { reasoningEffort: executionContext().agentProviderConfig.reasoning.effort };
  });
  ipcMain.handle("agent:configure", async (_event, provider, model, reasoningEffort = null) => modelConnectionGate.run(async () => {
    if (executionContext().activeAgentExecution) throw new Error("已有创作引擎任务正在运行，请先等待或停止当前任务");
    if (typeof provider !== "string" || provider.length > 40 || /[\r\n]/.test(provider)) throw new Error("Provider 无效");
    if (typeof model !== "string" || model.length > 120 || /[\r\n]/.test(model)) throw new Error("Model 无效");
    if (!executionContext().agentStorage || !state.agentCli) throw new Error("创作配置尚未就绪");
    const requestEpoch = ++state.configurationRequestEpoch;
    const evidence = await loadCodexEvidence(executionContext().agentStorage.evidencePath);
    const next = resolveCodexProvider({
      credentialScopes: credentialScopesFromEnvironment(process.env, executionContext().agentStorage.credentialSaltPath),
      evidence,
      provider,
      model,
      reasoningEffort,
      qualificationContextDigest: state.engineQualificationContextDigest ?? "unavailable",
    });
    const configurationError = providerConfigurationError(next, process.env);
    if (configurationError) throw new Error(configurationError);
    const transport = await prepareModelTransport(next);
    const previousBridge = executionContext().modelBridge;
    const previousAdapter = executionContext().codexAdapter;
    const nextAdapter = new CodexAppServerAdapter({ command: null, cwd: projectRoot, languagePreferences: () => ({ preferredLanguages: app.getPreferredSystemLanguages() }) });
    let status;
    try {
      if (requestEpoch !== state.configurationRequestEpoch) throw new Error("已有更新的模型配置，本次迟到结果不会发布");
      if (executionContext().activeAgentExecution) throw new Error("模型配置准备期间已有任务启动，已取消本次切换");
      status = applyAgentConfiguration({
        adapter: nextAdapter,
        configuration: {
          apiKey: null,
          command: state.agentCli.path ? [state.agentCli.path, "app-server"] : null,
          commandSource: state.agentCli.source,
          configurationError,
          cwd: executionContext().agentStorage.workspaceHome,
          env: createCodexEnvironment(process.env, { ...executionContext().agentStorage, credentialRef: transport.bridge ? null : next.credentialRef, bridgeCredential: transport.bridgeCredential, runtimeUrl: state.runtimeUrl, candidateToken: runtimeCandidateToken }),
          model: next.model,
          modelRoute: next.id,
          provider: next.provider,
        },
        persist: () => withModelConfigurationRollback([
          join(executionContext().agentStorage.codexHome, "config.toml"), join(executionContext().agentStorage.codexHome, "model-catalog.json"),
          join(app.getPath("userData"), "model-selection.json"),
        ], () => {
          ensureCodexConfig(executionContext().agentStorage.codexHome, next, transport.bridge);
          saveModelSelection(join(app.getPath("userData"), "model-selection.json"), { provider: next.provider, model: next.id, reasoningEffort: next.reasoning?.effort ?? null });
        }),
      });
    } catch (error) {
      if (transport.instance) await transport.instance.close();
      nextAdapter.close();
      throw error;
    }
    // Single publication point: readers can observe only the old or the new
    // adapter/bridge/plan tuple. Cleanup of the old tuple is deliberately later.
    executionContext().codexAdapter = nextAdapter;
    executionContext().modelBridge = transport.instance;
    executionContext().agentProviderConfig = next;
    previousAdapter.close();
    if (previousBridge) await previousBridge.close();
    workExecutionConsumer.poke();
    return { ...status, reasoningEffort: next.reasoning?.effort ?? null };
  }));
}
