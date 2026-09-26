/** Editors the app can paint. Any other stage id is still a recipe stage. */
const STAGE_EDITORS = [
  "script",
  "characters",
  "scene_intent",
  "scene_look",
  "cut_sketches",
  "reference_images",
  "production_notes",
  "render",
  "take_review",
  "assemble",
] as const;

export function stageHasEditor(stageId: string): boolean {
  return (STAGE_EDITORS as readonly string[]).includes(stageId);
}

/** Fallback for a stage the recipe named and this build does not edit. */
export function paintUnknownRecipeStage(stage: {
  id: string;
  label: string;
  disabledReason: string | null;
}): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-recipe-stage";
  wrap.dataset.stage = stage.id;
  const title = document.createElement("h2");
  title.textContent = stage.label || stage.id;
  const copy = document.createElement("p");
  copy.className = stage.disabledReason ? "workflow-banner warn" : "empty-state";
  copy.textContent =
    stage.disabledReason ??
    `${title.textContent} is part of this recipe. This build tracks it and does not have a separate editor for it yet.`;
  wrap.append(title, copy);
  return wrap;
}
