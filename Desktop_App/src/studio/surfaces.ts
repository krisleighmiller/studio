import type { SceneRecord, StageId, StageStatus } from "../workflowTypes";

export type WorkArea = "plan" | "produce" | "finish";
export type SurfaceId = StageId | "profile";

export type SurfaceNavItem = {
  id: SurfaceId;
  label: string;
  status: StageStatus | "neutral";
  statusLabel: string;
  active: boolean;
};

export type SurfaceBanner = {
  tone: "empty" | "blocked" | "loading" | "error";
  text: string;
};

const AREA: Record<StageId, WorkArea> = {
  script: "plan",
  characters: "plan",
  scene_intent: "plan",
  scene_look: "plan",
  cut_sketches: "plan",
  reference_images: "plan",
  production_notes: "plan",
  render: "produce",
  take_review: "produce",
  assemble: "finish",
};

export function workAreaForStage(stage: StageId): WorkArea {
  return AREA[stage];
}

export function surfacesForArea(input: {
  area: WorkArea;
  stages: { id: StageId; label: string }[];
  scene: SceneRecord | null;
  active: SurfaceId;
}): SurfaceNavItem[] {
  const items = input.stages
    .filter((stage) => workAreaForStage(stage.id) === input.area)
    .map((stage) => navItem(stage.id, stage.label, input.scene?.stages[stage.id] ?? "missing", input.active));
  if (input.area === "plan") {
    items.push(navItem("profile", "Profile", "neutral", input.active));
  }
  return items;
}

export function surfaceBanner(input: {
  surface: SurfaceId;
  hasScene: boolean;
  blockers: string[];
  busy: string | null;
}): SurfaceBanner | null {
  if (input.busy) return { tone: "loading", text: input.busy };
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
): SurfaceNavItem {
  return {
    id,
    label,
    status,
    statusLabel: status === "neutral" ? "" : status === "present" ? "ready" : status === "missing" ? "missing" : status,
    active: id === active,
  };
}
