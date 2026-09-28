import { LOOK_ROLES, MOUTH_POLICIES, SCENE_STRATEGIES, SHOT_ROLES, type CutRecord, type SceneRecord, type SceneReport } from "../../workflowTypes";
import { speechLabel } from "../labels";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, presentSurface, queueLookThumbs, sketchSelectField, sketchTextarea, sketchTextField } from "./shared";
import { renderStartFrameCard } from "./referenceStage";
import { editableSketch, formatSketchDialogue, nullable, parseSketchDialogue, splitList } from "./surfaceSelectors";
import { buildSketchModel } from "./surfaceModels";
import { renderAnimatic } from "./sequenceReview";
import type { SketchIntent, ReferenceIntent } from "./surfaceIntents";

function paintSketchStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  emit: (intent: SketchIntent) => void,
  emitReference: (intent: ReferenceIntent) => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-sketches";
  const verdict = sceneStatus?.mix_verdict;
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-sketch-toolbar";
  const strategyLabel = document.createElement("label");
  strategyLabel.className = "workflow-field inline";
  const strategyTitle = document.createElement("span");
  strategyTitle.textContent = "Strategy";
  const strategySelect = document.createElement("select");
  for (const option of SCENE_STRATEGIES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    strategySelect.append(item);
  }
  strategySelect.value = scene.strategy ?? sceneStatus?.strategy ?? "cinematic_balanced";
  strategyLabel.append(strategyTitle, strategySelect);
  const overrideLabel = document.createElement("label");
  overrideLabel.className = "workflow-check";
  const override = document.createElement("input");
  override.type = "checkbox";
  override.checked = Boolean(scene.strategy_override);
  const emitStrategy = () => {
    emit({
      type: "set-strategy",
      strategy: strategySelect.value,
      override: override.checked,
    });
  };
  strategySelect.addEventListener("change", emitStrategy);
  override.addEventListener("change", emitStrategy);
  overrideLabel.append(override, document.createTextNode(" Override mix rejection"));
  toolbar.append(strategyLabel, overrideLabel);
  const alternateRow = document.createElement("div");
  alternateRow.className = "workflow-inspector-actions";
  const alternate = actionButton("New alternate angle", () => emit({ type: "alternate" }));
  alternate.title =
    "Adds a new shot to this scene, with its own id, sketch, and animatic timing. It does not mark an existing video as an alternate take.";
  alternateRow.append(alternate);
  wrap.append(toolbar, alternateRow);

  if (verdict && (verdict.mix.performance + verdict.mix.other_dialogue + verdict.mix.silent) > 0) {
    const status = verdict.status;
    wrap.append(
      block(
        status === "reject"
          ? "workflow-banner reject"
          : status === "warn"
            ? "workflow-banner warn"
            : "workflow-banner",
        `${verdict.strategy.replace(/_/g, " ")} · ${verdict.mix.performance} performance / ${verdict.mix.other_dialogue} other dialogue / ${verdict.mix.silent} silent${
          verdict.messages.length ? `\n${verdict.messages.join("\n")}` : ""
        }`,
      ),
    );
  } else if (sceneStatus?.can_backfill_sketches) {
    wrap.append(
      missingPanel(
        "Cut sketches are missing.",
        "Backfill from production notes, then change shot roles so a cinematic-balanced mix is not all lip-sync. Or draft sketches from scene intent.",
      ),
    );
  } else if (!scene.intent) {
    wrap.append(
      missingPanel(
        "Scene intent is missing.",
        "Draft and approve scene intent first. Sketches need that planning layer.",
      ),
    );
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  if (sceneStatus?.can_backfill_sketches || scene.cuts.some((cut) => cut.production_note_path)) {
    actions.append(
      actionButton("Backfill from notes", () => emit({ type: "backfill" })),
    );
  }
  actions.append(
    actionButton("Draft sketches", () => emit({ type: "draft", strategy: strategySelect.value })),
  );
  wrap.append(actions);
  if (scene.cuts.length) {
    wrap.append(renderAnimatic(scene, emit));
    wrap.append(renderSketchCards(scene, emit, emitReference));
  }
  return wrap;
}

function renderSketchCards(
  scene: SceneRecord,
  emit: (intent: SketchIntent) => void,
  emitReference: (intent: ReferenceIntent) => void,
): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-sketch-cards";
  for (const cut of scene.cuts) {
    list.append(renderSketchCard(scene, cut, emit, emitReference));
  }
  queueLookThumbs(list);
  return list;
}

function moveCut(
  scene: SceneRecord,
  cut: CutRecord,
  delta: number,
  emit: (intent: SketchIntent) => void,
): void {
  const cuts = [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut);
  const index = cuts.findIndex((item) => item.stable_id && item.stable_id === cut.stable_id);
  const next = index + delta;
  if (index < 0 || next < 0 || next >= cuts.length) return;
  const ids = cuts.map((item) => item.stable_id ?? "").filter((id) => id.length > 0);
  if (ids.length !== cuts.length) return;
  const [moved] = ids.splice(index, 1);
  ids.splice(next, 0, moved);
  emit({ type: "reorder-cuts", shotIds: ids });
}

function renderCutOrder(
  scene: SceneRecord,
  cut: CutRecord,
  emit: (intent: SketchIntent) => void,
): HTMLElement {
  const cuts = [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut);
  const index = cuts.findIndex((item) => item.id.cut === cut.id.cut);
  const row = document.createElement("div");
  row.className = "workflow-sketch-toolbar";
  const earlier = actionButton("Move earlier", () => moveCut(scene, cut, -1, emit));
  earlier.disabled = index <= 0 || !cut.stable_id;
  const later = actionButton("Move later", () => moveCut(scene, cut, 1, emit));
  later.disabled = index < 0 || index >= cuts.length - 1 || !cut.stable_id;
  row.append(earlier, later);
  return row;
}

function renderSketchCard(
  scene: SceneRecord,
  cut: CutRecord,
  emit: (intent: SketchIntent) => void,
  emitReference: (intent: ReferenceIntent) => void,
): HTMLElement {
  const sketch = editableSketch(cut);
  const card = document.createElement("article");
  card.className = `workflow-sketch-card${cut.id.cut === getSurfaceHost().selectedCut ? " active" : ""}`;
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${sketch.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = cut.sketch ? speechLabel(cut.speech_mode) : "Imported fallback";
  header.append(heading, status);
  card.append(header);
  card.append(renderCutOrder(scene, cut, emit));

  const fields = document.createElement("div");
  fields.className = "workflow-sketch-field-grid";
  const title = sketchTextField(fields, "Title", sketch.title ?? "");
  const role = sketchSelectField(
    fields,
    "Shot role",
    sketch.shot_role ?? "",
    SHOT_ROLES,
  );
  const subject = sketchTextField(fields, "Subject", sketch.subject ?? "");
  const speaker = sketchTextField(fields, "Speaker", sketch.speaker ?? "");
  const mouth = getSurfaceHost().recipeRequires("mouth_policy")
    ? sketchSelectField(fields, "Mouth policy", sketch.mouth_policy ?? "", MOUTH_POLICIES)
    : null;
  const pace = sketchSelectField(fields, "Pace", sketch.pace ?? "", [
    { id: "tight", label: "Tight" },
    { id: "natural", label: "Natural" },
    { id: "held", label: "Held" },
  ]);
  const onCamera = sketchTextField(
    fields,
    "On camera",
    (sketch.on_camera ?? []).join(", "),
  );
  card.append(fields);

  const prompt = sketchTextarea(
    card,
    "Visual prompt",
    sketch.visual_prompt ?? "",
    5,
  );
  const negative = sketchTextarea(
    card,
    "Negative prompt",
    sketch.negative_prompt ?? "",
    2,
  );

  const refs = document.createElement("fieldset");
  refs.className = "workflow-sketch-references";
  const refsTitle = document.createElement("legend");
  refsTitle.textContent = "Scene look references";
  refs.append(refsTitle);
  const checkedRefs = new Set(sketch.reference_roles ?? []);
  const refChecks = LOOK_ROLES.map((option) => {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    input.value = option.id;
    input.checked = checkedRefs.has(option.id);
    label.append(input, document.createTextNode(option.label));
    refs.append(label);
    return input;
  });
  card.append(refs);

  const transition = sketchTextarea(
    card,
    "Transition idea",
    sketch.transition_idea ?? "",
    2,
  );
  const dialogue = sketchTextarea(
    card,
    "Dialogue coverage",
    formatSketchDialogue(sketch),
    3,
  );
  dialogue.placeholder = "Speaker | heard_as | dialogue text";
  const dialogueHint = document.createElement("p");
  dialogueHint.className = "workflow-field-hint";
  dialogueHint.textContent =
    "One line per beat: Speaker | on_camera, voice_over, lip_sync, or off_camera | dialogue text";
  card.append(dialogueHint);

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Save sketch", () => {
      emit({
        type: "save-sketch",
        cut: cut.id.cut,
        sketch: {
          title: nullable(title.value),
          shot_role: nullable(role.value),
          subject: nullable(subject.value),
          speaker: nullable(speaker.value),
          mouth_policy: mouth ? nullable(mouth.value) : null,
          pace: nullable(pace.value),
          visual_prompt: nullable(prompt.value),
          negative_prompt: nullable(negative.value),
          reference_roles: refChecks
            .filter((input) => input.checked)
            .map((input) => input.value),
          transition_idea: nullable(transition.value),
          dialogue: parseSketchDialogue(dialogue.value),
          on_camera: splitList(onCamera.value),
        },
      });
    }),
  );
  card.append(actions);
  card.append(renderStartFrameCard(scene, cut, emitReference, true));
  return card;
}

export function renderSketchStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  onIntent: (intent: SketchIntent) => void,
  onReference: (intent: ReferenceIntent) => void,
): HTMLElement {
  const model = buildSketchModel({
    scene,
    report: sceneStatus,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
    chatReady: getSurfaceHost().chatReady,
  });
  const view = paintSketchStage(scene, sceneStatus, onIntent, onReference);
  if (model.emptyTitle && model.emptyDetail && !view.querySelector(".workflow-missing")) {
    view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  }
  return presentSurface(view, model, Boolean(model.banner));
}