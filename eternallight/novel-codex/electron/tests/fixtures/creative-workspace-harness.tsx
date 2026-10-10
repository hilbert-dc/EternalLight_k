import { CreativeWorkspaceView as Workspace } from "../../../src/CreativeWorkspaceView";
import { useWorkTaskList } from "../../../src/useWorkTaskList";
import { useWorkTaskControls } from "../../../src/useWorkTaskControls";
import { useEffect, useState } from "react";
import type { ChapterWritingRequest } from "../../../src/chapter-writing";

export let preparedChapterWriting: ChapterWritingRequest | null = null;
export async function prepareWritingRequest() {
  const entries = [...document.querySelectorAll<HTMLButtonElement>('[aria-label="让 AI 起草本章"]')];
  const button = entries[0];
  if (entries.length !== 1 || !button?.closest(".chapter-document") || button.matches(":disabled")) throw new Error("Chapter writing preparation is unavailable");
  button.click();
  await new Promise(resolve => setTimeout(resolve, 30));
  if (!preparedChapterWriting) throw new Error("No chapter writing request was prepared");
  return preparedChapterWriting;
}

export async function revealControl(element: Element | null) {
  if (!element) return;
  const ancestors: HTMLDetailsElement[] = [];
  for (let parent = element.parentElement; parent; parent = parent.parentElement) {
    if (parent instanceof HTMLDetailsElement) ancestors.unshift(parent);
  }
  for (const details of ancestors) if (!details.open) {
    details.querySelector("summary")?.click();
    await new Promise(resolve => setTimeout(resolve, 30));
  }
}

/** Fixture shell owns the same project reader as the production App. */
export function CreativeWorkspaceView(props) {
  const taskList = useWorkTaskList(props.api, props.projectId, props.refreshToken, props.agentRunning, true);
  const [error, setError] = useState(null), [notice, setNotice] = useState(null), [dirty, setDirty] = useState(false);
  const taskControls = useWorkTaskControls({ scope: taskList.scope,
    isBlocked: () => dirty || !!taskList.error || !taskList.loaded,
    refresh: taskList.refresh, onFormalChange: props.onFormalChange,
    onError: setError, onNotice: setNotice });
  useEffect(() => {
    if (!taskControls.resuming) return;
    taskList.poll();
    const timer = window.setInterval(taskList.poll, 1500);
    return () => window.clearInterval(timer);
  }, [taskControls.resuming, taskList.poll]);
  return <>{error && <p className="creative-alert error" role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}<Workspace {...props} taskList={taskList} taskControls={taskControls}
    onDirtyChange={value => { setDirty(value); props.onDirtyChange?.(value); }}
    onPrepareChapterWriting={props.onPrepareChapterWriting ?? (request => { preparedChapterWriting = request; })} /></>;
}
