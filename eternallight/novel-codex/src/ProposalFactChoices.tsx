import type { ProposalSetDetail, NovelCodexApi } from "./types";
import { StatePropertyDetails } from "./StatePropertyDetails";
import { StoryEventDetails } from "./StoryEventDetails";

type Props = { api: NovelCodexApi; projectId: string; options: ProposalSetDetail["reviewOptions"][string]; choices: Record<string, unknown>; disabled: boolean; onChange: (choices: Record<string, unknown>) => void };

export function ProposalFactChoices({ api, projectId, options, choices, disabled, onChange }: Props) {
  const groups = choices?.sceneGroupIds as string[] | undefined ?? [];
  const decisions = choices?.factDecisions as Record<string, string> | undefined ?? {};
  return <fieldset className="proposal-fact-choices" disabled={disabled}>
    <legend>确认事实与来源场景（正文整份采用）</legend>
    {options.sceneGroupIds.map((group) => <label key={group}><input type="checkbox" checked={groups.includes(group)} onChange={(event) => {
      const next = { ...decisions };
      if (!event.target.checked) for (const fact of options.factProposals) if (fact.sceneGroupId === group && next[fact.id] === "confirmed") next[fact.id] = "candidate";
      onChange({ sceneGroupIds: event.target.checked ? [...groups, group] : groups.filter((id) => id !== group), factDecisions: next });
    }} />事实来源场景 · {group}</label>)}
    {options.factProposals.map((fact) => <div key={fact.id} className="proposal-set-fact">
      <span>{fact.subjectId ?? "未指定主体"} · {fact.predicate}{!fact.storyEvent && ` = ${JSON.stringify(fact.value)}`}<StatePropertyDetails property={fact.stateProperty} /><StoryEventDetails event={fact.storyEvent} api={api} projectId={projectId} /><small>原文依据：{fact.evidence}</small></span>
      <select aria-label={`事实决定 ${fact.id}`} value={decisions[fact.id] ?? "candidate"} onChange={(event) => onChange({ sceneGroupIds: groups, factDecisions: { ...decisions, [fact.id]: event.target.value } })}>
        <option value="candidate">保留候选（不确认）</option><option value="rejected">拒绝</option><option value="confirmed" disabled={!groups.includes(fact.sceneGroupId)}>确认事实</option>
      </select>
    </div>)}
  </fieldset>;
}
