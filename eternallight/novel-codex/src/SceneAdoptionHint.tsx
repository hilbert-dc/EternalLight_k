export function SceneAdoptionHint({ sceneIds, selectedIds, hasFormalDraft }: {
  sceneIds: string[]; selectedIds: string[]; hasFormalDraft: boolean;
}) {
  const selectedCount = sceneIds.filter(id => selectedIds.includes(id)).length;
  const message = selectedCount === 0 ? "尚未选择场景，不能采用。"
    : selectedCount === sceneIds.length ? "已全选：将整章采用候选稿，包括标题、章纲来源、场景顺序和删除。"
      : hasFormalDraft ? "仅采用已选场景，保留未选正式场景及原标题、章纲来源；未选候选内容不会采用。"
        : "尚无正式稿：仅采用已选场景及候选标题，使用独立来源、不关联章纲；未选候选内容不会采用。";
  return <p className="scene-adoption-hint" role="status" aria-label="本次采用范围">{message}</p>;
}
