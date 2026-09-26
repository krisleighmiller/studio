import { type SceneRecord, type SceneReport } from "../../workflowTypes";
import { getSurfaceHost } from "./session";

function paintReadiness(
  scene: SceneRecord | undefined,
  sceneStatus: SceneReport | undefined,
): HTMLElement {
  const list = document.createElement("ul");
  list.className = "workflow-readiness";
  const rows: [boolean, string][] = [
    [scene?.stages.script === "present", "Script beats"],
    [
      (getSurfaceHost().state?.characters.length ?? 0) > 0,
      `${getSurfaceHost().state?.characters.length ?? 0} characters (${getSurfaceHost().report?.identity_ready ?? 0} identity-ready)`,
    ],
    [Boolean(sceneStatus?.scene_intent), "Scene intent"],
    [Boolean(sceneStatus?.scene_look), "Scene look board"],
    [Boolean(sceneStatus?.cut_sketches), "Cut sketches"],
    [
      (sceneStatus?.planned_visual_sources ?? 0) === (scene?.cuts.length ?? 0),
      `${sceneStatus?.planned_visual_sources ?? 0}/${scene?.cuts.length ?? 0} visual-source strategies planned`,
    ],
    [
      (sceneStatus?.render_ready_sources ?? 0) === (scene?.cuts.length ?? 0),
      `${sceneStatus?.render_ready_sources ?? 0}/${scene?.cuts.length ?? 0} concrete render sources ready`,
    ],
    [Boolean(sceneStatus?.production_notes), "Production notes"],
  ];
  for (const [ready, label] of rows) {
    const item = document.createElement("li");
    item.className = ready ? "ready" : "missing";
    item.textContent = `${ready ? "Present" : "Missing"} · ${label}`;
    list.append(item);
  }
  return list;
}

export function renderReadiness(scene: SceneRecord | undefined, sceneStatus: SceneReport | undefined): HTMLElement {
  return paintReadiness(scene, sceneStatus);
}
