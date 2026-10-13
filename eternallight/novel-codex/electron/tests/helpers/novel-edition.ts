import type { CreativeEdition, MediaProject } from "../../../shared/multimedia-types";

/** Explicit current-contract novel identity for App fixtures. */
export function fixtureNovelEdition(projectId: string): CreativeEdition {
  return { id: `edition-${projectId}`, editionId: `edition-${projectId}`, projectId, scopeId: `scope-${projectId}`, branchId: `branch-${projectId}`, medium: "novel", title: "小说", canonSnapshotId: null, sources: [] };
}
export const novelFixtureMedia = { listEditions: async (projectId: string) => ({ editions: [fixtureNovelEdition(projectId)] }) };
export function fixtureCreatedNovel(project: MediaProject) {
  const edition = fixtureNovelEdition(project.id);
  return { project, editions: [edition], activeEditionId: edition.id };
}
