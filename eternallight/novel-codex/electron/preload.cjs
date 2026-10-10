const { contextBridge } = require("electron");
const { ipcRenderer } = require("electron");

const invoke = (channel) => async (...args) => {
  const result = await ipcRenderer.invoke(channel, ...args);
  if (result?.executionRequestFailure?.code === "TASK_CANCELLED") {
    // contextBridge drops custom properties on Error; a plain rejection record
    // preserves this execution outcome across the isolated Renderer boundary.
    throw { code: "TASK_CANCELLED", message: result.executionRequestFailure.message };
  }
  if (result?.nativeRequestFailure) {
    const failure = result.nativeRequestFailure;
    throw Object.assign(new Error(failure.message), { nativeFailure: failure, code: failure.code, details: failure.details });
  }
  return result;
};

const executionListeners = new Set();
const invokeExecution = channel => (...args) => {
  const requestId = globalThis.crypto.randomUUID();
  for (const listener of executionListeners) listener({ method: "agent/execution-request", params: { requestId, projectId: args[0] } });
  return invoke(channel)({ requestId, args });
};

contextBridge.exposeInMainWorld("novelAuthorSession", {
  read: invoke("author:read"),
  save: invoke("author:save"),
  signOut: invoke("author:sign-out"),
  clear: invoke("author:clear"),
});

contextBridge.exposeInMainWorld("novelCodex", {
  platform: process.platform,
  version: "0.1.0",
  multimedia: {
    listEditions: invoke("media:list-editions"),
    createEdition: invoke("media:create-edition"),
    readWorkspace: invoke("media:read-workspace"),
    readTextPage: invoke("media:read-text-page"),
    readSubtitleFont: invoke("media:read-subtitle-font"),
    saveDocument: invoke("media:save-document"),
    patchDocument: invoke("media:patch-document"),
    readCanon: invoke("media:read-canon"),
    saveCanon: invoke("media:save-canon"),
    sourceImpacts: invoke("media:source-impacts"),
    importFiles: invoke("media:import-files"),
    exportEdition: invoke("media:export-edition"),
    openExport: invoke("media:open-export"),
    listConnections: invoke("media:list-connections"),
    saveConnection: invoke("media:save-connection"),
  },
  app: {
    confirmClose: invoke("app:confirm-close"),
    cancelClose: invoke("app:cancel-close"),
    onCloseRequested: (listener) => {
      const handler = () => listener();
      ipcRenderer.on("app:close-requested", handler);
      return () => ipcRenderer.removeListener("app:close-requested", handler);
    },
  },
  runtime: {
    status: invoke("runtime:status"),
    retryStartup: () => ipcRenderer.invoke("runtime:retry-startup"),
    listProjects: invoke("runtime:list-projects"),
    createProject: invoke("runtime:create-project"),
    getState: invoke("runtime:get-state"),
    getLocalMetrics: invoke("runtime:metrics"),
    recordStageAbandonment: invoke("runtime:metrics-abandon"),
    exportProject: invoke("runtime:export-project"),
    getCreativeWorkspace: invoke("runtime:get-creative-workspace"),
    getQualityNotebook: invoke("runtime:get-quality-notebook"),
    changeQualityNotebook: invoke("runtime:change-quality-notebook"),
    getQualityPacket: invoke("runtime:get-quality-packet"),
    getQualityReview: invoke("runtime:get-quality-review"),
    reviewQualityPacket: invokeExecution("agent:review-quality-packet"),
    listProposalSets: invoke("runtime:list-proposal-sets"),
    createProposalSet: invoke("runtime:create-proposal-set"),
    getProposalSet: invoke("runtime:get-proposal-set"),
    previewProposalSet: invoke("runtime:preview-proposal-set"),
    applyProposalSet: invoke("runtime:apply-proposal-set"),
    undoProposalSet: invoke("runtime:undo-proposal-set"),
    getImpactAnalysis: invoke("runtime:get-impact-analysis"),
    getHistoricalKnowledgeState: invoke("runtime:get-historical-knowledge-state"),
    createHypothesis: invoke("runtime:create-hypothesis"),
    getHypothesis: invoke("runtime:get-hypothesis"),
    getIdentityCatalog: invoke("runtime:get-identity-catalog"),
    getCharacterKnowledge: invoke("runtime:get-character-knowledge"),
    saveIdentityDecisions: invoke("runtime:save-identity-decisions"),
    savePropertyRules: invoke("runtime:save-property-rules"),
    listGenerationRuns: invoke("runtime:list-generation-runs"),
    previewGenerationReplan: invoke("runtime:preview-generation-replan"),
    replanGeneration: invoke("runtime:replan-generation"),
    getSceneAssessment: invoke("runtime:get-scene-assessment"),
    getCommitmentExtraction: (projectId, chapterId, reportRevisionId = null) => ipcRenderer.invoke("runtime:get-commitment-extraction", projectId, chapterId, reportRevisionId),
    establishCommitment: invoke("runtime:establish-commitment"),
    getCommitment: invoke("runtime:get-commitment"),
    listCommitments: invoke("runtime:list-commitments"),
    saveCommitment: invoke("runtime:save-commitment"),
    createCommitmentReview: invoke("runtime:create-commitment-review"),
    listWorkTasks: invoke("runtime:list-work-tasks"),
    getSearchAudit: invoke("runtime:get-search-audit"),
    startSearchAudit: invoke("runtime:start-search-audit"),
    controlSearchAudit: invoke("runtime:control-search-audit"),
    getEntityAudit: invoke("runtime:get-entity-audit"),
    startEntityAudit: invoke("runtime:start-entity-audit"),
    controlEntityAudit: invoke("runtime:control-entity-audit"),
    getRelationAudit: invoke("runtime:get-relation-audit"),
    startRelationAudit: invoke("runtime:start-relation-audit"),
    controlRelationAudit: invoke("runtime:control-relation-audit"),
    getKnowledgeAudit: invoke("runtime:get-knowledge-audit"),
    startConditionEvaluation: invoke("runtime:start-condition-evaluation"),
    getConditionEvaluation: invoke("runtime:get-condition-evaluation"),
    controlConditionEvaluation: invoke("runtime:control-condition-evaluation"),
    getConditionExplanations: invoke("runtime:get-condition-explanations"),
    startKnowledgeAudit: invoke("runtime:start-knowledge-audit"),
    controlKnowledgeAudit: invoke("runtime:control-knowledge-audit"),
    getSnapshotKnowledgeAudit: invoke("runtime:get-snapshot-knowledge-audit"),
    startSnapshotKnowledgeAudit: invoke("runtime:start-snapshot-knowledge-audit"),
    controlSnapshotKnowledgeAudit: invoke("runtime:control-snapshot-knowledge-audit"),
    getFactAudit: invoke("runtime:get-fact-audit"),
    startFactAudit: invoke("runtime:start-fact-audit"),
    controlFactAudit: invoke("runtime:control-fact-audit"),
    getWorkTask: invoke("runtime:get-work-task"),
    getWorkIssues: invoke("runtime:get-work-issues"),
    getTaskFinalReply: invoke("runtime:get-task-final-reply"),
    getRevisionOutcome: invoke("runtime:get-revision-outcome"),
    decideRevisionOutcome: invoke("runtime:decide-revision-outcome"),
    getPlanningReview: invoke("runtime:get-planning-review"),
    cancelPlanningReview: invoke("runtime:cancel-planning-review"),
    setWorkingBaseline: invoke("runtime:set-working-baseline"),
    clearWorkingBaseline: invoke("runtime:clear-working-baseline"),
    saveContinuationIntent: invoke("runtime:save-continuation-intent"),
    getWriterPreview: invoke("runtime:get-writer-preview"),
    listPlanningImpacts: invoke("runtime:list-planning-impacts"),
    saveArtifact: invoke("runtime:save-artifact"),
    materializeChapterPlan: invoke("runtime:materialize-chapter-plan"),
    getWorkCatalog: invoke("runtime:get-work-catalog"),
    updateWorkCatalog: invoke("runtime:update-work-catalog"),
    materializeStandaloneChapter: invoke("runtime:materialize-standalone-chapter"),
    deleteChapter: invoke("runtime:delete-chapter"),
    listChapterStructureChanges: invoke("runtime:list-chapter-structure-changes"),
    readChapterStructureChange: invoke("runtime:read-chapter-structure-change"),
    associateChapterPlan: invoke("runtime:associate-chapter-plan"),
    rebindChapterPlan: invoke("runtime:rebind-chapter-plan"),
    resolvePlanningImpact: invoke("runtime:resolve-planning-impact"),
    setArtifactLock: invoke("runtime:set-artifact-lock"),
    saveDraftCheckpoint: invoke("runtime:save-draft-checkpoint"),
    createReview: invoke("runtime:create-review"),
    rejectIdentityReview: invoke("runtime:reject-identity-review"),
    acceptReview: invoke("runtime:accept-review"),
    undoCommit: invoke("runtime:undo-commit"),
    getDiffs: invoke("runtime:get-diffs"),
    search: invoke("runtime:search"),
  },
  agent: {
    listExecutions: invoke("agent:list-executions"),
    beginInterlude: invoke("agent:begin-interlude"),
    returnInterlude: invoke("agent:return-interlude"),
    readMessage: invoke("agent:read-message"),
    listMessages: invoke("agent:list-messages"),
    submitMessage: invoke("agent:submit-message"),
    changeMessage: invoke("agent:change-message"),
    setWorkspaceContext: invoke("agent:set-workspace-context"),
    retryChapterStructureCheck: invoke("agent:retry-chapter-structure-check"),
    status: invoke("agent:status"),
    recoveryReport: invoke("agent:recovery-report"),
    listConnections: invoke("agent:list-connections"),
    verifyConnection: invoke("agent:verify-connection"),
    verifyImageInput: invoke("agent:verify-image-input"),
    verifyConnectionEdge: invoke("agent:verify-connection-edge"),
    configure: invoke("agent:configure"),
    setReasoningEffort: invoke("agent:set-reasoning-effort"),
    sendPrompt: (projectId, prompt, revisionRepairSource = null, conversationFocus = null, threadId = null, mediaInput = null) => invokeExecution("agent:send-prompt")(projectId, prompt, revisionRepairSource, conversationFocus, threadId, mediaInput),
    listProposedActions: invoke("agent:list-proposed-actions"),
    readActionPermission: invoke("agent:read-action-permission"),
    readExecutionScope: invoke("agent:read-execution-scope"),
    previewExecutionScope: invoke("agent:preview-execution-scope"),
    confirmExecutionScope: invoke("agent:confirm-execution-scope"),
    approveAction: invokeExecution("agent:approve-action"),
    declineAction: invoke("agent:decline-action"),
    generatePlanning: invokeExecution("agent:generate-planning"),
    generateWork: invokeExecution("agent:generate-work"),
    continueWork: invokeExecution("agent:continue-work"),
    retryWork: invokeExecution("agent:retry-work"),
    resolveGenerationFeedback: invokeExecution("agent:resolve-generation-feedback"),
    addFutureIntent: invoke("agent:add-future-intent"),
    reviseCurrentChapter: invoke("agent:revise-current-chapter"),
    generateChapter: invokeExecution("agent:generate-chapter"),
    resumeKnowledge: invokeExecution("agent:resume-knowledge"),
    resumeWorkTask: invokeExecution("agent:resume-work-task"),
    recoverWorkTask: invokeExecution("agent:recover-work-task"),
    controlWorkTask: invoke("agent:control-work-task"),
    reviewPlanning: invokeExecution("agent:review-planning"),
    steer: invoke("agent:steer"),
    interrupt: invoke("agent:interrupt"),
    listThreads: invoke("agent:list-threads"),
    createThread: invoke("agent:create-thread"),
    renameThread: invoke("agent:rename-thread"),
    readConversation: invoke("agent:read-conversation"),
    resumeThread: invoke("agent:resume-thread"),
    reconnectThread: invoke("agent:reconnect-thread"),
    readThread: invoke("agent:read-thread"),
    updateThreadReadState: invoke("agent:update-thread-read-state"),
    reviewThread: invokeExecution("agent:review-thread"),
    compactThread: invokeExecution("agent:compact-thread"),
    listBackgroundTerminals: invoke("agent:list-background-terminals"),
    terminateBackgroundTerminal: invoke("agent:terminate-background-terminal"),
    setThreadPinned: invoke("agent:set-thread-pinned"),
    archiveThread: invoke("agent:archive-thread"),
    unarchiveThread: invoke("agent:unarchive-thread"),
    resolveUserInput: invoke("agent:resolve-user-input"),
    onEvent: (listener) => {
      const handler = (_event, payload) => listener(payload);
      executionListeners.add(listener);
      ipcRenderer.on("agent:event", handler);
      return () => { executionListeners.delete(listener); ipcRenderer.removeListener("agent:event", handler); };
    },
    onUserInputRequest: (listener) => {
      const handler = (_event, payload) => listener(payload);
      ipcRenderer.on("agent:user-input-request", handler);
      return () => ipcRenderer.removeListener("agent:user-input-request", handler);
    },
  },
});
