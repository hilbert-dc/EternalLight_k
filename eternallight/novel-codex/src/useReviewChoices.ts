import { useMemo, useState, type Dispatch, type SetStateAction } from "react";
import type { ReviewUiSnapshot } from "./creative-workspace-types";
import { emptyReviewUi } from "./review-ui-storage";

/** Author choices are one recoverable UI snapshot, separate from Native publication facts. */
export function useReviewChoices() {
  const [reviewUiSnapshot, restoreReviewUi] = useState(emptyReviewUi);
  const setters = useMemo(() => {
    const field = <K extends keyof ReviewUiSnapshot>(key: K): Dispatch<SetStateAction<ReviewUiSnapshot[K]>> => update => {
      restoreReviewUi(previous => ({ ...previous, [key]: typeof update === "function"
        ? (update as (value: ReviewUiSnapshot[K]) => ReviewUiSnapshot[K])(previous[key]) : update }));
    };
    return {
      setFactDecisions: field("factDecisions"),
      setNoConfirmedChanges: field("noConfirmedChanges"),
      setFactDependencyTargets: field("factDependencyTargets"),
      setDirectionChangeDecisions: field("directionChangeDecisions"),
      setManualDirectionDecisions: field("manualDirectionDecisions"),
      setSelectedSceneGroups: field("selectedSceneGroups"),
    };
  }, []);
  return { ...reviewUiSnapshot, ...setters, reviewUiSnapshot, restoreReviewUi };
}
