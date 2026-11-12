import type { ProjectReport, ProjectState, RequirementRecord } from "../../workflowTypes";
import type { WorkAreaId } from "../selection";

export type ProjectSummary = {
  name: string;
  profileLabel: string;
  recipeLabel: string;
  workArea: string;
  blockers: string[];
  pendingApprovals: string[];
};

const BLOCKING_KINDS = new Set(["location", "voice", "plate"]);

export function requirementBlocks(requirement: Pick<RequirementRecord, "kind" | "status">): boolean {
  return BLOCKING_KINDS.has(requirement.kind) && (requirement.status === "missing" || requirement.status === "draft");
}

export function buildProjectSummary(input: {
  state: ProjectState | null;
  report: ProjectReport | null;
  workArea: WorkAreaId | null;
}): ProjectSummary {
  const state = input.state;
  const report = input.report;
  const name = state?.project_name?.trim() || report?.project_name?.trim() || "Untitled project";
  const profile = state?.profile;
  const profileLabel = profile?.medium?.trim() || profile?.audience?.trim() || "No profile";
  const recipeLabel = report?.recipe?.label || recipeName(profile?.recipe);
  const blockers = (state?.requirements ?? [])
    .filter(requirementBlocks)
    .map((requirement) => requirement.name);
  const warnings = report?.warnings ?? [];
  return {
    name,
    profileLabel,
    recipeLabel,
    workArea: input.workArea ?? "plan",
    blockers: [...blockers, ...warnings],
    pendingApprovals: pendingApprovals(state),
  };
}

function recipeName(recipe: string | undefined): string {
  if (!recipe) return "Narrative dialogue";
  return recipe
    .split("_")
    .filter(Boolean)
    .map((word) => word.slice(0, 1).toUpperCase() + word.slice(1))
    .join(" ");
}

function pendingApprovals(state: ProjectState | null): string[] {
  if (!state) return [];
  const lines: string[] = [];
  for (const scene of state.scenes) {
    if (scene.intent && !scene.intent_approved) lines.push(`Scene ${scene.id} intent`);
    const slots = scene.look_board?.slots ?? [];
    if (slots.some((slot) => (slot.asset_ids?.length ?? 0) > 0 && !slot.approved_asset_id)) {
      lines.push(`Scene ${scene.id} look`);
    }
    for (const cut of scene.cuts) {
      const takes = cut.takes ?? [];
      if (takes.length && !takes.some((take) => take.id === cut.selected_take || take.path === cut.selected_take)) {
        lines.push(`Scene ${scene.id} Cut ${cut.id.cut} take`);
      }
      if (
        cut.visual_source?.strategy === "previous_selected_take_exit_frame" &&
        !cut.visual_source.asset_id
      ) {
        lines.push(`Scene ${scene.id} Cut ${cut.id.cut} continuity`);
      }
    }
  }
  return lines;
}
