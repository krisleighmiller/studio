import { type SceneRecord } from "../../workflowTypes";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, newAlternateAngleButton, presentSurface } from "./shared";
import { buildTakeReviewModel } from "./surfaceModels";
import type { TakeReviewIntent } from "./surfaceIntents";

function paintTakeReviewStage(scene: SceneRecord, emit: (intent: TakeReviewIntent) => void): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-take-review";
  const refresh = actionButton("Refresh takes from project", () => {
    emit({ type: "refresh" });
  });
  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(newAlternateAngleButton(() => emit({ type: "alternate" })), refresh);
  const hint = document.createElement("p");
  hint.className = "workflow-field-hint";
  hint.textContent =
    "New alternate angle adds a shot to this scene. Mark alternate only changes this clip’s review status.";
  wrap.append(
    block(
      "workflow-banner",
      "A generated clip is a take until you select it. Changing a selected take unbinds dependent continuity frames. It does not approve a replacement frame.",
    ),
    actions,
    hint,
  );
  let count = 0;
  for (const cut of scene.cuts) {
    const takes = cut.takes ?? [];
    if (!takes.length) continue;
    count += takes.length;
    const card = document.createElement("article");
    card.className = "workflow-start-frame-card";
    const heading = document.createElement("h3");
    heading.textContent = `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
    card.append(heading);
    for (const take of takes) {
      const row = document.createElement("div");
      row.className = "workflow-take-row";
      const summary = document.createElement("button");
      summary.type = "button";
      summary.className = "workflow-take-path";
      summary.textContent = `${take.path} · ${take.status.replace(/_/g, " ")}`;
      summary.addEventListener("click", () => {
        getSurfaceHost().hooks?.onOpenPath(getSurfaceHost().resolvePath(take.path) ?? take.path);
      });
      const actions = document.createElement("div");
      actions.className = "workflow-inspector-actions";
      for (const [status, label] of [
        ["selected", "Select"],
        ["alternate", "Mark alternate"],
        ["rejected", "Reject"],
        ["needs_edit", "Needs edit"],
        ["needs_rerender", "Needs rerender"],
      ] as const) {
        const button = actionButton(label, () => {
          emit({ type: "set-status", cut: cut.id.cut, takeId: take.id, status });
        });
        button.disabled = Boolean(getSurfaceHost().busy) || take.status === status;
        actions.append(button);
      }
      row.append(summary, actions);
      card.append(row);
    }
    wrap.append(card);
  }
  if (!count) {
    wrap.append(
      missingPanel(
        "No takes to review.",
        "Generate clips under Scenes/SceneN/CutN, then refresh this stage. The importer registers video files as unreviewed takes.",
      ),
    );
  }
  return wrap;
}

export function renderTakeReviewStage(
  scene: SceneRecord,
  onIntent: (intent: TakeReviewIntent) => void,
): HTMLElement {
  const model = buildTakeReviewModel({
    scene,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
  });
  return presentSurface(paintTakeReviewStage(scene, onIntent), model, true);
}
