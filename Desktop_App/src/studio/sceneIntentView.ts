import type { SceneIntent, SceneRecord } from "../workflowTypes";
import { applySurfaceActions } from "./surfaceActions";
import type { SurfacePresentation } from "./surfaces/surfaceModels";

export type SceneIntentIntent =
  | { type: "draft" }
  | { type: "save"; intent: SceneIntent }
  | { type: "approve" };

const FIELDS: { key: keyof SceneIntent; label: string; list: boolean }[] = [
  { key: "narrative_purpose", label: "Narrative purpose", list: false },
  { key: "emotional_purpose", label: "Emotional purpose", list: false },
  { key: "character_focus", label: "Character focus", list: true },
  { key: "visual_contrast", label: "Visual contrast", list: false },
  { key: "important_props", label: "Important props", list: true },
  { key: "continuity_constraints", label: "Continuity constraints", list: true },
  { key: "sound_and_transition", label: "Sound and transition", list: true },
  { key: "audience_must_know", label: "Audience must know", list: false },
];

let draft: SceneIntent | null = null;
let draftScene: number | null = null;
let draftEdited = false;

export function hasEditedIntentDraft(sceneId: number): boolean { return draftScene === sceneId && draftEdited; }

export function clearIntentDraft(): void {
  draft = null;
  draftScene = null;
  draftEdited = false;
}

function emptyIntent(): SceneIntent {
  return {
    narrative_purpose: "",
    emotional_purpose: "",
    character_focus: [],
    visual_contrast: "",
    important_props: [],
    continuity_constraints: [],
    sound_and_transition: [],
    audience_must_know: "",
  };
}

function workingCopy(scene: SceneRecord): SceneIntent {
  if (draftScene !== scene.id || !draft) {
    draftScene = scene.id;
    draftEdited = false;
    draft = { ...emptyIntent(), ...scene.intent };
  }
  return draft;
}

export function renderSceneIntentView(
  scene: SceneRecord,
  model: SurfacePresentation,
  onIntent: (intent: SceneIntentIntent) => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-intent";
  wrap.dataset.status = model.status;
  wrap.dataset.tone = model.tone;
  if (model.loading) wrap.setAttribute("aria-busy", "true");
  const copy = workingCopy(scene);
  if (model.error) wrap.append(note("workflow-banner reject", model.error));
  if (model.banner) {
    wrap.append(note(model.tone === "ready" ? "workflow-banner" : "workflow-missing", model.banner));
  }
  for (const field of FIELDS) {
    const label = document.createElement("label");
    label.className = "workflow-field";
    const title = document.createElement("span");
    title.textContent = field.label;
    const input = document.createElement("textarea");
    input.rows = field.list ? 2 : 3;
    const value = copy[field.key];
    input.value = Array.isArray(value) ? value.join(", ") : (value ?? "");
    input.addEventListener("input", () => {
      if (!draft) return;
      draftEdited = true;
      if (field.list) {
        draft[field.key] = input.value
          .split(/[,;\n]/)
          .map((item) => item.trim())
          .filter(Boolean) as never;
      } else {
        draft[field.key] = input.value as never;
      }
    });
    label.append(title, input);
    wrap.append(label);
  }
  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Draft from script", () => onIntent({ type: "draft" })),
    actionButton("Save draft", () => onIntent({ type: "save", intent: { ...workingCopy(scene) } })),
    actionButton("Approve intent", () => onIntent({ type: "approve" })),
  );
  wrap.append(actions);
  applySurfaceActions(wrap, model.actions);
  return wrap;
}

function note(className: string, text: string): HTMLElement {
  const node = document.createElement("p");
  node.className = className;
  node.textContent = text;
  return node;
}

function actionButton(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", () => {
    if (button.disabled) return;
    onClick();
  });
  return button;
}
