// Current read contract for App fixtures whose projects have no saved chapter text.
export const chapterRuntimeReads = (projectFor: (id: string) => object) => ({
  getCreativeWorkspace: async (id: string) => ({ project: projectFor(id), revision: 1, headSnapshotId: "fixture-head", sourceDigest: "a".repeat(64), index: { status: "ready" },
    artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
    chapters: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] }),
  listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  listGenerationRuns: async () => ({ runs: [] }),
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
  getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
});
