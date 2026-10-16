import type { CutRecord, ProjectState, SceneRecord } from "../../workflowTypes";
import type { DirectorSelection } from "../selection";

export type DirectorContextEntry = {
  kind: "workArea" | "scene" | "shot" | "asset" | "lookSlot" | "requirement";
  id: string;
  /** Host scope line, without the leading dash. */
  label: string;
  name: string;
  status: string;
};

export type DirectorContextModel = {
  headline: string;
  entries: DirectorContextEntry[];
  scopeText: string;
};

const LOOK_DISPLAY: Record<string, string> = {
  interior_reference: "Interior",
  exterior_reference: "Exterior",
  location_reference: "Location",
  wardrobe_reference: "Wardrobe",
  wardrobe_plate: "Wardrobe plate",
  prop_reference: "Prop",
  mood_reference: "Mood",
  lighting_reference: "Lighting",
  composition_reference: "Composition",
  group_reference: "Group",
  // Audio roles, kept byte-identical to the host's AssetRole::display_label
  // (host/src/workflow/state.rs) so the Director never sees two spellings.
  dialogue: "Dialogue",
  voice_over: "Voice over",
  music: "Music",
  ambience: "Ambience",
  room_tone: "Room tone",
  sound_effect: "Sound effect",
  foley: "Foley",
  source_audio: "Source audio",
  scratch_audio: "Scratch audio",
  mix: "Mix",
};

/** Same lines the host writes into the Director prompt. */
export function describeDirectorContext(
  state: ProjectState | null,
  selection: DirectorSelection,
  stageLabel?: string | null,
): DirectorContextModel {
  const entries = entriesFor(state, selection);
  const scene = state?.scenes.find((item) => item.stable_id === selection.sceneStableId);
  const stage =
    stageLabel ||
    (selection.stage ? selection.stage.split("_").join(" ") : null);
  const sceneName = scene ? `Scene ${scene.id}` : null;
  const headline =
    sceneName && stage ? `${sceneName} · ${stage}` : sceneName || stage || (state ? "Nothing selected" : "No project open");
  return {
    headline,
    entries,
    scopeText: entries.length ? ["Workflow scope:", ...entries.map((entry) => `- ${entry.label}`)].join("\n") : "",
  };
}

function entriesFor(state: ProjectState | null, selection: DirectorSelection): DirectorContextEntry[] {
  const scene = state?.scenes.find((item) => item.stable_id === selection.sceneStableId);
  const cut = scene?.cuts.find((item) => item.stable_id === selection.shotStableId);
  const lookSlot = scene?.look_board?.slots?.find((slot) => slot.id === selection.lookSlotId);
  const requirement = state?.requirements?.find((item) => item.id === selection.requirementId);
  const asset = selection.assetId ? state?.assets?.find((item) => item.id === selection.assetId) : undefined;
  const entries: DirectorContextEntry[] = [];
  if (selection.workArea) {
    const area = `${selection.workArea.slice(0, 1).toUpperCase()}${selection.workArea.slice(1)}`;
    entries.push({
      kind: "workArea",
      id: selection.workArea,
      label: `work area: ${selection.workArea}`,
      name: area,
      status: selection.stage ?? "unknown",
    });
  }
  if (selection.sceneStableId) {
    entries.push({
      kind: "scene",
      id: selection.sceneStableId,
      label: `scene: ${selection.sceneStableId} — ${scene ? sceneLabel(scene) : "unresolved"}`,
      name: scene ? `Scene ${scene.id}` : "Unresolved scene",
      status: scene ? "resolved" : "unresolved",
    });
  }
  if (selection.shotStableId) {
    entries.push({
      kind: "shot",
      id: selection.shotStableId,
      label: `shot: ${selection.shotStableId} — ${cut ? shotLabel(cut) : "unresolved"}`,
      name: cut ? shotLabel(cut) : "Unresolved shot",
      status: cut ? (cut.dependency_state ?? "resolved") : "unresolved",
    });
  }
  if (selection.assetId) {
    entries.push({
      kind: "asset",
      id: selection.assetId,
      label: `asset: ${selection.assetId} — ${asset?.role ?? "unresolved"}`,
      name: asset ? roleDisplay(asset.role) : "Unresolved asset",
      status: asset?.status ?? "unresolved",
    });
  }
  if (selection.lookSlotId) {
    entries.push({
      kind: "lookSlot",
      id: selection.lookSlotId,
      label: `look slot: ${selection.lookSlotId} — ${lookSlot ? roleDisplay(lookSlot.role, lookSlot.title) : "unresolved"}`,
      name: lookSlot ? roleDisplay(lookSlot.role, lookSlot.title) : "Unresolved look slot",
      status: lookSlot?.approved_asset_id ? "approved" : lookSlot ? "pending" : "unresolved",
    });
  }
  if (selection.requirementId) {
    entries.push({
      kind: "requirement",
      id: selection.requirementId,
      label: `requirement: ${selection.requirementId} — ${
        requirement ? `${requirement.name} (${requirement.status})` : "unresolved"
      }`,
      name: requirement?.name ?? "Unresolved requirement",
      status: requirement?.status ?? "unresolved",
    });
  }
  return entries;
}

function sceneLabel(scene: SceneRecord): string {
  // Host scope.rs owns this exact format — keep parity (see phase4-closeout).
  const title = scene.title.trim();
  return title ? `Scene ${scene.id} ${title}` : `Scene ${scene.id}`;
}

function shotLabel(cut: CutRecord): string {
  const title = cut.title?.trim() ?? "";
  return title ? `Cut ${cut.id.cut} ${title}` : `Cut ${cut.id.cut}`;
}

function roleDisplay(role: string, title?: string | null): string {
  const trimmed = title?.trim();
  if (trimmed) return trimmed;
  return LOOK_DISPLAY[role] ?? role;
}

