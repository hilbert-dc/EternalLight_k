import { useRef, useState, type ComponentProps, type ReactNode } from 'react';
import { ConversationTools } from './ConversationTools';
import { AgentInspector } from './AgentInspector';
import { PaneSash } from './WorkspaceLayout';

type Props = {
  identity: { id: string | null; title: string; scope: string };
  tools: Omit<ComponentProps<typeof ConversationTools>, 'scope' | 'container' | 'searchContainer' | 'inspectorOpen' | 'onInspector'>;
  status: ReactNode;
  reading: ReactNode;
  composer: ReactNode;
  viewingExecution: boolean;
  inspector: Omit<ComponentProps<typeof AgentInspector>, 'contextTab' | 'onContextTabChange'>;
  inspectorSize: { height: number; max: number; resize: (height: number) => void };
};
/** Owns the conversation's reading/search/inspector surface; keeps the composer mounted. */
export function ConversationPanel({ identity, tools, status, reading, composer, viewingExecution, inspector, inspectorSize }: Props) {
  const readingArea = useRef<HTMLDivElement>(null);
  const [searchContainer, setSearchContainer] = useState<HTMLDivElement | null>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [contextTab, setContextTab] = useState<ComponentProps<typeof AgentInspector>['contextTab']>('story');
  return <aside className="conversation-rail" data-conversation-id={identity.id ?? ''} aria-label="创作引擎协作栏">
    <header className="conversation-shell-header"><strong title={identity.title}>{identity.title}</strong>
      <ConversationTools key={identity.scope} {...tools} scope={identity.scope} container={readingArea}
        searchContainer={searchContainer} inspectorOpen={inspectorOpen} onInspector={() => setInspectorOpen(open => !open)} />
    </header>
    {status}
    <div className="conversation-search-slot" ref={setSearchContainer} />
    <div className="conversation-reading-area" ref={readingArea}>{reading}</div>
    {composer}
    {inspectorOpen && viewingExecution && <PaneSash orientation="horizontal" label="调整功能区高度" value={inspectorSize.height}
      min={88} max={inspectorSize.max} nextValue={(start, _dx, dy) => start - dy} onResize={inspectorSize.resize} />}
    <div className="conversation-context" id="conversation-inspector" hidden={!inspectorOpen || !viewingExecution}>
      <AgentInspector {...inspector} contextTab={contextTab} onContextTabChange={setContextTab} />
    </div>
  </aside>;
}
