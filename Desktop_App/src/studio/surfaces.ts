import type { StageNavItem, StageStatus } from "../workflowTypes";
import { stageStatus } from "../workflowTypes";
import type { SceneRecord } from "../workflowTypes";

export type WorkArea = "plan" | "produce" | "finish";
export type SurfaceId = string;

export type SurfaceNavItem = {
  id: SurfaceId;
  label: string;
  status: StageStatus | "neutral";
  statusLabel: string;
  active: boolean;
  available: boolean;
  disabledReason: string | null;
};

export type SurfaceBanner = {
  tone: "empty" | "blocked" | "loading" | "error";
  text: string;
};

export type DeskStage = {
  id: string;
  label: string;
  workArea: WorkArea;
  available: boolean;
  disabledReason: string | null;
  required_roles?: string[];
};

export function stagesFromRecipe(input: {
  navigation: StageNavItem[] | null | undefined;
  recipeStages: { id: string; label: string; work_area?: WorkArea; required_roles?: string[] }[] | null | undefined;
  scene: SceneRecord | null;
}): DeskStage[] {
  if (input.navigation?.length) {
    return input.navigation.map((item) => ({
      id: item.id,
      label: item.label,
      workArea: item.work_area,
      available: item.available,
      disabledReason: item.disabled_reason ?? null,
    }));
  }
  return (input.recipeStages ?? []).map((stage) => ({
    id: stage.id,
    label: stage.label,
    workArea: stage.work_area ?? "plan",
    available: true,
    disabledReason: null,
    required_roles: stage.required_roles,
  }));
}

export function workAreaForStage(stageId: string | null, stages: DeskStage[]): WorkArea | null {
  if (!stageId) return null;
  if (stageId === "profile") return "plan";
  return stages.find((stage) => stage.id === stageId)?.workArea ?? null;
}

export function surfacesForArea(input: {
  area: WorkArea;
  stages: DeskStage[];
  scene: SceneRecord | null;
  active: SurfaceId;
}): SurfaceNavItem[] {
  const items = input.stages
    .filter((stage) => stage.workArea === input.area)
    .map((stage) =>
      navItem(
        stage.id,
        stage.label,
        input.scene ? stageStatus(input.scene.stages, stage.id) : "missing",
        input.active,
        stage.available,
        stage.disabledReason,
      ),
    );
  if (input.area === "plan") {
    items.push(navItem("profile", "Profile", "neutral", input.active, true, null));
  }
  return items;
}

export function surfaceBanner(input: {
  surface: SurfaceId;
  hasScene: boolean;
  blockers: string[];
  busy: string | null;
  disabledReason?: string | null;
}): SurfaceBanner | null {
  if (input.busy) return { tone: "loading", text: input.busy };
  if (input.disabledReason) return { tone: "blocked", text: input.disabledReason };
  if (!input.hasScene && input.surface !== "profile" && input.surface !== "characters") {
    return { tone: "empty", text: "Select a scene." };
  }
  if (input.blockers.length && input.surface !== "profile") {
    return { tone: "blocked", text: `Blocked on ${input.blockers.join(", ")}` };
  }
  return null;
}

function navItem(
  id: SurfaceId,
  label: string,
  status: StageStatus | "neutral",
  active: SurfaceId,
  available: boolean,
  disabledReason: string | null,
): SurfaceNavItem {
  return {
    id,
    label,
    status,
    statusLabel: status === "neutral" ? "" : status === "present" ? "ready" : status === "missing" ? "missing" : status,
    active: id === active,
    available,
    disabledReason,
  };
}
