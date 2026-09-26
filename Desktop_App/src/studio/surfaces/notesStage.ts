import { type CutRecord, type SceneRecord } from "../../workflowTypes";
import { roleLabel, speechLabel } from "../labels";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, newAlternateAngleButton, presentSurface, queueTakeVideos, renderCutButton } from "./shared";
import { dependencyNote, renderBlocker, visualSourceStatus } from "./surfaceSelectors";
import { buildNotesModel } from "./surfaceModels";
import type { NotesIntent } from "./surfaceIntents";

function paintProductionNotesStage(
  scene: SceneRecord,
  emit: (intent: NotesIntent) => void,
): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-production-notes";
  const sync = actionButton("Build all production video prompts", () => {
    emit({ type: "sync-all" });
  });
  sync.disabled = Boolean(getSurfaceHost().busy);
  const refresh = actionButton("Refresh rendered takes", () => {
    emit({ type: "refresh-takes" });
  });
  refresh.disabled = Boolean(getSurfaceHost().busy);
  const intro = document.createElement("section");
  intro.className = "workflow-source-controls";
  const alternate = newAlternateAngleButton(() => emit({ type: "alternate" }));
  intro.append(
    block(
      "workflow-field-hint",
      "Build complete moving-image prompts from the script, scene intent, ordered cut sketches, dialogue, approved scene references, and each cut’s approved visual source. Use this after planning or reference images change.",
    ),
    block(
      "workflow-field-hint",
      "New alternate angle adds a shot to this scene. Mark alternate only changes this clip’s review status.",
    ),
    sync,
    refresh,
    alternate,
  );
  list.append(intro);
  for (const cut of scene.cuts) {
    list.append(renderProductionNoteCard(cut, emit));
  }
  return list;
}

export function renderProductionNoteCard(
  cut: CutRecord,
  emit: (intent: NotesIntent) => void,
  renderQueue = false,
): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-start-frame-card workflow-production-note-card";
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  const blocker = renderBlocker(cut);
  status.textContent = cut.package_needs_review
    ? blocker
      ? `Needs review · render blocked · ${blocker}`
      : `Needs review · ${cut.takes?.length ?? 0} take(s) kept`
    : blocker
      ? `Render blocked · ${blocker}`
      : `${cut.takes?.length ?? 0} take(s) · ready`;
  header.append(heading, status);
  card.append(header);
  if (cut.package_needs_review) {
    card.append(
      block(
        "workflow-banner warn",
        "Animatic timing changed. This shot package needs review. Existing renders were kept.",
      ),
      actionButton("Mark package reviewed", () => {
        emit({ type: "clear-package", cut: cut.id.cut });
      }),
    );
  }
  const dependency = dependencyNote(cut.dependency_state);
  if (dependency) {
    card.append(block("workflow-banner warn", dependency));
  }

  const facts = document.createElement("p");
  facts.className = "workflow-field-hint";
  facts.textContent = [
    roleLabel(cut.sketch?.shot_role),
    speechLabel(cut.speech_mode),
    cut.render_lane?.replace(/_/g, " ") ?? "lane pending",
    visualSourceStatus(cut),
  ].join(" · ");
  card.append(facts);

  if (!renderQueue) {
    const promptLabel = document.createElement("label");
    promptLabel.className = "workflow-field";
    const promptTitle = document.createElement("span");
    promptTitle.textContent = "Production video prompt";
    const prompt = document.createElement("textarea");
    prompt.rows = 10;
    prompt.value = cut.prompt ?? cut.sketch?.visual_prompt ?? "";
    prompt.placeholder =
      "Describe the moving shot, performance or action, camera behavior, continuity, and sound.";
    promptLabel.append(promptTitle, prompt);

    const negativeLabel = document.createElement("label");
    negativeLabel.className = "workflow-field";
    const negativeTitle = document.createElement("span");
    negativeTitle.textContent = "Negative prompt";
    const negative = document.createElement("textarea");
    negative.rows = 3;
    negative.value = cut.negative_prompt ?? cut.sketch?.negative_prompt ?? "";
    negativeLabel.append(negativeTitle, negative);
    card.append(promptLabel, negativeLabel);

    if (cut.dialogue?.length) {
      const dialogue = document.createElement("p");
      dialogue.className = "workflow-production-dialogue";
      dialogue.textContent = cut.dialogue
        .map((line) => `${line.speaker}: “${line.text}”`)
        .join(" ");
      card.append(dialogue);
    }

    const save = actionButton("Save production prompt", () => {
      emit({
        type: "save-prompt",
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    });
    save.disabled = Boolean(getSurfaceHost().busy) || !cut.production_note_path;
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    const sync = actionButton("Build full video prompt", () => {
      emit({ type: "sync-cut", cut: cut.id.cut });
    });
    sync.disabled = Boolean(getSurfaceHost().busy) || !cut.production_note_path || !cut.sketch;
    sync.title =
      "Synthesize a complete video prompt from the script, cut sequence, current sketch, dialogue, and approved visual references.";
    actions.append(sync, save);
    const notePath = getSurfaceHost().resolvePath(cut.production_note_path);
    if (notePath) {
      actions.append(actionButton("Open full handoff", () => getSurfaceHost().hooks?.onOpenPath(notePath)));
    }
    actions.append(renderCutButton(cut, emit));
    card.append(actions);
  } else {
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    actions.append(renderCutButton(cut, emit));
    card.append(actions);
  }

  const takes = cut.takes ?? [];
  if (takes.length) {
    const takeList = document.createElement("div");
    takeList.className = "workflow-production-takes";
    for (const take of [...takes].reverse()) {
      const preview = document.createElement("figure");
      preview.className = "workflow-production-take";
      const video = document.createElement("video");
      video.controls = true;
      video.preload = "metadata";
      video.playsInline = true;
      video.dataset.takePath = getSurfaceHost().resolvePath(take.path) ?? take.path;
      const caption = document.createElement("figcaption");
      caption.textContent = `${take.path.split("/").pop() ?? take.id} · ${take.status.replace(/_/g, " ")}`;
      const open = actionButton(
        "Open in viewer",
        () => getSurfaceHost().hooks?.onOpenPath(getSurfaceHost().resolvePath(take.path) ?? take.path),
      );
      const reviewActions = document.createElement("div");
      reviewActions.className = "workflow-inspector-actions";
      for (const [status, label] of [
        ["selected", "Select"],
        ["alternate", "Mark alternate"],
        ["rejected", "Reject"],
        ["needs_edit", "Needs edit"],
        ["needs_rerender", "Needs rerender"],
      ] as const) {
        const review = actionButton(label, () => {
          emit({ type: "set-take-status", cut: cut.id.cut, takeId: take.id, status });
        });
        review.disabled = Boolean(getSurfaceHost().busy) || take.status === status;
        reviewActions.append(review);
      }
      reviewActions.append(
        actionButton("Delete video", () => {
          emit({ type: "delete-take", cut: cut.id.cut, takeId: take.id, path: take.path });
        }),
      );
      preview.append(video, caption, open, reviewActions);
      takeList.append(preview);
    }
    card.append(takeList);
    queueTakeVideos(takeList);
  }
  return card;
}

export function renderProductionNotesStage(
  scene: SceneRecord,
  onIntent: (intent: NotesIntent) => void,
): HTMLElement {
  const model = buildNotesModel({
    scene,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
    chatReady: getSurfaceHost().chatReady,
  });
  const view = paintProductionNotesStage(scene, onIntent);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, false);
}
