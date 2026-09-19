const { contextBridge } = require("electron");
const { ipcRenderer } = require("electron");

const invoke = (channel) => (...args) => ipcRenderer.invoke(channel, ...args);

contextBridge.exposeInMainWorld("novelCodex", {
  platform: process.platform,
  version: "0.1.0",
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
    listProjects: invoke("runtime:list-projects"),
    createProject: invoke("runtime:create-project"),
    getState: invoke("runtime:get-state"),
    getLocalMetrics: invoke("runtime:metrics"),
    recordStageAbandonment: invoke("runtime:metrics-abandon"),
    exportProject: invoke("runtime:export-project"),
    getCreativeWorkspace: invoke("runtime:get-creative-workspace"),
    cancelPlanningReview: invoke("runtime:cancel-planning-review"),
    setWorkingBaseline: invoke("runtime:set-working-baseline"),
    clearWorkingBaseline: invoke("runtime:clear-working-baseline"),
    saveContinuationIntent: invoke("runtime:save-continuation-intent"),
    getWriterPreview: invoke("runtime:get-writer-preview"),
    listPlanningImpacts: invoke("runtime:list-planning-impacts"),
    saveArtifact: invoke("runtime:save-artifact"),
    materializeChapterPlan: invoke("runtime:materialize-chapter-plan"),
    materializeStandaloneChapter: invoke("runtime:materialize-standalone-chapter"),
    associateChapterPlan: invoke("runtime:associate-chapter-plan"),
    resolvePlanningImpact: invoke("runtime:resolve-planning-impact"),
    setArtifactLock: invoke("runtime:set-artifact-lock"),
    saveDraftCheckpoint: invoke("runtime:save-draft-checkpoint"),
    createReview: invoke("runtime:create-review"),
    acceptReview: invoke("runtime:accept-review"),
    undoCommit: invoke("runtime:undo-commit"),
    getDiffs: invoke("runtime:get-diffs"),
    runStoryTests: invoke("runtime:story-tests"),
    search: invoke("runtime:search"),
  },
  agent: {
    status: invoke("agent:status"),
    configure: invoke("agent:configure"),
    sendPrompt: invoke("agent:send-prompt"),
    generatePlanning: invoke("agent:generate-planning"),
    generateChapter: invoke("agent:generate-chapter"),
    reviewPlanning: invoke("agent:review-planning"),
    steer: invoke("agent:steer"),
    interrupt: invoke("agent:interrupt"),
    listThreads: invoke("agent:list-threads"),
    resumeThread: invoke("agent:resume-thread"),
    reconnectThread: invoke("agent:reconnect-thread"),
    readThread: invoke("agent:read-thread"),
    reviewThread: invoke("agent:review-thread"),
    compactThread: invoke("agent:compact-thread"),
    listBackgroundTerminals: invoke("agent:list-background-terminals"),
    terminateBackgroundTerminal: invoke("agent:terminate-background-terminal"),
    setThreadPinned: invoke("agent:set-thread-pinned"),
    archiveThread: invoke("agent:archive-thread"),
    unarchiveThread: invoke("agent:unarchive-thread"),
    resolveUserInput: invoke("agent:resolve-user-input"),
    onEvent: (listener) => {
      const handler = (_event, payload) => listener(payload);
      ipcRenderer.on("agent:event", handler);
      return () => ipcRenderer.removeListener("agent:event", handler);
    },
    onUserInputRequest: (listener) => {
      const handler = (_event, payload) => listener(payload);
      ipcRenderer.on("agent:user-input-request", handler);
      return () => ipcRenderer.removeListener("agent:user-input-request", handler);
    },
  },
});
