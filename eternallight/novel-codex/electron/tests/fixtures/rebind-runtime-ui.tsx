import "../../../src/styles.css";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView } from "./creative-workspace-harness";

// Top-level navigation harness. Editing and confirmations belong to the production workspace controller.
function Journey() {
  const { projectId, chapterId } = window.rebindFixture;
  const api = window.novelCodex;
  const [workspace, setWorkspace] = useState(null);
  const [view, setView] = useState("chapters");
  const [refreshToken, setRefreshToken] = useState(0);
  const [dirty, setDirty] = useState(false);
  const discard = useRef(null);
  const forbidden = async () => { throw new Error("Test did not authorize model execution or unrelated writes"); };
  useEffect(() => { void api.runtime.getCreativeWorkspace(projectId).then(setWorkspace); }, [api, projectId]);
  const refresh = async () => {
    if (window.failFormalRefresh) throw new Error("测试刷新连接中断");
    setWorkspace(await api.runtime.getCreativeWorkspace(projectId)); setRefreshToken(value => value + 1);
  };
  if (!workspace) return <p>正在读取真实作品…</p>;
  return <main style={{ width: "min(980px, 95vw)", margin: "24px auto" }}>
    <nav><button disabled={dirty} onClick={() => setView("chapters")}>章节页</button><button disabled={dirty} onClick={() => setView("brief")}>简报页</button><button disabled={!dirty} onClick={() => discard.current?.()}>放弃未保存输入</button></nav>
    <CreativeWorkspaceView taskConversationContainer={null} api={api} projectId={projectId} view={view}
      state={{ novelEdition: { chapters: workspace.chapters }, canon: { characters: [] } }} selectedChapterId={chapterId}
      agentAvailable agentRunning={false} refreshToken={refreshToken}
      onSelectChapter={() => {}} onGenerateWork={forbidden} onContinueWork={forbidden}
      onRetryWork={forbidden} onResolveFeedback={forbidden} onReviewPlanning={forbidden} onCancelPlanningReview={forbidden}
      onGenerateChapter={forbidden} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}}
      onFormalChange={refresh} onDirtyChange={setDirty} onDiscardHandlerChange={handler => { discard.current = handler; }} />
  </main>;
}
createRoot(document.getElementById("root")).render(<Journey />);
