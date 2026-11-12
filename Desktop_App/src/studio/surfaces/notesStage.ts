import { type CutRecord, type SceneRecord } from "../../workflowTypes";
import { roleLabel, speechLabel } from "../labels";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, newAlternateAngleButton, presentSurface, queueTakeVideos, renderCutButton } from "./shared";
import { dependencyNote, renderBlocker, visualSourceStatus } from "./surfaceSelectors";
import { buildNotesModel } from "./surfaceModels";
import type { NotesIntent } from "./surfaceIntents";
import { renderPerformanceAudio } from "./performanceAudio";
import { renderVideoTakeEdit } from "./videoTakeEdit";

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
      "Build moving-image prompts from the script, scene intent, ordered cut sketches and dialogue. Approved scene references and a visual source are used when available; you do not need separate images of ordinary props to write a prompt. Rendering checks its own source requirements.",
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
  status.textContent = cut.script_dialogue_review
    ? "Script dialogue changed · cut plan needs review"
    : cut.package_needs_review
    ? blocker
      ? `Needs review · render blocked · ${blocker}`
      : `Needs review · ${cut.takes?.length ?? 0} take(s) kept`
    : blocker
      ? `Render blocked · ${blocker}`
      : `${cut.takes?.length ?? 0} take(s) · ready`;
  header.append(heading, status);
  card.append(header);
  if (cut.script_dialogue_review) {
    card.append(block("workflow-banner warn", "Script dialogue changed. The production-note cues and cut sketch below are retained from the earlier plan, not current script dialogue. Compare and resolve this cut in Sketches before treating these as current."));
  }
  if (cut.script_dialogue_kept && !cut.script_dialogue_review) {
    card.append(block("workflow-field-hint", "Authored production-note cues deliberately kept after script revision. These are not current script text."));
  }
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
  if (cut.speech_mode === "source_video_lipsync") card.append(renderPerformanceAudio(cut, emit));

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
    save.disabled = Boolean(getSurfaceHost().busy) || !cut.sketch;
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    const sync = actionButton("Build full video prompt", () => {
      emit({ type: "sync-cut", cut: cut.id.cut });
    });
    sync.disabled = Boolean(getSurfaceHost().busy) || !cut.sketch;
    sync.title =
      "Synthesize a complete video prompt from the script, cut sequence, current sketch, dialogue, and approved visual references.";
    actions.append(sync, save);
    const review = actionButton("Review planning → production note", () => emit({ type: "review-handoff", cut: cut.id.cut }));
    review.disabled = Boolean(getSurfaceHost().busy) || !cut.sketch || !cut.production_note_path;
    actions.append(review);
    const notePath = getSurfaceHost().resolvePath(cut.production_note_path);
    if (notePath) {
      actions.append(actionButton("Open full handoff", () => getSurfaceHost().hooks?.onOpenPath(notePath)));
    }
    if (!cut.selected_performance_asset_id) actions.append(renderCutButton(cut, emit));
    if (cut.speech_mode === "source_video_lipsync" && !cut.selected_performance_asset_id) {
      const resume = actionButton("Resume with saved audio", () => emit({ type: "resume-audio", cut: cut.id.cut }));
      resume.disabled = Boolean(getSurfaceHost().busy) || Boolean(renderBlocker(cut));
      resume.title = "Check whether this shot's saved WAV and job history are safe to resume, then prepare a fresh approval only. No speech or video is submitted by this check.";
      actions.append(resume);
    }
    card.append(actions);
  } else {
    const actions = document.createElement("div");
    actions.className = "workflow-inspector-actions";
    if (!cut.selected_performance_asset_id) actions.append(renderCutButton(cut, emit));
    if (cut.speech_mode === "source_video_lipsync" && !cut.selected_performance_asset_id) {
      const resume = actionButton("Resume with saved audio", () => emit({ type: "resume-audio", cut: cut.id.cut }));
      resume.disabled = Boolean(getSurfaceHost().busy) || Boolean(renderBlocker(cut));
      resume.title = "Check whether this shot's saved WAV and job history are safe to resume, then prepare a fresh approval only. No speech or video is submitted by this check.";
      actions.append(resume);
    }
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
  const selectedTake = takes.find(take => take.id === cut.selected_take || take.path === cut.selected_take);
  if (selectedTake) {
    const scene = getSurfaceHost().state?.scenes?.find(item => item.id === cut.id.source_scene);
    const shot = scene?.animatic?.shots?.find(item => item.cut_id.source_scene === cut.id.source_scene && item.cut_id.cut === cut.id.cut);
    if (shot) {
      card.append(renderVideoTakeEdit(shot, selectedTake, intent => {
        if (intent.type === "set-shot-trim") emit(intent);
      }));
    } else {
      card.append(block("workflow-field-hint", "To edit this selected take, create its sketch animatic timing first."));
    }
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
