import React from "react";
import { createPortal } from "react-dom";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";

// App callback integration harness; domain editing has separate workspace tests.
export function CreativeWorkspaceView(props) {
  const chapter = { id: "chapter-01", number: 1, title: "夹具章节" };
  const actions = {
    work: () => props.onGenerateWork("继续作品", 1, 1, "snapshot", "a".repeat(64), null),
    continue: () => props.onContinueWork("work"),
    retry: () => props.onRetryWork("work"),
    feedbackContinue: () => props.onResolveFeedback("work", "assessment", "continue", ""),
    feedbackRepair: () => props.onResolveFeedback("work", "assessment", "repairDraft", "调整"),
    chapter: () => props.onGenerateChapter(chapter, "章节候选", { povCharacterId: null, storyTime: null }),
    review: () => props.onReviewPlanning({ authorIntent: "检查大纲", roles: [] }),
    stopReview: () => props.onCancelPlanningReview("review"),
  };
  return <div>{props.taskConversationContainer && createPortal(<WorkTaskPanel conversation={props.conversation}
    api={props.api} projectId={props.projectId} tasks={[]} activeCount={0} nextCursor={null} loadingMore={false}
    loaded reading={false} readError={null} onRetry={() => {}} revision={1} busy={false} interruptionBusy={false}
    agentRunning={false} agentAvailable dirty={false} onControl={() => {}} onUndo={() => {}} onLoadMore={() => {}} />,
    props.taskConversationContainer)}
    {Object.entries(actions).map(([name, action]) => <button key={name} data-execute={name} onClick={() => { Promise.resolve(action()).catch(error => { window.executionError = error.message; }); }}>{name}</button>)}
  </div>;
}
