import { type AnimaticShot, type AssetRecord, type CutRecord, type CutSketch, type SceneRecord, type VisualSourceStrategy } from "../../workflowTypes";

export function effectiveVisualSourceStrategy(cut: CutRecord): VisualSourceStrategy | null {
  return cut.visual_source?.strategy ??
    (cut.approved_start_frame ? "preproduction_start_frame" : null);
}

export function visualSourceReady(cut: CutRecord): boolean {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (strategy === "no_start_frame") return true;
  if (strategy === "preproduction_start_frame") return Boolean(cut.approved_start_frame);
  return Boolean(cut.visual_source?.asset_id);
}

export function visualSourceStatus(cut: CutRecord): string {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (!strategy) return "Source strategy needed";
  if (visualSourceReady(cut)) return "Render source ready";
  if (strategy === "previous_selected_take_exit_frame") return "Planned · awaiting take";
  if (strategy === "shared_reference") return "Planned · choose reference";
  if (strategy === "deferred_manual") return "Planned · awaiting manual source";
  return "Planned · start frame needed";
}

export function renderBlocker(cut: CutRecord): string | null {
  if (!cut.production_note_path) return "production note missing";
  if (!cut.prompt?.trim()) return "production video prompt missing";
  if (!effectiveVisualSourceStrategy(cut)) return "visual-source strategy missing";
  if (!visualSourceReady(cut)) {
    const strategy = effectiveVisualSourceStrategy(cut);
    if (strategy === "preproduction_start_frame") return "approved start frame missing";
    if (strategy === "previous_selected_take_exit_frame") {
      return "predecessor take or exit frame missing";
    }
    if (strategy === "shared_reference") return "approved shared reference missing";
    return "concrete render source missing";
  }
  return null;
}

export function visualSourceHint(cut: CutRecord, boundAsset: AssetRecord | undefined): string {
  const strategy = effectiveVisualSourceStrategy(cut);
  if (!strategy) {
    return "Choose and save a strategy. This approves how the cut will obtain its opening image.";
  }
  if (boundAsset) {
    return `Bound source: ${boundAsset.path}`;
  }
  if (strategy === "previous_selected_take_exit_frame") {
    const predecessor = cut.visual_source?.predecessor?.cut;
    return predecessor
      ? `Depends on Cut ${predecessor}. Production notes may proceed, but rendering waits for that cut’s selected take and exit frame.`
      : "Choose an earlier cut. Production notes may preserve the dependency before its frame exists.";
  }
  if (strategy === "shared_reference") {
    return "Choose an approved scene-look still to use as the shared source.";
  }
  if (strategy === "no_start_frame") {
    return "This cut is render ready without a starting image.";
  }
  if (strategy === "deferred_manual") {
    return "The source is intentionally deferred. Rendering remains blocked until a concrete asset is attached.";
  }
  return cut.approved_start_frame
    ? `Approved start frame: ${cut.approved_start_frame}`
    : "Generate or attach a still, then choose Use on the approved version.";
}

export function approvedSceneLookAssets(scene: SceneRecord, assets: AssetRecord[]): AssetRecord[] {
  const ids = new Set(
    (scene.look_board?.slots ?? [])
      .map((slot) => slot.approved_asset_id)
      .filter((id): id is string => Boolean(id)),
  );
  return (assets ?? []).filter(
    (asset) =>
      ids.has(asset.id) &&
      asset.scene_id === scene.id &&
      asset.status === "approved",
  );
}

export function cutImageAssets(assets: AssetRecord[], sceneId: number, cutNumber: number): AssetRecord[] {
  return (assets ?? []).filter(
    (asset) =>
      CUT_IMAGE_ROLES.has(asset.role) &&
      asset.cut_id?.source_scene === sceneId &&
      asset.cut_id?.cut === cutNumber,
  );
}

export function cutForShot(scene: SceneRecord, shot: AnimaticShot): CutRecord | undefined {
  return scene.cuts.find(
    (cut) => cut.id.cut === shot.cut_id.cut && cut.id.source_scene === shot.cut_id.source_scene,
  );
}

export function dependencyNote(state: CutRecord["dependency_state"]): string | null {
  switch (state) {
    case "needs_review":
      return "An approved upstream artifact changed. This shot needs review. Existing files were kept.";
    case "stale":
      return "An approved upstream artifact changed. This shot is stale. Existing files were kept.";
    case "invalid":
      return "An approved upstream artifact changed. This shot is invalid until it is rebuilt. Existing files were kept.";
    case "keep":
      return "An approved upstream artifact changed. This shot is safe to keep.";
    default:
      return null;
  }
}

export function editableSketch(cut: CutRecord): CutSketch {
  if (cut.sketch) return cut.sketch;
  const hasDialogue = Boolean(cut.dialogue?.length);
  const lipsync = cut.speech_mode === "source_video_lipsync";
  const voiceOver = cut.speech_mode === "voice_over";
  return {
    title: cut.title,
    shot_role: lipsync ? "performance" : voiceOver ? "reaction" : hasDialogue ? "atmosphere" : "insert",
    subject: cut.subject,
    speaker: cut.dialogue?.[0]?.speaker ?? null,
    mouth_policy: lipsync
      ? "speaking_visible"
      : voiceOver
        ? "off_camera"
        : hasDialogue
          ? "closed_or_not_visible"
          : "no_people",
    pace: null,
    visual_prompt: cut.prompt,
    negative_prompt: cut.negative_prompt,
    reference_roles: [],
    transition_idea: null,
    dialogue: (cut.dialogue ?? []).map((line) => ({
      ...line,
      heard_as: lipsync ? "lip_sync" : hasDialogue ? "voice_over" : null,
    })),
    on_camera: [],
  };
}

export function nullable(value: string): string | null {
  const trimmed = value.trim();
  return trimmed || null;
}

export function splitList(value: string): string[] {
  return value
    .split(/[,;\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

export function formatSketchDialogue(sketch: CutSketch): string {
  return (sketch.dialogue ?? [])
    .map(
      (line) =>
        `${line.speaker ?? ""} | ${line.heard_as ?? ""} | ${line.text ?? ""}`,
    )
    .join("\n");
}

export function parseSketchDialogue(
  value: string,
): { speaker: string; heard_as?: string | null; text: string }[] {
  return value
    .split("\n")
    .map((line) => {
      const parts = line.split("|");
      if (parts.length >= 3) {
        const speaker = parts.shift()?.trim() ?? "";
        const heardAs = parts.shift()?.trim() ?? "";
        return {
          speaker,
          heard_as: heardAs || null,
          text: parts.join("|").trim(),
        };
      }
      if (parts.length === 2) {
        return {
          speaker: parts[0].trim(),
          heard_as: null,
          text: parts[1].trim(),
        };
      }
      return { speaker: "", heard_as: null, text: line.trim() };
    })
    .filter((line) => line.text);
}

export const CUT_IMAGE_ROLES = new Set([
  "cut_start_frame",
  "performance_source",
  "reaction_plate",
  "insert_plate",
  "edit_source",
]);
