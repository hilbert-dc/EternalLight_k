// @ts-check
import { createConversationDelivery } from "./conversation-delivery.mjs";
import { createAutomaticWorkConsumers } from "./automatic-work-consumers.mjs";
import { createTrustedIpcRegistry } from "./trusted-ipc-registry.mjs";
import { createDesktopWindow } from "./desktop-window.mjs";
import { createRuntimeService } from "./runtime-service.mjs";
import { createModelTransport } from "./model-transport.mjs";
import { createAgentTurnRuntime } from "./agent-turn-runtime.mjs";
import { createWorkFlowCoordinator } from "./work-flow-coordinator.mjs";
import { createCreativeTaskCoordinator } from "./creative-task-coordinator.mjs";
import { createGenerationCoordinator } from "./generation-coordinator.mjs";
import { createPlanningReviewCoordinator } from "./planning-review-coordinator.mjs";
import { createAdvisoryCoordinator } from "./advisory-coordinator.mjs";
import { createDesktopServices } from "./desktop-services.mjs";
import { DesktopLifecycle } from "./desktop-lifecycle.mjs";

import { WorkInterludes } from "./work-interludes.mjs";
import { ExecutionDomains, EXECUTION_POWER_POLICY } from "./execution-domains.mjs";

import { WorkControlCommands } from "./work-control-commands.mjs";
import { ExecutionRequests } from "./execution-requests.mjs";

import { app, BrowserWindow, protocol, powerSaveBlocker } from "electron";

import { randomBytes } from "node:crypto";

import { isAbsolute, join } from "node:path";
import { fileURLToPath } from "node:url";

import { CodexAppServerAdapter } from "./codex-adapter.mjs";

import { ModelConnectionGate } from "./model-connection-gate.mjs";

const { isTrustedRendererUrl, assertTrustedIpcSender, createWindow } = createDesktopWindow({
  get rendererUrl() { return rendererUrl; },
  get packagedEntry() { return packagedEntry; },
  get rendererWebContents() { return rendererWebContents; }, set rendererWebContents(value) { rendererWebContents = value; },
  get __dirname() { return __dirname; },
  get desktopLifecycle() { return desktopLifecycle; },
  get chapterStructureConsumer() { return chapterStructureConsumer; },
});

const { developmentRuntimeBinaries, findFreeLoopbackPort, startRuntime, runtimeRequest } = createRuntimeService({
  get projectRoot() { return projectRoot; },
  get runtimePort() { return runtimePort; }, set runtimePort(value) { runtimePort = value; },
  get configuredRuntimePort() { return configuredRuntimePort; },
  get runtimeUrl() { return runtimeUrl; }, set runtimeUrl(value) { runtimeUrl = value; },
  get runtimeError() { return runtimeError; }, set runtimeError(value) { runtimeError = value; },
  get bundledRuntime() { return bundledRuntime; },
  get runtimeAssetRoot() { return runtimeAssetRoot; },
  get runtimeProcess() { return runtimeProcess; }, set runtimeProcess(value) { runtimeProcess = value; },
  get desktopLifecycle() { return desktopLifecycle; },
  get runtimeCandidateToken() { return runtimeCandidateToken; },
  get runtimeAuthorToken() { return runtimeAuthorToken; },
  get mediaExecutionServer() { return mediaExecutionServer; },
  get requestRuntimeStartupCheck() { return requestRuntimeStartupCheck; }, set requestRuntimeStartupCheck(value) { requestRuntimeStartupCheck = value; },
});

const { ensureCodexConfig, prepareModelTransport, prepareExecutionDomain, disposeExecutionDomain, assertCurrentAgentPlan } = createModelTransport({
  get engineBaseInstructions() { return engineBaseInstructions; },
  get runtimeAssetRoot() { return runtimeAssetRoot; },
  get bundledRuntime() { return bundledRuntime; },
  get executionContext() { return executionContext; },
  get executionDomains() { return executionDomains; },
  get baseExecutionDomain() { return baseExecutionDomain; },
  get agentCli() { return agentCli; },
  get modelConnectionGate() { return modelConnectionGate; },
  get recoveryIndex() { return recoveryIndex; },
  get runtimeUrl() { return runtimeUrl; },
  get runtimeCandidateToken() { return runtimeCandidateToken; },
  get recordAgentEvent() { return recordAgentEvent; },
  get sendAgentMessage() { return sendAgentMessage; },
  get interruptAgentTurn() { return interruptAgentTurn; },
  get engineQualificationContextDigest() { return engineQualificationContextDigest; },
});

const { clearAgentCallbacks, forwardAgentEvent, recordAgentEvent, beginRecoveryRun, interruptAgentTurn, beginAgentExecution, closeFinishedReads, readRecoveryOperationEvidence, refreshRecoveryReport, finishAgentExecution, sendAgentTurn, runCoreTurnWithLease, attachAgentWebContents, sendAgentMessage } = createAgentTurnRuntime({
  get executionContext() { return executionContext; },
  get runtimeRequest() { return runtimeRequest; },
  get executionRequests() { return executionRequests; },
  get modelConnectionGate() { return modelConnectionGate; },
  get assertCurrentAgentPlan() { return assertCurrentAgentPlan; },
  get executionDomains() { return executionDomains; },
  get recoveryIndex() { return recoveryIndex; },
  get workExecutionOwners() { return workExecutionOwners; },
  get runtimeCandidateToken() { return runtimeCandidateToken; },
  get conversationMessages() { return conversationMessages; },
  get engineQualificationContextDigest() { return engineQualificationContextDigest; },
  get rendererWebContents() { return rendererWebContents; },
  get baseExecutionDomain() { return baseExecutionDomain; },
});

const { assertThreadProject, prepareNovelWorkflow, withWorkFlow, prepareWorkExecutionCondition, acquireWorkFlowOwner, currentWorkFlowState, bindWorkFlowAttempt, bindCodexTurn, executionIdentity, controlDomain } = createWorkFlowCoordinator({
  get runtimeRequest() { return runtimeRequest; },
  get executionContext() { return executionContext; },
  get executionRequests() { return executionRequests; },
  get modelConnectionGate() { return modelConnectionGate; },
  get workExecutionOwners() { return workExecutionOwners; },
  get workExecutionConsumer() { return workExecutionConsumer; },
  get assertCurrentAgentPlan() { return assertCurrentAgentPlan; },
  get engineQualificationContextDigest() { return engineQualificationContextDigest; },
  get baseExecutionDomain() { return baseExecutionDomain; },
  get workControlCommands() { return workControlCommands; },
  get executionDomains() { return executionDomains; },
});

const { failedCreativeAttempts, unstartedCreativeAttempts, runCreativeAgentTask, candidateActionDependencies, runQueuedPlanningReview, modelChapterKnowledge, continueCreativeWork, runCandidateWorkflow, continueFailedCreativeWork, resumeWorkTask } = createCreativeTaskCoordinator({
  get executionContext() { return executionContext; },
  get attachAgentWebContents() { return attachAgentWebContents; },
  get executionDomains() { return executionDomains; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get sendAgentMessage() { return sendAgentMessage; },
  get beginAgentExecution() { return beginAgentExecution; },
  get runtimeRequest() { return runtimeRequest; },
  get bindWorkFlowAttempt() { return bindWorkFlowAttempt; },
  get prepareNovelWorkflow() { return prepareNovelWorkflow; },
  get beginRecoveryRun() { return beginRecoveryRun; },
  get sendAgentTurn() { return sendAgentTurn; },
  get bindCodexTurn() { return bindCodexTurn; },
  get finishAgentExecution() { return finishAgentExecution; },
  get currentWorkFlowState() { return currentWorkFlowState; },
  get runPlanningReview() { return runPlanningReview; },
  get runAdvisoryAgentTask() { return runAdvisoryAgentTask; },
  get withWorkFlow() { return withWorkFlow; },
  get acquireWorkFlowOwner() { return acquireWorkFlowOwner; },
  get continueGenerationRun() { return continueGenerationRun; },
  get retryGenerationRun() { return retryGenerationRun; },
  get workControlCommands() { return workControlCommands; },
});

const { continueGenerationRun, settleGenerationUsage, reconcileGenerationUsage, listGenerationRunsWithUsage, startGenerationRun, retryGenerationRun, submitGenerationIntent } = createGenerationCoordinator({
  get executionContext() { return executionContext; },
  get runtimeRequest() { return runtimeRequest; },
  get acquireWorkFlowOwner() { return acquireWorkFlowOwner; },
  get currentWorkFlowState() { return currentWorkFlowState; },
  get runAdvisoryAgentTask() { return runAdvisoryAgentTask; },
  get runCreativeAgentTask() { return runCreativeAgentTask; },
  get unstartedCreativeAttempts() { return unstartedCreativeAttempts; },
  get workControlCommands() { return workControlCommands; },
});

const { runPlanningReview } = createPlanningReviewCoordinator({
  get executionContext() { return executionContext; },
  get attachAgentWebContents() { return attachAgentWebContents; },
  get executionDomains() { return executionDomains; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get sendAgentMessage() { return sendAgentMessage; },
  get beginAgentExecution() { return beginAgentExecution; },
  get runtimeRequest() { return runtimeRequest; },
  get workControlCommands() { return workControlCommands; },
  get acquireWorkFlowOwner() { return acquireWorkFlowOwner; },
  get currentWorkFlowState() { return currentWorkFlowState; },
  get assertCurrentAgentPlan() { return assertCurrentAgentPlan; },
  get beginRecoveryRun() { return beginRecoveryRun; },
  get sendAgentTurn() { return sendAgentTurn; },
  get bindCodexTurn() { return bindCodexTurn; },
  get finishAgentExecution() { return finishAgentExecution; },
});

const { runAdvisoryAgentTask } = createAdvisoryCoordinator({
  get multimediaHost() { return multimediaHost; },
  get runtimeRequest() { return runtimeRequest; },
  get executionContext() { return executionContext; },
  get attachAgentWebContents() { return attachAgentWebContents; },
  get executionDomains() { return executionDomains; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get sendAgentMessage() { return sendAgentMessage; },
  get beginAgentExecution() { return beginAgentExecution; },
  get executionRequests() { return executionRequests; },
  get conversationMessages() { return conversationMessages; },
  get bindWorkFlowAttempt() { return bindWorkFlowAttempt; },
  get beginRecoveryRun() { return beginRecoveryRun; },
  get sendAgentTurn() { return sendAgentTurn; },
  get bindCodexTurn() { return bindCodexTurn; },
  get unstartedCreativeAttempts() { return unstartedCreativeAttempts; },
  get finishAgentExecution() { return finishAgentExecution; },
});

const { stopRuntime, prepareStartup, initializeStartup, renderMediaHtml } = createDesktopServices({
  get runtimeProcess() { return runtimeProcess; }, set runtimeProcess(value) { runtimeProcess = value; },
  get executionContext() { return executionContext; },
  get mediaExecutionServer() { return mediaExecutionServer; }, set mediaExecutionServer(value) { mediaExecutionServer = value; },
  get conversationMessages() { return conversationMessages; },
  get workExecutionConsumer() { return workExecutionConsumer; },
  get executionDomains() { return executionDomains; },
  get workExecutionOwners() { return workExecutionOwners; },
  get requestRuntimeStartupCheck() { return requestRuntimeStartupCheck; }, set requestRuntimeStartupCheck(value) { requestRuntimeStartupCheck = value; },
  get chapterStructureConsumer() { return chapterStructureConsumer; },
  get clearAgentCallbacks() { return clearAgentCallbacks; },
  get bundledRuntime() { return bundledRuntime; }, set bundledRuntime(value) { bundledRuntime = value; },
  get developmentRuntimeBinaries() { return developmentRuntimeBinaries; },
  get startupBudgetsRecovered() { return startupBudgetsRecovered; }, set startupBudgetsRecovered(value) { startupBudgetsRecovered = value; },
  get recoveryIndex() { return recoveryIndex; }, set recoveryIndex(value) { recoveryIndex = value; },
  get runtimeAssetRoot() { return runtimeAssetRoot; },
  get projectRoot() { return projectRoot; },
  get engineBaseInstructions() { return engineBaseInstructions; }, set engineBaseInstructions(value) { engineBaseInstructions = value; },
  get engineQualificationContextDigest() { return engineQualificationContextDigest; }, set engineQualificationContextDigest(value) { engineQualificationContextDigest = value; },
  get startupPreparation() { return startupPreparation; }, set startupPreparation(value) { startupPreparation = value; },
  get recordAgentEvent() { return recordAgentEvent; },
  get prepareModelTransport() { return prepareModelTransport; },
  get multimediaHost() { return multimediaHost; }, set multimediaHost(value) { multimediaHost = value; },
  get runtimeRequest() { return runtimeRequest; },
  get startRuntime() { return startRuntime; },
  get ensureCodexConfig() { return ensureCodexConfig; },
  get agentCli() { return agentCli; }, set agentCli(value) { agentCli = value; },
  get runtimeUrl() { return runtimeUrl; },
  get runtimeCandidateToken() { return runtimeCandidateToken; },
  get readRecoveryOperationEvidence() { return readRecoveryOperationEvidence; },
  get registerIpcHandlers() { return registerIpcHandlers; },
});

const __dirname = fileURLToPath(new URL(".", import.meta.url));
protocol.registerSchemesAsPrivileged([{ scheme: "creative-media", privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true } }]);
app.setName("小说工坊");
const configuredUserDataDir = process.env.NOVEL_CODEX_USER_DATA_DIR ?? null;
if (configuredUserDataDir !== null) {
  if (!isAbsolute(configuredUserDataDir) || configuredUserDataDir.includes("\0")) throw new Error("NOVEL_CODEX_USER_DATA_DIR 必须是绝对路径");
  app.setPath("userData", configuredUserDataDir);
} else {
  app.setPath("userData", join(app.getPath("appData"), app.getName()));
}
const rendererUrl = process.env.NOVEL_CODEX_RENDERER_URL ?? "http://127.0.0.1:5173";
const parsedRendererUrl = new URL(rendererUrl);
if (parsedRendererUrl.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(parsedRendererUrl.hostname)) throw new Error("NOVEL_CODEX_RENDERER_URL 必须是回环 HTTP 地址");
const configuredRuntimePort = process.env.NOVEL_CODEX_RUNTIME_PORT ? Number(process.env.NOVEL_CODEX_RUNTIME_PORT) : null;
if (configuredRuntimePort !== null && (!Number.isInteger(configuredRuntimePort) || configuredRuntimePort < 1 || configuredRuntimePort > 65_535)) throw new Error("NOVEL_CODEX_RUNTIME_PORT 无效");
let runtimePort = configuredRuntimePort;
let runtimeUrl = "";
const runtimeCandidateToken = randomBytes(32).toString("base64url");
const runtimeAuthorToken = randomBytes(32).toString("base64url");
const projectRoot = join(__dirname, "..");
const runtimeAssetRoot = app.isPackaged ? process.resourcesPath : projectRoot;
const packagedEntry = join(__dirname, "../dist/index.html");
let runtimeProcess;
let bundledRuntime = null;
let runtimeError = null;
let workspaceContext = null;
/** @type {DesktopLifecycle} */
let desktopLifecycle;
let requestRuntimeStartupCheck = () => {};
let businessIpcRegistered = false;
let startupPreparation = null;
let startupBudgetsRecovered = false;

let multimediaHost = null;
let mediaExecutionServer = null;
let agentCli = null;

let configurationRequestEpoch = 0;
let engineBaseInstructions = "";
let engineQualificationContextDigest = null;

const baseExecutionDomain = {
  agentWebContents: null,
  removeAgentWebContentsListener: null,
  agentStorage: null,
  agentProviderConfig: null,
  modelBridge: null,
  activeAgentProjectId: null,
  activeAgentPurpose: null,
  activeAgentTargetKey: null,
  activeRuntimeTask: null,
  activeAgentExecution: null,
  recoveryRefreshPromise: null,
  advisorySteerInFlight: false,
  advisorySteerCompletion: null,
  activeGenerationRunId: null,
  activeWorkFlow: null,
  recoveryJournal: null,
  codexAdapter: new CodexAppServerAdapter({ command: null, cwd: projectRoot, languagePreferences: () => ({ preferredLanguages: app.getPreferredSystemLanguages() }) }),
};
const executionDomains = new ExecutionDomains({
  base: baseExecutionDomain, prepare: prepareExecutionDomain, dispose: disposeExecutionDomain,
  keepAwake: () => {
    // Protect owner renewal while working; display sleep and locking remain available.
    const id = powerSaveBlocker.start(EXECUTION_POWER_POLICY);
    return () => powerSaveBlocker.stop(id);
  },
});
function executionContext() { return executionDomains.current(); }
const executionRequests = new ExecutionRequests();
let messageSendPrompt = null;
const { conversationMessages } = createConversationDelivery({
  get executionDomains() { return executionDomains; },
  get runtimeRequest() { return runtimeRequest; },
  get executionContext() { return executionContext; },
  get desktopLifecycle() { return desktopLifecycle; },
  get runtimeError() { return runtimeError; },
  get modelConnectionGate() { return modelConnectionGate; },
  get workspaceContext() { return workspaceContext; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get executionRequests() { return executionRequests; },
  get withWorkFlow() { return withWorkFlow; },
  get acquireWorkFlowOwner() { return acquireWorkFlowOwner; },
  get runAdvisoryAgentTask() { return runAdvisoryAgentTask; },
  get workInterludes() { return workInterludes; },
  get workExecutionConsumer() { return workExecutionConsumer; },
  get messageSendPrompt() { return messageSendPrompt; },
});

const workInterludes = new WorkInterludes({request:runtimeRequest,messages:conversationMessages,poke:()=>workExecutionConsumer.poke()});
const workControlCommands = new WorkControlCommands({ request: runtimeRequest, path: () => join(app.getPath("userData"), "pending-work-controls.json") });

const modelConnectionGate = new ModelConnectionGate(() => Boolean(executionContext().activeAgentExecution || executionContext().activeWorkFlow || executionContext().activeGenerationRunId));

let recoveryIndex = null;

let rendererWebContents = null;

const { chapterStructureConsumer, workExecutionOwners, workExecutionConsumer } = createAutomaticWorkConsumers({
  get assertTrustedIpcSender() { return assertTrustedIpcSender; },
  get executionContext() { return executionContext; },
  get desktopLifecycle() { return desktopLifecycle; },
  get modelConnectionGate() { return modelConnectionGate; },
  get runtimeRequest() { return runtimeRequest; },
  get executionRequests() { return executionRequests; },
  get executionDomains() { return executionDomains; },
  get withWorkFlow() { return withWorkFlow; },
  get runCreativeAgentTask() { return runCreativeAgentTask; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get workControlCommands() { return workControlCommands; },
  get interruptAgentTurn() { return interruptAgentTurn; },
  get baseExecutionDomain() { return baseExecutionDomain; },
  get runtimeError() { return runtimeError; },
  get runtimeProcess() { return runtimeProcess; },
  get rendererWebContents() { return rendererWebContents; },
  get assertCurrentAgentPlan() { return assertCurrentAgentPlan; },
  get prepareWorkExecutionCondition() { return prepareWorkExecutionCondition; },
  get workspaceContext() { return workspaceContext; },
  get conversationMessages() { return conversationMessages; },
  get assertThreadProject() { return assertThreadProject; },
  get resumeWorkTask() { return resumeWorkTask; },
});

const { trustedIpcMain, registerIpcHandlers } = createTrustedIpcRegistry({
  get assertTrustedIpcSender() { return assertTrustedIpcSender; },
  get businessIpcRegistered() { return businessIpcRegistered; }, set businessIpcRegistered(value) { businessIpcRegistered = value; },
  get executionIdentity() { return executionIdentity; },
  get executionDomains() { return executionDomains; },
  get controlDomain() { return controlDomain; },
  get messageSendPrompt() { return messageSendPrompt; }, set messageSendPrompt(value) { messageSendPrompt = value; },
  get executionRequests() { return executionRequests; },
  get conversationMessages() { return conversationMessages; },
  get workInterludes() { return workInterludes; },
  get runtimeRequest() { return runtimeRequest; },
  get withWorkFlow() { return withWorkFlow; },
  get executionContext() { return executionContext; },
  get currentWorkFlowState() { return currentWorkFlowState; },
  get runCreativeAgentTask() { return runCreativeAgentTask; },
  get runAdvisoryAgentTask() { return runAdvisoryAgentTask; },
  get runQueuedPlanningReview() { return runQueuedPlanningReview; },
  get reconcileGenerationUsage() { return reconcileGenerationUsage; },
  get chapterStructureConsumer() { return chapterStructureConsumer; },
  get workExecutionConsumer() { return workExecutionConsumer; },
  get listGenerationRunsWithUsage() { return listGenerationRunsWithUsage; },
  get workControlCommands() { return workControlCommands; },
  get interruptAgentTurn() { return interruptAgentTurn; },
  get multimediaHost() { return multimediaHost; }, set multimediaHost(value) { multimediaHost = value; },
  get workspaceContext() { return workspaceContext; }, set workspaceContext(value) { workspaceContext = value; },
  get assertCurrentAgentPlan() { return assertCurrentAgentPlan; },
  get refreshRecoveryReport() { return refreshRecoveryReport; },
  get modelConnectionGate() { return modelConnectionGate; },
  get prepareModelTransport() { return prepareModelTransport; },
  get projectRoot() { return projectRoot; },
  get runtimeCandidateToken() { return runtimeCandidateToken; },
  get ensureCodexConfig() { return ensureCodexConfig; },
  get engineQualificationContextDigest() { return engineQualificationContextDigest; }, set engineQualificationContextDigest(value) { engineQualificationContextDigest = value; },
  get agentCli() { return agentCli; }, set agentCli(value) { agentCli = value; },
  get engineBaseInstructions() { return engineBaseInstructions; }, set engineBaseInstructions(value) { engineBaseInstructions = value; },
  get configurationRequestEpoch() { return configurationRequestEpoch; }, set configurationRequestEpoch(value) { configurationRequestEpoch = value; },
  get runtimeUrl() { return runtimeUrl; }, set runtimeUrl(value) { runtimeUrl = value; },
  get forwardAgentEvent() { return forwardAgentEvent; },
  get assertThreadProject() { return assertThreadProject; },
  get runCoreTurnWithLease() { return runCoreTurnWithLease; },
  get candidateActionDependencies() { return candidateActionDependencies; },
  get resumeWorkTask() { return resumeWorkTask; },
  get runCandidateWorkflow() { return runCandidateWorkflow; },
  get startGenerationRun() { return startGenerationRun; },
  get continueGenerationRun() { return continueGenerationRun; },
  get retryGenerationRun() { return retryGenerationRun; },
  get submitGenerationIntent() { return submitGenerationIntent; },
  get continueCreativeWork() { return continueCreativeWork; },
  get modelChapterKnowledge() { return modelChapterKnowledge; },
  get runPlanningReview() { return runPlanningReview; },
  get baseExecutionDomain() { return baseExecutionDomain; },
  get sendAgentMessage() { return sendAgentMessage; },
});

desktopLifecycle = new DesktopLifecycle({
  app, windows: () => BrowserWindow.getAllWindows(), registry: trustedIpcMain,
  initialize: initializeStartup, cleanup: stopRuntime, check: () => requestRuntimeStartupCheck(),
  createWindow, readHealth: () => runtimeRequest("/health"),
});
desktopLifecycle.start();
