// @ts-check
import { terminateEngineProcess } from "./engine-process-lifecycle.mjs";
import { app, BrowserWindow, dialog, session, nativeImage, protocol, safeStorage, shell } from "electron";
import { createCodexEnvironment, createQualificationContextDigest, credentialScopesFromEnvironment, inspectCodexConnection, prepareIsolatedStorage, providerConfigurationError, resolveCodexCli, resolveCodexProvider, resolveRuntimeBinaries } from "./codex-runtime.mjs";
import { HostRecoveryJournal, PersistentTaskBudget, RecoveryIndex } from "@eternallight/agent-engine";
import { createNovelRecoveryOperationContract } from "./novel-recovery-contract.mjs";
import { join } from "node:path";
import { readFileSync } from "node:fs";
import { readModelEvidenceCache } from "./model-evidence-cache.mjs";
import { readModelSelection } from "./model-selection-store.mjs";
import { MultimediaHost, MediaConnectionStore, resolveMediaExecutables } from "./media-host.mjs";
import { createMediaExecutionServer } from "./media-execution-server.mjs";
import { reconcileRecoveryIndex } from "./recovery-reconciliation.mjs";
import { renderMediaHtml as renderMediaHtmlWithElectron } from "./media-html-renderer.mjs";

// Explicit live capabilities avoid stale configuration snapshots during concurrent execution.
/**
 * @param {{
 *   runtimeProcess: import("node:child_process").ChildProcess | null,
 *   readonly executionContext: import("./execution-domains.mjs").ExecutionDomains["current"],
 *   mediaExecutionServer: Awaited<ReturnType<typeof import("./media-execution-server.mjs").createMediaExecutionServer>> | null,
 *   readonly conversationMessages: ReturnType<typeof import("./conversation-delivery.mjs").createConversationDelivery>["conversationMessages"],
 *   readonly workExecutionConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionConsumer"],
 *   readonly executionDomains: import("./execution-domains.mjs").ExecutionDomains,
 *   readonly workExecutionOwners: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["workExecutionOwners"],
 *   requestRuntimeStartupCheck: () => void,
 *   readonly chapterStructureConsumer: ReturnType<typeof import("./automatic-work-consumers.mjs").createAutomaticWorkConsumers>["chapterStructureConsumer"],
 *   readonly clearAgentCallbacks: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["clearAgentCallbacks"],
 *   bundledRuntime: {creativeRuntime: string, novelWorkspaceMcp: string} | null,
 *   readonly developmentRuntimeBinaries: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["developmentRuntimeBinaries"],
 *   startupBudgetsRecovered: boolean,
 *   recoveryIndex: import("@eternallight/agent-engine").RecoveryIndex | null,
 *   readonly runtimeAssetRoot: string,
 *   readonly projectRoot: string,
 *   engineBaseInstructions: string,
 *   engineQualificationContextDigest: string | null,
 *   startupPreparation: ReturnType<ReturnType<typeof import("./desktop-services.mjs").createDesktopServices>["prepareStartup"]> | null,
 *   readonly recordAgentEvent: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["recordAgentEvent"],
 *   readonly prepareModelTransport: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["prepareModelTransport"],
 *   multimediaHost: import("./media-host.mjs").MultimediaHost,
 *   readonly runtimeRequest: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["runtimeRequest"],
 *   readonly startRuntime: ReturnType<typeof import("./runtime-service.mjs").createRuntimeService>["startRuntime"],
 *   readonly ensureCodexConfig: ReturnType<typeof import("./model-transport.mjs").createModelTransport>["ensureCodexConfig"],
 *   agentCli: ReturnType<typeof import("./codex-runtime.mjs").resolveCodexCli> | null,
 *   readonly runtimeUrl: string,
 *   readonly runtimeCandidateToken: string,
 *   readonly readRecoveryOperationEvidence: ReturnType<typeof import("./agent-turn-runtime.mjs").createAgentTurnRuntime>["readRecoveryOperationEvidence"],
 *   readonly registerIpcHandlers: ReturnType<typeof import("./trusted-ipc-registry.mjs").createTrustedIpcRegistry>["registerIpcHandlers"],
 * }} dependencies
 */
export function createDesktopServices(dependencies) {
  async function stopRuntime() {
    const child = dependencies.runtimeProcess;
    const bridge = dependencies.executionContext().modelBridge;
    const mediaServer = dependencies.mediaExecutionServer;
    dependencies.conversationMessages.pause();
    await dependencies.workExecutionConsumer.stop();
    await dependencies.executionDomains.closeAll();
    await dependencies.workExecutionConsumer.settled();
    await dependencies.conversationMessages.settled();
    await dependencies.workExecutionOwners.stop();
    dependencies.mediaExecutionServer = null;
    // Stop new media IO while Runtime can still seal any proven not-sent result.
    // Aborting IO preserves uncertain remote jobs in their original ledger.
    await mediaServer?.close();
    dependencies.runtimeProcess = null;
    dependencies.executionContext().modelBridge = null;
    dependencies.requestRuntimeStartupCheck = () => {};
    dependencies.chapterStructureConsumer.stop();
    dependencies.clearAgentCallbacks();
    dependencies.executionContext().codexAdapter.close();
    await Promise.allSettled([terminateEngineProcess(child), bridge?.close()]);
  }

  async function prepareStartup(signal) {
    signal.throwIfAborted();
    dependencies.bundledRuntime = app.isPackaged ? resolveRuntimeBinaries({ resourcesPath: process.resourcesPath }) : dependencies.developmentRuntimeBinaries();
    const storage = prepareIsolatedStorage(app.getPath("userData"));
    if (!dependencies.startupBudgetsRecovered) {
      const recoveredBudgets = PersistentTaskBudget.recoverOrphans(storage.budgetDb);
      dependencies.startupBudgetsRecovered = true;
      if (recoveredBudgets.length) console.warn(`[novel-codex] recovered ${recoveredBudgets.length} unfinished model budget(s) as unknown usage`);
    }
    dependencies.recoveryIndex = new RecoveryIndex(storage.recoveryPath);
    dependencies.executionContext().recoveryJournal = new HostRecoveryJournal(dependencies.recoveryIndex, createNovelRecoveryOperationContract({ getBinding: () => {
      const task = dependencies.executionContext().activeAgentExecution?.task;
      return task ? { runId: task.id, projectId: task.projectId, threadId: task.threadId } : null;
    } }));
    const enginePromptPath = app.isPackaged
      ? join(dependencies.runtimeAssetRoot, "codex-runtime", "engine-prompt.md")
      : join(dependencies.projectRoot, "codex-runtime/engine-prompt.md");
    dependencies.engineBaseInstructions = readFileSync(enginePromptPath, "utf8").trim();
    if (!dependencies.engineBaseInstructions) throw new Error("创作引擎基础指令为空");
    const codexCli = resolveCodexCli({
      explicitPath: process.env.NOVEL_CODEX_CLI,
      isPackaged: app.isPackaged,
      projectRoot: dependencies.projectRoot,
      resourcesPath: process.resourcesPath,
    });
    dependencies.engineQualificationContextDigest = codexCli.path
      ? createQualificationContextDigest({ baseInstructions: dependencies.engineBaseInstructions, cliPath: codexCli.path })
      : null;
    const evidenceSnapshot = await readModelEvidenceCache(storage.evidencePath);
    signal.throwIfAborted();
    let savedSelection = null, selectionError = null;
    try { savedSelection = readModelSelection(join(app.getPath("userData"), "model-selection.json")); }
    catch (error) { selectionError = error.message; }
    let selectedConnection;
    try { selectedConnection = inspectCodexConnection(savedSelection ?? { model: process.env.NOVEL_CODEX_MODEL, provider: process.env.NOVEL_CODEX_PROVIDER }); }
    catch (error) { selectionError = error.message; selectedConnection = inspectCodexConnection({ model: undefined, provider: undefined }); }
    let providerConfig = null;
    let capabilityError = null;
    try {
      if (selectionError) throw new Error(selectionError);
      if (evidenceSnapshot.error) throw new Error(evidenceSnapshot.error);
      if (!dependencies.engineQualificationContextDigest) throw new Error(codexCli.reason ?? "内置创作引擎来源不可验证");
      providerConfig = resolveCodexProvider({
        credentialScopes: credentialScopesFromEnvironment(process.env, storage.credentialSaltPath),
        evidence: evidenceSnapshot.records,
        model: selectedConnection.id,
        reasoningEffort: selectedConnection.reasoning?.effort ?? null,
        provider: selectedConnection.provider,
        qualificationContextDigest: dependencies.engineQualificationContextDigest,
      });
    } catch (error) {
      capabilityError = error instanceof Error ? error.message : String(error);
    }
  return { storage, codexCli, selectedConnection, providerConfig, capabilityError };
  }

  async function initializeStartup({ signal }) {
    dependencies.startupPreparation ??= prepareStartup(signal).catch(error => { dependencies.startupPreparation = null; throw error; });
    const { storage, codexCli, selectedConnection, providerConfig, capabilityError } = await dependencies.startupPreparation;
    signal.throwIfAborted();
    dependencies.executionContext().codexAdapter.setCallbacks({ onRawEvent: dependencies.recordAgentEvent });
    const modelTransport = providerConfig ? await dependencies.prepareModelTransport(providerConfig, storage, {
      signal, onInstance: instance => { dependencies.executionContext().modelBridge = instance; },
    }) : { bridge: null, bridgeCredential: null, instance: null };
    signal.throwIfAborted();
    if (!dependencies.multimediaHost) {
      dependencies.multimediaHost = new MultimediaHost({ runtimeRequest: dependencies.runtimeRequest,
        directory: join(app.getPath("userData"), "media"),
        connections: new MediaConnectionStore({ directory: app.getPath("userData"), encryption: safeStorage, credentialSaltPath: storage.credentialSaltPath }),
        executables: resolveMediaExecutables({ packaged: app.isPackaged, resourcesPath: process.resourcesPath, environment: process.env }),
        chooseFiles: async () => {
          const result = await dialog.showOpenDialog({ properties: ["openFile", "multiSelections"],
            filters: [{ name: "图片、音频和视频", extensions: ["png", "jpg", "jpeg", "webp", "gif", "wav", "mp3", "m4a", "ogg", "mp4", "mov", "webm"] }] });
          return result.canceled ? [] : result.filePaths.map(path => ({ path }));
        },
        decodeImage: bytes => {
          const image = nativeImage.createFromBuffer(Buffer.from(bytes));
          if (image.isEmpty()) throw new Error("图片无法解码");
          return image.getSize();
        },
        renderHtml: renderMediaHtml,
        openPath: path => shell.openPath(path),
      });
      protocol.handle("creative-media", request => dependencies.multimediaHost.preview(request).catch(() => new Response("Not found", { status: 404 })));
    }
    dependencies.mediaExecutionServer ??= await createMediaExecutionServer({ runtimeRequest: dependencies.runtimeRequest, multimediaHost: dependencies.multimediaHost, databasePath: storage.budgetDb, getExecution: record => dependencies.executionDomains.find({ projectId: record.projectId, taskId: record.attemptId })?.activeAgentExecution });
    dependencies.executionDomains.reopen();
    await dependencies.startRuntime(storage.runtimeDb, storage.budgetDb, signal);
    const startupRequest = (path, options = {}) => dependencies.runtimeRequest(path, { ...options, signal });
    if (providerConfig) dependencies.ensureCodexConfig(storage.codexHome, providerConfig, modelTransport.bridge);
    dependencies.executionContext().agentStorage = storage;
    dependencies.agentCli = codexCli;
    dependencies.executionContext().agentProviderConfig = providerConfig ?? selectedConnection;
    const configurationError = capabilityError ?? providerConfigurationError(selectedConnection, process.env);
    dependencies.executionContext().codexAdapter.configure({
      apiKey: null,
      command: codexCli.path ? [codexCli.path, "app-server"] : null,
      commandSource: codexCli.source,
      configurationError,
      cwd: storage.workspaceHome,
      env: createCodexEnvironment(process.env, { ...storage, credentialRef: modelTransport.bridge ? null : selectedConnection.credentialRef, bridgeCredential: modelTransport.bridgeCredential, runtimeUrl: dependencies.runtimeUrl, candidateToken: dependencies.runtimeCandidateToken }),
      model: selectedConnection.model,
      modelRoute: selectedConnection.id,
      provider: selectedConnection.provider,
    });
    await reconcileRecoveryIndex(dependencies.recoveryIndex, startupRequest, (projectId, phase, error) => {
      console.error(`[novel-codex] recovery ${phase} reconciliation failed for project ${projectId}: ${error instanceof Error ? error.message : String(error)}`);
    }, dependencies.readRecoveryOperationEvidence);
    signal.throwIfAborted();
    if (!codexCli.path) console.warn(`[novel-codex] ${codexCli.reason}`);
    if (configurationError) console.warn(`[novel-codex] ${configurationError}`);
    dependencies.registerIpcHandlers();
    dependencies.workExecutionOwners.start();
    dependencies.chapterStructureConsumer.start();
    dependencies.conversationMessages.start();
    dependencies.workExecutionConsumer.start();
  }

  async function renderMediaHtml(options) {
    return renderMediaHtmlWithElectron(options, { BrowserWindow, session });
  }

    return { stopRuntime, prepareStartup, initializeStartup, renderMediaHtml };
}
