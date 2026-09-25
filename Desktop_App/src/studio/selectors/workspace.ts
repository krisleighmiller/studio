import { stageStatus, type CutRecord, type SceneRecord, type StageId } from "../../workflowTypes";

export type WorkspaceAction = {
  id: string;
  label: string;
  enabled: boolean;
  reason: string | null;
};

export type WorkspaceModel = {
  artifactType: string;
  stableId: string | null;
  primary: string;
  alternates: string[];
  approval: string;
  dependency: string;
  actions: WorkspaceAction[];
};

export function buildWorkspace(input: {
  scene: SceneRecord | null;
  cut: CutRecord | null;
  stage: StageId | null;
  lookSlotId: string | null;
  busy: string | null;
}): WorkspaceModel {
  if (!input.scene) {
    return {
      artifactType: "None",
      stableId: null,
      primary: "No scene selected",
      alternates: [],
      approval: "unknown",
      dependency: "unknown",
      actions: [],
    };
  }
  const scene = input.scene;
  const stage = input.stage;
  const busyReason = input.busy ? "Wait for the current action to finish." : null;
  if (stage === "scene_look") return lookWorkspace(scene, input.lookSlotId, busyReason);
  if (stage === "take_review" || stage === "render") return takeWorkspace(input.cut, busyReason);
  if (stage === "assemble") return assembleWorkspace(scene, busyReason);
  return {
    artifactType: stage ?? "workspace",
    stableId: input.cut?.stable_id || scene.stable_id || null,
    primary: input.cut ? `Cut ${input.cut.id.cut}` : `Scene ${scene.id}`,
    alternates: [],
    approval: stage ? stageStatus(scene.stages, stage) : "unknown",
    dependency: input.cut?.dependency_state ?? "unknown",
    actions: [],
  };
}

function lookWorkspace(scene: SceneRecord, lookSlotId: string | null, busy: string | null): WorkspaceModel {
  const slots = scene.look_board?.slots ?? [];
  const slot = slots.find((item) => item.id === lookSlotId) ?? slots[0];
  const versions = slot?.asset_ids ?? [];
  const approved = slot?.approved_asset_id ?? null;
  return {
    artifactType: "Look",
    stableId: slot?.id ?? scene.stable_id ?? null,
    primary: approved ? `Approved ${approved}` : "No approved look",
    alternates: versions.filter((id) => id !== approved),
    approval: approved ? "approved" : versions.length ? "pending" : "missing",
    dependency: "unknown",
    actions: [
      action("approve-look", "Approve look", Boolean(slot) && !busy, busy ?? (slot ? null : "Draft a look board first.")),
    ],
  };
}

function takeWorkspace(cut: CutRecord | null, busy: string | null): WorkspaceModel {
  const takes = cut?.takes ?? [];
  const selected = takes.find((take) => take.id === cut?.selected_take || take.path === cut?.selected_take);
  const continuity =
    cut?.visual_source?.strategy === "previous_selected_take_exit_frame" && !cut.visual_source.asset_id;
  return {
    artifactType: "Take",
    stableId: cut?.stable_id ?? null,
    primary: selected?.path ?? "No selected take",
    alternates: takes.filter((take) => take !== selected).map((take) => take.path),
    approval: selected?.status ?? (takes.length ? "unreviewed" : "missing"),
    dependency: cut?.dependency_state ?? "unknown",
    actions: [
      action("select-take", "Select take", takes.length > 0 && !busy, busy ?? (takes.length ? null : "No takes yet.")),
      action(
        "bind-continuity",
        "Bind continuity frame",
        Boolean(continuity) && !busy,
        continuity ? busy : "This cut does not need a continuity frame.",
      ),
    ],
  };
}

function assembleWorkspace(scene: SceneRecord, busy: string | null): WorkspaceModel {
  const versions = scene.post?.versions ?? [];
  const latest = versions[versions.length - 1];
  const missing = scene.cuts.filter((cut) => {
    const take = (cut.takes ?? []).find(
      (candidate) => candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    return !take;
  });
  return {
    artifactType: "Review export",
    stableId: latest?.id ?? scene.stable_id ?? null,
    primary: latest?.assembly_path || latest?.label || "No review export",
    alternates: versions.slice(0, -1).map((version) => version.label),
    approval: latest?.stage ?? scene.stages.assemble ?? "missing",
    dependency: missing.length ? "blocked" : "keep",
    actions: [
      action(
        "assemble",
        "Assemble",
        !busy && missing.length === 0 && scene.cuts.length > 0,
        busy ??
          (scene.cuts.length === 0
            ? "This scene has no cuts."
            : missing.length
              ? `Select a take for ${missing.map((cut) => `Cut ${cut.id.cut}`).join(", ")}.`
              : null),
      ),
      action("open-export", "Open review export", Boolean(latest?.assembly_path) && !busy, latest?.assembly_path ? busy : "No review export yet."),
    ],
  };
}

function action(id: string, label: string, enabled: boolean, reason: string | null): WorkspaceAction {
  return { id, label, enabled, reason: enabled ? null : reason };
}
