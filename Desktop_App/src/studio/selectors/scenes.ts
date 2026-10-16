import type { AssetRecord, ProjectState, SceneRecord, SceneReport } from "../../workflowTypes";

/**
 * Compose a scene heading without duplicating a "Scene N" prefix that the
 * stored title already carries (imported projects often store titles like
 * "Scene 1 — The last train"). Returns "Scene N — <title without prefix>".
 */
export function sceneHeadingTitle(id: number, title: string): string {
  const trimmed = title.trim();
  if (new RegExp(`^Scene\\s+${id}\\s*$`, "i").test(trimmed)) return `Scene ${id}`;
  const stripped = trimmed
    .replace(new RegExp(`^Scene\\s+${id}\\s*[—–\\-:]\\s*`, "i"), "")
    .trim();
  if (!stripped) return trimmed || `Scene ${id}`;
  return `Scene ${id} — ${stripped}`;
}
import { requirementBlocks } from "./project";

export type SceneSummary = {
  key: string;
  stableId: string;
  displayNumber: number;
  title: string;
  thumbnailPath: string | null;
  intent: string;
  look: string;
  cuts: number;
  references: string;
  takes: string;
  finish: string;
  characters: string[];
  requirements: string[];
  warnings: string[];
  blockers: string[];
  detail: string;
};

export function buildSceneSummaries(input: {
  state: ProjectState;
  reports: SceneReport[];
  assets: AssetRecord[];
  resolvePath: (path: string | null | undefined) => string | null;
}): SceneSummary[] {
  return input.state.scenes.map((scene) => {
    const report = input.reports.find((item) => item.id === scene.id);
    const blockers = (input.state.requirements ?? [])
      .filter((requirement) => (requirement.scene_ids ?? []).includes(scene.id) && requirementBlocks(requirement))
      .map((requirement) => requirement.name);
    const requirements = (input.state.requirements ?? [])
      .filter((requirement) => (requirement.scene_ids ?? []).includes(scene.id))
      .map((requirement) => requirement.name);
    const warnings = report?.warnings ?? [];
    const selected = scene.cuts.filter((cut) =>
      (cut.takes ?? []).some((take) => take.id === cut.selected_take || take.path === cut.selected_take),
    ).length;
    const versions = scene.post?.versions?.length ?? 0;
    const summary: SceneSummary = {
      key: scene.stable_id?.trim() || `scene:${scene.id}`,
      stableId: scene.stable_id?.trim() || "",
      displayNumber: scene.id,
      title: scene.title.trim() || `Scene ${scene.id}`,
      thumbnailPath: thumbnail(scene, input.assets, input.resolvePath),
      intent: scene.intent_approved ? "approved" : scene.intent ? "draft" : "missing",
      look: lookCount(scene),
      cuts: scene.cuts.length,
      references:
        report && report.planned_visual_sources > 0
          ? `${report.render_ready_sources}/${report.planned_visual_sources}`
          : "unknown",
      takes: scene.cuts.length ? `${selected}/${scene.cuts.length}` : "0",
      finish: versions ? `${versions} export${versions === 1 ? "" : "s"}` : "none",
      characters: charactersIn(scene, input.state),
      requirements,
      warnings,
      blockers,
      detail: "",
    };
    summary.detail = detailLine(summary);
    return summary;
  });
}

function lookCount(scene: SceneRecord): string {
  const slots = scene.look_board?.slots ?? [];
  if (!slots.length) return "missing";
  const approved = slots.filter((slot) => slot.approved_asset_id).length;
  return `${approved}/${slots.length}`;
}

function charactersIn(scene: SceneRecord, state: ProjectState): string[] {
  const names = new Set<string>();
  const known = state.characters.map((character) => character.name.trim()).filter(Boolean);
  const speakers = [
    ...(scene.dialogue ?? []).map((line) => line.speaker),
    ...scene.cuts.flatMap((cut) => (cut.dialogue ?? []).map((line) => line.speaker)),
  ];
  for (const speaker of speakers) {
    const match = known.find((name) => name.toLowerCase() === speaker.trim().toLowerCase());
    if (match) names.add(match);
  }
  return [...names];
}

function thumbnail(
  scene: SceneRecord,
  assets: AssetRecord[],
  resolvePath: (path: string | null | undefined) => string | null,
): string | null {
  for (const slot of scene.look_board?.slots ?? []) {
    const asset = assets.find((item) => item.id === slot.approved_asset_id);
    const path = resolvePath(asset?.path);
    if (path) return path;
  }
  return null;
}

function detailLine(summary: SceneSummary): string {
  const parts = [
    `Intent ${summary.intent}`,
    `Look ${summary.look}`,
    `${summary.cuts} cuts`,
    `References ${summary.references}`,
    `Takes ${summary.takes}`,
    `Finish ${summary.finish}`,
  ];
  if (summary.characters.length) parts.push(summary.characters.join(", "));
  if (summary.blockers.length) parts.push(`Blocked: ${summary.blockers.join(", ")}`);
  if (summary.warnings.length) parts.push(summary.warnings.join("; "));
  return parts.join(" · ");
}
