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

export async function reconcileRecoveryIndex(index, runtimeRequest, onError = () => {}) {
  const unfinishedRuns = await index.unfinishedRuns();
  for (const [projectId, projectRuns] of groupByProject(unfinishedRuns)) {
    for (const batch of batches(projectRuns)) {
      try {
        const taskIds = batch.map((run) => run.runId);
        let fence = null;
        let cursor = 0;
        let tasks = null;
        const operations = [];
        do {
          const snapshot = await runtimeRequest(`/api/projects/${encodeURIComponent(projectId)}/tasks/recovery-snapshot`, {
            method: "POST",
            body: JSON.stringify({ taskIds, ...(fence ? { fence, cursor } : {}) }),
          });
          if (fence && snapshot.fence !== fence) throw new Error("Runtime changed recovery fence during pagination");
          fence = snapshot.fence;
          tasks ??= snapshot.tasks;
          operations.push(...snapshot.operations);
          if (snapshot.complete) break;
          if (!Number.isInteger(snapshot.nextCursor) || snapshot.nextCursor <= cursor) throw new Error("Runtime returned an invalid recovery cursor");
          cursor = snapshot.nextCursor;
        } while (true);
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
