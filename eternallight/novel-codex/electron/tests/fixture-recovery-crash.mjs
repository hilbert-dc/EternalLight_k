import { HostRecoveryJournal, RecoveryIndex } from "@eternallight/agent-engine";

const path = process.argv[2];
if (!path) throw new Error("recovery index path is required");

const journal = new HostRecoveryJournal(new RecoveryIndex(path));
await journal.beginRun({ runId: "crashed-task", planId: "plan-1", threadId: "crashed-thread", projectId: "project-1" });
await journal.observe({
  method: "item/started",
  params: {
    threadId: "crashed-thread",
    turnId: "turn-1",
    item: {
      id: "crashed-write",
      type: "mcpToolCall",
      server: "novel_workspace",
      tool: "save_outline",
      arguments: { projectId: "project-1", taskId: "crashed-task", threadId: "crashed-thread",
        idempotencyKey: "crashed-attempt-1", payload: { title: "must not be journaled" }, expectedRevision: 0, expectedCandidateRevisionId: null },
      status: "inProgress",
    },
  },
});
await journal.flush();
process.exit(17);
