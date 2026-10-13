import type { ReviewUiSnapshot } from './creative-workspace-types';

export function emptyReviewUi(sceneGroupIds: string[] = []): ReviewUiSnapshot {
  return {
    factDecisions: {},
    noConfirmedChanges: false,
    factDependencyTargets: {},
    directionChangeDecisions: {},
    manualDirectionDecisions: {},
    selectedSceneGroups: sceneGroupIds,
  };
}

export function readReviewUi(key: string, fallback: ReviewUiSnapshot): ReviewUiSnapshot {
  try {
    const stored = localStorage.getItem(key);
    if (!stored) return fallback;
    return { ...fallback, ...JSON.parse(stored) as Partial<ReviewUiSnapshot> };
  } catch {
    return fallback;
  }
}

export function writeReviewUi(key: string, snapshot: ReviewUiSnapshot) {
  try { localStorage.setItem(key, JSON.stringify(snapshot)); } catch { /* Local recovery is best effort. */ }
}

export function removeReviewUi(key: string) {
  try { localStorage.removeItem(key); } catch { /* Local recovery is best effort. */ }
}
