export function CreativeWorkspaceView({ view, selectedChapterId, onDirtyChange, onDiscardHandlerChange }) {
  return <section data-search-view={view} data-search-chapter={selectedChapterId}>
    <button onClick={() => { onDiscardHandlerChange(() => { onDirtyChange(false); return true; }); onDirtyChange(true); }}>标记未保存</button>
  </section>;
}
