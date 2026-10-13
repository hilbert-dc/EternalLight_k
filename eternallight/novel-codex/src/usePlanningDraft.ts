import { useReducer } from "react";
import type { EditableChapterPlan, WorkspaceView } from "./creative-workspace-types";
import type { ChapterPlanPayload, CreativeWorkspace, OutlineNode } from "./types";

function asText(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function newPlanRow(chapterNumber: number): EditableChapterPlan {
  return {
    id: `chapter-plan-${crypto.randomUUID()}`,
    primaryNodeId: "",
    chapterNumber,
    title: "",
    goal: "",
    conflict: "",
    dependencyKeys: "",
    scenes: [],
    locked: false,
    publicCommitments: [],
    allowances: [],
    writerGuidance: "",
    povCharacterId: null,
    storyTime: null,
  };
}

function newOutlineRoot(): OutlineNode { return { clientKey: `book-${crypto.randomUUID()}`, kind: "book", position: 0, title: "全书主线", publicTitle: "", purpose: "", summary: "", dependencyKeys: [], certainty: "open", protection: "free", writerGuidance: "" }; }

function editablePlanRows(payload: Record<string, unknown> | null | undefined): EditableChapterPlan[] {
  if (!payload || !Array.isArray(payload.chapters)) return [];
  return payload.chapters.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const chapter = item as Partial<ChapterPlanPayload>;
    return [{
      id: asText(chapter.chapterPlanId) || `chapter-plan-${chapter.chapterNumber}`,
      chapterPlanId: asText(chapter.chapterPlanId) || undefined,
      primaryNodeId: asText(chapter.primaryNodeId),
      chapterNumber: Number(chapter.chapterNumber) || 1,
      title: asText(chapter.title),
      goal: asText(chapter.goal),
      conflict: asText(chapter.conflict),
      dependencyKeys: Array.isArray(chapter.dependencyKeys) ? chapter.dependencyKeys.join(", ") : "",
      scenes: Array.isArray(chapter.scenes) ? chapter.scenes : [],
      locked: chapter.locked === true,
      publicTitle: asText(chapter.publicTitle),
      writerGuidance: asText(chapter.writerGuidance),
      publicCommitments: Array.isArray(chapter.publicCommitments) ? chapter.publicCommitments : [],
      allowances: Array.isArray(chapter.allowances) ? chapter.allowances : [],
      povCharacterId: asText(chapter.povCharacterId) || null,
      storyTime: Number.isInteger(chapter.storyTime) ? chapter.storyTime : null,
    }];
  });
}


type PlanningDraft = { brief: { premise: string; promise: string; stakes: string; endingDirection: string }; outlineNodes: OutlineNode[]; plans: EditableChapterPlan[]; outlineError: { nodeId: string; message: string } | null };
const initial = (): PlanningDraft => ({ brief: { premise: "", promise: "", stakes: "", endingDirection: "" }, outlineNodes: [newOutlineRoot()], plans: [newPlanRow(1)], outlineError: null });
type Action = { type: "reset" } | { type: "load"; workspace: CreativeWorkspace; view: WorkspaceView }
  | { type: "brief"; key: string; value: string } | { type: "outline"; nodes: OutlineNode[] }
  | { type: "plan"; index: number; change: Partial<EditableChapterPlan> } | { type: "append"; row: EditableChapterPlan }
  | { type: "outlineError"; error: PlanningDraft["outlineError"] };
function reducer(state: PlanningDraft, action: Action): PlanningDraft {
  switch (action.type) {
    case "reset": return initial();
    case "load": {
      const kind = action.view === "plans" ? "plan" : action.view;
      if (kind === "chapters") return state;
      const artifact = action.workspace.artifacts[kind];
      const payload = artifact.candidate?.payload ?? artifact.accepted?.payload;
      if (kind === "brief") return { ...state, brief: { premise: asText(payload?.premise), promise: asText(payload?.promise), stakes: asText(payload?.stakes), endingDirection: asText(payload?.endingDirection) } };
      if (kind === "outline") return { ...state, outlineNodes: payload && Array.isArray(payload.nodes) ? payload.nodes as OutlineNode[] : [newOutlineRoot()], outlineError: null };
      const rows = editablePlanRows(payload);
      return { ...state, plans: rows.length ? rows : [newPlanRow(1)] };
    }
    case "brief": return { ...state, brief: { ...state.brief, [action.key]: action.value } };
    case "outline": return { ...state, outlineNodes: action.nodes, outlineError: null };
    case "plan": return { ...state, plans: state.plans.map((row, index) => index === action.index ? { ...row, ...action.change } : row) };
    case "append": return { ...state, plans: [...state.plans, action.row] };
    case "outlineError": return { ...state, outlineError: action.error };
  }
}
export function usePlanningDraft(onEdit: () => void) {
  const [state, dispatch] = useReducer(reducer, undefined, initial);
  const edit = (action: Action) => { dispatch(action); onEdit(); };
  return { ...state, reset: () => dispatch({ type: "reset" }), load: (workspace: CreativeWorkspace, view: WorkspaceView) => dispatch({ type: "load", workspace, view }),
    editBrief: (key: string, value: string) => edit({ type: "brief", key, value }),
    editOutline: (nodes: OutlineNode[]) => edit({ type: "outline", nodes }),
    editPlan: (index: number, change: Partial<EditableChapterPlan>) => edit({ type: "plan", index, change }),
    appendPlan: () => edit({ type: "append", row: newPlanRow((state.plans.at(-1)?.chapterNumber ?? 0) + 1) }),
    reportOutlineError: (error: PlanningDraft["outlineError"]) => dispatch({ type: "outlineError", error }),
  };
}
