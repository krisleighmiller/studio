import { LOOK_ROLES, MOUTH_POLICIES, SCENE_STRATEGIES, SHOT_ROLES, type CutRecord, type SceneRecord, type SceneReport } from "../../workflowTypes";
import { speechLabel } from "../labels";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, presentSurface, queueLookThumbs, sketchSelectField, sketchTextarea, sketchTextField } from "./shared";
import { renderStartFrameCard } from "./referenceStage";
import { editableSketch, formatSketchDialogue, nullable, parseSketchDialogue, splitList } from "./surfaceSelectors";
import { buildSketchModel } from "./surfaceModels";
import { renderAnimatic } from "./sequenceReview";
import type { SketchIntent, ReferenceIntent, NotesIntent } from "./surfaceIntents";
import { renderSceneDialogueAudio } from "../sceneDialogueAudio";

function paintSketchStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  emit: (intent: SketchIntent) => void,
  emitReference: (intent: ReferenceIntent) => void,
  emitAudio?: (intent: NotesIntent) => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-sketches workflow-animatic";
  wrap.dataset.sceneId = String(scene.id);
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
  const limitLabel = document.createElement("label");
  limitLabel.className = "workflow-field inline";
  const limitTitle = document.createElement("span");
  limitTitle.textContent = "Maximum cuts in this scene";
  const limitSelect = document.createElement("select");
  limitSelect.setAttribute("aria-label", "Maximum cuts in this scene");
  for (let count = 1; count <= 12; count += 1) {
    const option = document.createElement("option");
    option.value = String(count);
    option.textContent = String(count);
    limitSelect.append(option);
  }
  limitSelect.value = String(scene.max_cuts ?? 12);
  limitSelect.addEventListener("change", () => emit({ type: "set-max-cuts", maxCuts: Number(limitSelect.value) }));
  limitLabel.append(limitTitle, limitSelect);
  toolbar.append(strategyLabel, overrideLabel, limitLabel);
  const alternateRow = document.createElement("div");
  alternateRow.className = "workflow-inspector-actions";
  const alternate = actionButton("New alternate angle", () => emit({ type: "alternate" }));
  alternate.disabled = alternate.disabled || scene.cuts.length >= (scene.max_cuts ?? 12);
  alternate.title =
    "Adds a new shot to this scene, with its own id, sketch, and animatic timing. It does not mark an existing video as an alternate take.";
  alternateRow.append(alternate);
  wrap.append(toolbar, alternateRow);
  if (scene.cuts.length > (scene.max_cuts ?? 12)) {
    wrap.append(block("workflow-banner warn", `This scene already has ${scene.cuts.length} cuts, above your limit of ${scene.max_cuts}. Remove unwanted sketch-only cuts below before drafting again; cuts with attached work are protected. No model request will be sent while over the limit.`));
  }

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
    const preview = renderAnimatic(scene, emit);
    const heading = preview.querySelector("h2");
    if (heading) heading.textContent = heading.textContent?.replace(/^Sketch animatic/, "Storyboard preview") ?? "Storyboard preview";
    const previewNote = preview.querySelector<HTMLElement>(".workflow-field-hint");
    if (previewNote) previewNote.textContent = "Preview the cut cards in order using their stills, dialogue text, timing, and any attached scratch audio. This is not rendered video.";
    const rows = preview.querySelector<HTMLOListElement>(".workflow-animatic-list");
    const shotRows = rows ? Array.from(rows.children) : [];
    rows?.remove();
    // Keep the scene-level preview and approval, but put each shot's existing
    // editorial controls on the corresponding cut card rather than a second list.
    wrap.append(...Array.from(preview.childNodes));
    const cards = renderSketchCards(scene, emit, emitReference, emitAudio);
    const cutCards = Array.from(cards.querySelectorAll<HTMLElement>(".workflow-sketch-card"));
    (scene.animatic?.shots ?? []).forEach((shot, index) => {
      const cutIndex = scene.cuts.findIndex(cut => cut.id.source_scene === shot.cut_id.source_scene && cut.id.cut === shot.cut_id.cut);
      if (cutIndex >= 0 && shotRows[index] && cutCards[cutIndex]) {
        const shotList = document.createElement("ol");
        shotList.className = "workflow-animatic-list";
        shotList.append(shotRows[index]);
        cutCards[cutIndex].append(shotList);
      }
    });
    // Preserve controls for any legacy orphan shot rather than silently hiding it.
    if (rows) {
      rows.append(...shotRows.filter(row => !row.isConnected && !cards.contains(row)));
      if (rows.children.length) wrap.append(rows);
    }
    wrap.append(cards);
  }
  return wrap;
}

function renderSketchCards(
  scene: SceneRecord,
  emit: (intent: SketchIntent) => void,
  emitReference: (intent: ReferenceIntent) => void,
  emitAudio?: (intent: NotesIntent) => void,
): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-sketch-cards";
  for (const cut of scene.cuts) {
    list.append(renderSketchCard(scene, cut, emit, emitReference, emitAudio));
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
  emitAudio?: (intent: NotesIntent) => void,
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
  if (cut.script_dialogue_review) {
    const review = cut.script_dialogue_review;
    const lines = (items: { speaker: string; text: string }[]) => items.length
      ? items.map((line) => `${line.speaker}: ${line.text}`).join("\n") : "(none)";
    card.append(block("workflow-banner warn", `Script dialogue changed in ${scene.source_path ?? "saved script"}. Cut ${cut.id.cut} is not automatically revised.\nPrevious scene script:\n${lines(review.previous)}\nCurrent scene script:\n${lines(review.current)}\nCut production-note dialogue (${cut.production_note_path ?? "no note source"}, retained):\n${lines(cut.dialogue ?? [])}\nCut sketch dialogue (retained):\n${lines(cut.sketch?.dialogue ?? [])}`));
  }
  if (cut.script_dialogue_kept && !cut.script_dialogue_review) {
    card.append(block("workflow-field-hint", "Explicitly kept authored cut dialogue for the current script revision. The cut and production-note cues were not copied from the script."));
  }
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
  refsTitle.textContent = "Scene look references for prompt context";
  refs.append(refsTitle, block("workflow-field-hint", "These roles guide the production prompt, not the video upload. To choose one image for Render Cut, use Visual source and reference image below: select Approved shared scene reference, choose the approved image, then Save source strategy."));
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
  if (cut.script_dialogue_review) {
    const revise = actionButton("Revise cut dialogue", () => dialogue.focus());
    const keep = actionButton("Keep authored dialogue", () => emit({ type: "resolve-script-dialogue", shotId: cut.stable_id ?? "", choice: "keep" }));
    keep.disabled = !cut.stable_id || Boolean(getSurfaceHost().busy);
    card.append(revise, keep, block("workflow-field-hint", "Revise in the field above, save the sketch, then explicitly keep the resulting cut dialogue. Production-note cues are separate and remain unchanged until reviewed there."));
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton("Save sketch", () => {
      emit({
        type: "save-sketch",
        cut: cut.id.cut,
        shotId: cut.stable_id || undefined,
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
  if (scene.animatic?.approved) {
    const reopen = actionButton("Reopen sketch for revision", () => emit({ type: "reopen-sketch", shotId: cut.stable_id ?? "" }));
    reopen.disabled = reopen.disabled || !cut.stable_id;
    reopen.title = "Reopens this scene's sketch animatic approval for an otherwise empty sketch. Removal is a separate action; attached work stays protected.";
    actions.append(reopen);
    card.append(block("workflow-field-hint", "Sketch animatic approved. Reopen for revision before removing an empty sketch; media, production notes and dependent shots remain protected."));
  }
  const remove = actionButton("Remove cut", () => emit({ type: "remove-cut", shotId: cut.stable_id ?? "" }));
  remove.disabled = remove.disabled || !cut.stable_id;
  remove.title = "Remove this sketch-only cut and its rough-cut timing. Cuts with media, production work, or dependent shots cannot be removed here.";
  actions.append(remove);
  if (emitAudio && cut.speech_mode === "source_video_lipsync") card.append(renderSceneDialogueAudio({ ...scene, cuts: [cut] }, emitAudio));
  card.append(renderStartFrameCard(scene, cut, emitReference, true));
  return card;
}

export function renderSketchStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  onIntent: (intent: SketchIntent) => void,
  onReference: (intent: ReferenceIntent) => void,
  onAudio?: (intent: NotesIntent) => void,
): HTMLElement {
  const model = buildSketchModel({
    scene,
    report: sceneStatus,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
    chatReady: getSurfaceHost().chatReady,
  });
  const view = paintSketchStage(scene, sceneStatus, onIntent, onReference, onAudio);
  if (model.emptyTitle && model.emptyDetail && !view.querySelector(".workflow-missing")) {
    view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  }
  return presentSurface(view, model, Boolean(model.banner));
}