// @ts-check
function groupByProject(items) {
  const projects = new Map();
  for (const item of items) {
    if (!item.projectId) continue;
    const projectItems = projects.get(item.projectId) ?? [];
    projectItems.push(item);
    projects.set(item.projectId, projectItems);
  }
  return projects;
}

function batches(items, size = 100) {
  return Array.from({ length: Math.ceil(items.length / size) }, (_, index) => items.slice(index * size, (index + 1) * size));
}

async function readSnapshot(runtimeRequest, projectId, taskIds) {
  let fence = null, cursor = 0, tasks = null;
  const operations = [];
  do {
    const snapshot = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/recovery-snapshot`, {
      method: "POST", body: JSON.stringify({ taskIds, ...(fence ? { fence, cursor } : {}) }),
    });
    if (fence && snapshot.fence !== fence) throw new Error("Runtime changed recovery fence during pagination");
    fence = snapshot.fence;
    tasks ??= snapshot.tasks;
    operations.push(...snapshot.operations);
    if (snapshot.complete) break;
    if (!Number.isInteger(snapshot.nextCursor) || snapshot.nextCursor <= cursor) throw new Error("Runtime returned an invalid recovery cursor");
    cursor = snapshot.nextCursor;
  } while (true);
  return { fence, tasks, operations };
}

export async function reconcileRecoveryIndex(index, runtimeRequest, onError = (_error, _run, _stage) => {}, readOperationEvidence = null) {
  const unfinishedRuns = await index.unfinishedRuns();
  for (const [projectId, projectRuns] of groupByProject(unfinishedRuns)) {
    for (const batch of batches(projectRuns)) {
      try {
        const taskIds = batch.map((run) => run.runId);
        let snapshot = await readSnapshot(runtimeRequest, projectId, taskIds);
        if (readOperationEvidence) {
          const terminalTasks = new Set(snapshot.tasks.filter(task => ["completed", "failed", "cancelled"].includes(task.status)).map(task => task.id));
          let attempted = false;
          // One bounded pass. Failed or insufficient evidence never dispatches a model turn.
          for (const operation of snapshot.operations.filter(operation => operation.state === "accepted" && terminalTasks.has(operation.run_id)).slice(0, 100)) {
            try {
              const evidence = await readOperationEvidence({ projectId, taskId: operation.run_id,
                threadId: operation.thread_id, turnId: operation.turn_id, callId: operation.tool_call_id,
                toolName: operation.tool_name, requestHash: operation.request_hash });
              attempted = true;
              await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/recovery-operations/reconcile`, {
                method: "POST", body: JSON.stringify(evidence),
              });
            } catch {
              // No request bodies or private history enter recovery diagnostics.
              onError(projectId, "operation", new Error("Original operation evidence or Native reconciliation unavailable"));
            }
          }
          // Even a lost POST response may have changed the authoritative ledger.
          // Failure here leaves reconciliation required; the old fence is never projected.
          if (attempted) {
            const fresh = await readSnapshot(runtimeRequest, projectId, taskIds);
            if (!fresh.fence || fresh.fence === snapshot.fence) throw new Error("Runtime did not issue a fresh recovery fence after reconciliation");
            snapshot = fresh;
          }
        }
        const { fence, tasks, operations } = snapshot;
        const operationsByRun = Map.groupBy(operations, (operation) => operation.run_id);
        for (const run of batch) await index.observeRuntimeOperations(run.runId, operationsByRun.get(run.runId) ?? []);
        const tasksById = new Map(tasks.map((task) => [task.id, task]));
        for (const run of batch) {
          const task = tasksById.get(run.runId);
          await index.reconcileRun(run.runId, task?.status ?? "missing", undefined, { fence });
        }
      } catch (error) {
        await index.markReconciliationRequired(batch.map((run) => run.runId), error instanceof Error ? error.message : String(error));
        onError(projectId, "snapshot", error);
      }
    }
  }
}
