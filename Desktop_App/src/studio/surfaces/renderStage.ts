import { type SceneRecord } from "../../workflowTypes";
import { getSurfaceHost } from "./session";
import { block, missingPanel, newAlternateAngleButton, presentSurface } from "./shared";
import { buildRenderQueueModel } from "./surfaceModels";
import type { NotesIntent } from "./surfaceIntents";
import { renderProductionNoteCard } from "./notesStage";

function paintRenderStage(scene: SceneRecord, emit: (intent: NotesIntent) => void): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-production-notes";
  const takes = scene.cuts.reduce((total, cut) => total + (cut.takes?.length ?? 0), 0);
  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(newAlternateAngleButton(() => emit({ type: "alternate" })));
  wrap.append(
    block(
      takes ? "workflow-banner" : "workflow-missing",
      takes
        ? `${takes} take(s) recorded. Render another take here or directly from Production Notes.`
        : "No takes recorded yet. Ready cuts can be rendered here or directly from Production Notes.",
    ),
    actions,
    block(
      "workflow-field-hint",
      "New alternate angle adds a shot to this scene. Mark alternate only changes this clip’s review status.",
    ),
  );
  for (const cut of scene.cuts) {
    wrap.append(renderProductionNoteCard(cut, emit, true));
  }
  return wrap;
}

export function renderRenderStage(
  scene: SceneRecord,
  onIntent: (intent: NotesIntent) => void,
): HTMLElement {
  const model = buildRenderQueueModel({
    scene,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
  });
  const view = paintRenderStage(scene, onIntent);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, true);
}
