import { LOOK_ROLES, MOUTH_POLICIES, SCENE_STRATEGIES, SHOT_ROLES, type AnimaticShot, type CutRecord, type SceneRecord, type SceneReport } from "../../workflowTypes";
import { speechLabel } from "../labels";
import { getRoughPlayback, getSurfaceHost, setRoughPlayback, stopRoughCut } from "./session";
import { actionButton, block, missingPanel, presentSurface, queueLookThumbs, sketchSelectField, sketchTextarea, sketchTextField } from "./shared";
import { renderStartFrameCard } from "./referenceStage";
import { cutForShot, editableSketch, formatSketchDialogue, nullable, parseSketchDialogue, splitList } from "./surfaceSelectors";
import { buildSketchModel } from "./surfaceModels";
import type { SketchIntent, ReferenceIntent } from "./surfaceIntents";

const TRANSITIONS: { value: string; label: string }[] = [
  { value: "", label: "Cut" },
  { value: "dissolve", label: "Dissolve" },
  { value: "fade_through_black", label: "Fade through black" },
];

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

function paintRoughStage(scene: SceneRecord, shot: AnimaticShot | null): void {
  const stage = document.querySelector<HTMLElement>(".workflow-rough-cut-stage");
  if (!stage || Number(stage.dataset.sceneId) !== scene.id) return;
  stage.replaceChildren();
  if (!shot) {
    stage.hidden = true;
    return;
  }
  stage.hidden = false;
  const cut = cutForShot(scene, shot);
  const title = document.createElement("h3");
  const index = (scene.animatic?.shots ?? []).findIndex((item) => item.id === shot.id);
  title.textContent = `${index + 1}. Cut ${shot.cut_id.cut} · ${cut?.sketch?.title || cut?.title || "Untitled"} · ${shot.duration_seconds.toFixed(2)}s`;
  const dialogue = document.createElement("p");
  const lines = cut?.sketch?.dialogue?.length ? cut.sketch.dialogue : [];
  const spoken = lines.length
    ? lines.map((line) => `${line.speaker}: ${line.text}`).join(" ")
    : (cut?.dialogue ?? []).map((line) => `${line.speaker}: ${line.text}`).join(" ");
  dialogue.textContent = spoken || "No dialogue on this shot.";
  stage.append(title, dialogue);
  const imagePath = getSurfaceHost().resolvePath(cut?.approved_start_frame);
  if (imagePath) {
    const image = document.createElement("img");
    image.alt = title.textContent ?? "";
    stage.append(image);
    void getSurfaceHost().mediaUrl(imagePath).then((url) => {
      image.src = url;
    });
  }
}

async function showRoughShot(scene: SceneRecord, index: number): Promise<void> {
  const playback = getRoughPlayback();
  if (!playback || playback.sceneId !== scene.id) return;
  const shots = scene.animatic?.shots ?? [];
  const shot = shots[index];
  if (!shot) {
    stopRoughCut();
    paintRoughStage(scene, null);
    getSurfaceHost().rerender();
    return;
  }
  playback.index = index;
  playback.audio?.pause();
  playback.audio = null;
  paintRoughStage(scene, shot);
  if (shot.scratch_audio_path) {
    const abs = getSurfaceHost().resolvePath(shot.scratch_audio_path);
    if (abs) {
      const url = await getSurfaceHost().mediaUrl(abs);
      const current = getRoughPlayback();
      if (!current || current.sceneId !== scene.id || current.index !== index) return;
      const audio = new Audio(url);
      current.audio = audio;
      void audio.play().catch(() => undefined);
    }
  }
  const ms = Math.max(0.4, shot.duration_seconds) * 1000;
  const timer = window.setTimeout(() => {
    const current = getSurfaceHost().state?.scenes.find((item) => item.id === scene.id) ?? scene;
    void showRoughShot(current, index + 1);
  }, ms);
  const live = getRoughPlayback();
  if (live) live.timer = timer;
}

function playRoughCut(scene: SceneRecord): void {
  const shots = scene.animatic?.shots ?? [];
  if (!shots.length) return;
  stopRoughCut();
  setRoughPlayback({ sceneId: scene.id, index: 0, timer: 0, audio: null });
  getSurfaceHost().rerender();
  void showRoughShot(scene, 0);
}

function renderAnimatic(scene: SceneRecord, emit: (intent: SketchIntent) => void): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-animatic";
  section.setAttribute("aria-label", "Animatic");
  const shots = scene.animatic?.shots ?? [];
  const total = shots.reduce((sum, shot) => sum + (shot.duration_seconds || 0), 0);
  const heading = document.createElement("h2");
  heading.textContent = scene.animatic?.approved
    ? `Rough cut · approved · ${total.toFixed(1)}s`
    : `Rough cut · ${total.toFixed(1)}s`;
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Timing and order sit on the cut sketches. Dialogue without a scratch file plays as temporary on-screen text. A timing change marks later shot packages for review and keeps existing renders.";
  section.append(heading, note);

  const stage = document.createElement("div");
  stage.className = "workflow-rough-cut-stage";
  stage.dataset.sceneId = String(scene.id);
  stage.hidden = getRoughPlayback()?.sceneId !== scene.id;
  section.append(stage);

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  controls.append(
    actionButton(getRoughPlayback()?.sceneId === scene.id ? "Stop rough cut" : "Play rough cut", () => {
      if (getRoughPlayback()?.sceneId === scene.id) {
        stopRoughCut();
        getSurfaceHost().rerender();
        return;
      }
      playRoughCut(scene);
    }),
    actionButton(scene.animatic?.approved ? "Rough cut approved" : "Approve rough cut", () => {
      emit({ type: "approve-animatic" });
    }),
  );
  const approveButton = controls.querySelectorAll("button")[1] as HTMLButtonElement | undefined;
  if (approveButton && scene.animatic?.approved) approveButton.disabled = true;
  section.append(controls);

  const warnings = scene.animatic?.warnings ?? [];
  if (warnings.length) {
    section.append(
      block(
        "workflow-banner warn",
        warnings.map((warning) => warning.message).join("\n"),
      ),
    );
  }

  const list = document.createElement("ol");
  list.className = "workflow-animatic-list";
  shots.forEach((shot, index) => {
    const cut = cutForShot(scene, shot);
    const item = document.createElement("li");
    item.className = "workflow-animatic-row";
    const label = document.createElement("span");
    label.className = "workflow-animatic-label";
    label.textContent = `Cut ${shot.cut_id.cut} · ${cut?.sketch?.title || cut?.title || "Untitled"}`;
    const duration = document.createElement("input");
    duration.type = "number";
    duration.min = "0.4";
    duration.max = "120";
    duration.step = "0.1";
    duration.value = String(shot.duration_seconds);
    duration.setAttribute("aria-label", `Duration for cut ${shot.cut_id.cut}`);
    duration.addEventListener("change", () => {
      const seconds = Number(duration.value);
      if (!Number.isFinite(seconds)) return;
      emit({ type: "set-duration", shotId: shot.id, durationSeconds: seconds });
    });
    const scratch = document.createElement("span");
    scratch.className = "workflow-animatic-scratch";
    const hasDialogue = Boolean(cut?.dialogue?.length || cut?.sketch?.dialogue?.length);
    if (shot.scratch_audio_path) {
      scratch.textContent = shot.scratch_audio_path.split("/").pop() ?? "Scratch audio";
    } else if (hasDialogue) {
      scratch.textContent = "Temporary dialogue";
    } else {
      scratch.textContent = "No scratch audio";
    }
    const rowActions = document.createElement("div");
    rowActions.className = "workflow-animatic-actions";
    rowActions.append(
      actionButton("Up", () => moveAnimaticShot(scene, index, -1, emit)),
      actionButton("Down", () => moveAnimaticShot(scene, index, 1, emit)),
      actionButton(shot.scratch_audio_path ? "Replace scratch" : "Attach scratch", () => {
        emit({ type: "attach-scratch", shotId: shot.id });
      }),
    );
    if (shot.scratch_audio_path) {
      rowActions.append(
        actionButton("Remove scratch", () => {
          emit({ type: "clear-scratch", shotId: shot.id });
        }),
      );
    }
    const takeInfo = document.createElement("span");
    takeInfo.className = "workflow-animatic-take";
    const selected = (cut?.takes ?? []).find(
      (candidate) => candidate.id === cut?.selected_take || candidate.path === cut?.selected_take,
    );
    takeInfo.textContent = selected
      ? `${selected.path.split("/").pop() ?? "take"} · ${selected.status.replace(/_/g, " ")}`
      : "No take";

    const trimIn = document.createElement("input");
    trimIn.type = "number";
    trimIn.min = "0";
    trimIn.step = "0.1";
    trimIn.placeholder = "in";
    trimIn.value = shot.trim_in_seconds == null ? "" : String(shot.trim_in_seconds);
    trimIn.setAttribute("aria-label", `Trim in (seconds) for cut ${shot.cut_id.cut}`);
    trimIn.addEventListener("change", () => {
      const parsed = trimIn.value.trim() === "" ? null : Number(trimIn.value);
      if (parsed !== null && !Number.isFinite(parsed)) return;
      emit({
        type: "set-shot-trim",
        shotId: shot.id,
        trimInSeconds: parsed,
        trimOutSeconds: shot.trim_out_seconds ?? null,
      });
    });

    const trimOut = document.createElement("input");
    trimOut.type = "number";
    trimOut.min = "0";
    trimOut.step = "0.1";
    trimOut.placeholder = "out";
    trimOut.value = shot.trim_out_seconds == null ? "" : String(shot.trim_out_seconds);
    trimOut.setAttribute("aria-label", `Trim out (seconds) for cut ${shot.cut_id.cut}`);
    trimOut.addEventListener("change", () => {
      const parsed = trimOut.value.trim() === "" ? null : Number(trimOut.value);
      if (parsed !== null && !Number.isFinite(parsed)) return;
      emit({
        type: "set-shot-trim",
        shotId: shot.id,
        trimInSeconds: shot.trim_in_seconds ?? null,
        trimOutSeconds: parsed,
      });
    });

    const transition = document.createElement("select");
    transition.setAttribute("aria-label", `Transition out of cut ${shot.cut_id.cut}`);
    for (const option of TRANSITIONS) {
      const element = document.createElement("option");
      element.value = option.value;
      element.textContent = option.label;
      transition.append(element);
    }
    transition.value = shot.transition ?? "";
    transition.addEventListener("change", () => {
      emit({
        type: "set-shot-transition",
        shotId: shot.id,
        transition: transition.value === "" ? null : transition.value,
      });
    });

    const flags: string[] = [];
    if (cut?.dependency_state) flags.push(cut.dependency_state.replace(/_/g, " "));
    if (cut?.package_needs_review) flags.push("needs review");
    const flag = document.createElement("span");
    flag.className = "workflow-animatic-flag";
    flag.textContent = flags.length ? `⚠ ${flags.join(", ")}` : "";

    item.append(label, takeInfo, trimIn, trimOut, transition, duration, scratch, flag, rowActions);
    list.append(item);
  });
  section.append(list);
  if (getRoughPlayback()?.sceneId === scene.id) {
    const current = shots[getRoughPlayback()!.index];
    if (current) queueMicrotask(() => paintRoughStage(scene, current));
  }
  return section;
}

function moveAnimaticShot(
  scene: SceneRecord,
  index: number,
  delta: number,
  emit: (intent: SketchIntent) => void,
): void {
  const shots = [...(scene.animatic?.shots ?? [])];
  const next = index + delta;
  if (next < 0 || next >= shots.length) return;
  const [moved] = shots.splice(index, 1);
  shots.splice(next, 0, moved);
  emit({ type: "reorder-animatic", shotIds: shots.map((shot) => shot.id) });
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
