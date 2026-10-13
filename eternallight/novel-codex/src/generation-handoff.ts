import type { GenerationHandoffProposal } from "./types";
import { MAX_GENERATION_CHAPTERS } from "../shared/generation-permit.mjs";

// Both immediate replies and queued completion notifications carry the same
// Native proposal. A notification cannot authorize or retarget that proposal.
export function boundGenerationHandoff(value: unknown, owner: {
  projectId: unknown; threadId: unknown; taskId: unknown; authorIntent: unknown;
}): GenerationHandoffProposal | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const proposal = value as Record<string, unknown>;
  if (proposal.resumeRun || proposal.state !== "proposed"
    || proposal.projectId !== owner.projectId || proposal.threadId !== owner.threadId
    || proposal.sourceTaskId !== owner.taskId || proposal.authorIntent !== owner.authorIntent
    || !["proposalId", "projectId", "threadId", "sourceTaskId", "authorIntent", "sourceDigest", "baseSnapshotId"]
      .every(key => typeof proposal[key] === "string" && (proposal[key] as string).trim().length > 0)
    || !Number.isSafeInteger(proposal.expectedRevision) || (proposal.expectedRevision as number) < 0
    || !Number.isSafeInteger(proposal.targetChapters) || (proposal.targetChapters as number) < 1
    || (proposal.targetChapters as number) > MAX_GENERATION_CHAPTERS) return null;
  return proposal as unknown as GenerationHandoffProposal;
}
