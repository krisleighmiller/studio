import type { StageId } from "../workflowTypes";

export type WorkAreaId = "plan" | "produce" | "finish";

/** Ephemeral desk selection. Durable identity is the stable id, not the display number. */
export type DirectorSelection = {
  sceneId: number | null;
  sceneStableId: string | null;
  cut: number | null;
  shotStableId: string | null;
  assetId: string | null;
  lookSlotId: string | null;
  requirementId: string | null;
  workArea: WorkAreaId | null;
  stage: StageId | null;
};

export function scopeKey(kind: string, id: string): string {
  return `${kind}:${id}`;
}

/** Drop entries the user removed from the next Director turn. Project state stays put. */
export function visibleDirectorSelection(
  selection: DirectorSelection,
  suppressed: ReadonlySet<string>,
): DirectorSelection {
  const hidden = (kind: string, id: string | null) =>
    Boolean(id && suppressed.has(scopeKey(kind, id)));
  const sceneHidden = hidden("scene", selection.sceneStableId);
  const shotHidden = hidden("shot", selection.shotStableId);
  const areaHidden = hidden("workArea", selection.workArea);
  return {
    ...selection,
    sceneId: sceneHidden ? null : selection.sceneId,
    sceneStableId: sceneHidden ? null : selection.sceneStableId,
    cut: shotHidden ? null : selection.cut,
    shotStableId: shotHidden ? null : selection.shotStableId,
    assetId: hidden("asset", selection.assetId) ? null : selection.assetId,
    lookSlotId: hidden("lookSlot", selection.lookSlotId) ? null : selection.lookSlotId,
    requirementId: hidden("requirement", selection.requirementId) ? null : selection.requirementId,
    workArea: areaHidden ? null : selection.workArea,
    stage: areaHidden ? null : selection.stage,
  };
}
