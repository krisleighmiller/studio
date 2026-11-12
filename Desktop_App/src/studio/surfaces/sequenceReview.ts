import type { AnimaticShot, JobRecord, SceneRecord } from "../../workflowTypes";
import { getRoughPlayback, getSurfaceHost, setRoughPlayback, stopRoughCut } from "./session";
import { actionButton, block } from "./shared";
import { cutForShot } from "./surfaceSelectors";
import { renderAnimaticTiming } from "./animaticTiming";
import { renderedSnapshotCurrent } from "./renderedSnapshot";
import type { AssembleIntent, SketchIntent } from "./surfaceIntents";

const TRANSITIONS: { value: string; label: string }[] = [
  { value: "", label: "Cut" },
  { value: "dissolve", label: "Dissolve" },
  { value: "fade_through_black", label: "Fade through black" },
];

/** Review the actual assembled bytes, never a timed slideshow of planning stills. */
export function renderRenderedSequence(job: JobRecord | undefined, emit: (intent: AssembleIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const section = document.createElement("section");
  section.className = "workflow-production-take";
  section.setAttribute("aria-label", "Rendered rough-cut review");
  const heading = document.createElement("h3");
  heading.textContent = "Rendered rough-cut review";
  section.append(heading);
  if (!job?.output_path) {
    section.append(block("workflow-field-hint", "No completed project draft yet. Prepare project master below, then return here after the local assembly completes."));
    return section;
  }
  const current = renderedSnapshotCurrent(host.state?.scenes ?? [], job.assembly_snapshot);
  const approved = current && host.state?.rendered_review?.job_id === job.id;
  section.append(block("workflow-field-hint", approved
    ? "Approved rendered sequence · this saved project master. Sketch planning approval is separate. Changing selected takes, order, trims, duration or transitions clears this review."
    : current ? "Draft · not a creative approval. Play, pause and seek the assembled rendered takes below. This is the saved assembly, not a sketch preview; prepare a new master after changing selections or timing."
    : "Stale master · playback shows the previous saved sequence, not the current selected order/timing. Prepare a new project master before approving."));
  const video = document.createElement("video");
  video.controls = true;
  video.preload = "metadata";
  video.playsInline = true;
  video.setAttribute("aria-label", "Selected rendered takes · saved project master");
  const status = block("workflow-field-hint", `Loading ${job.output_path}…`);
  status.addEventListener("contextmenu", () => {
    const selection = window.getSelection();
    if (!selection || !status.textContent) return;
    const range = document.createRange();
    range.selectNodeContents(status);
    selection.removeAllRanges();
    selection.addRange(range);
  });
  const seekLabel = document.createElement("label");
  seekLabel.className = "workflow-field";
  seekLabel.textContent = "Seek rendered rough cut";
  const seek = document.createElement("input");
  seek.type = "range";
  seek.min = "0";
  seek.max = "0";
  seek.step = "0.1";
  seek.value = "0";
  seek.disabled = true;
  seek.setAttribute("aria-label", "Seek rendered rough cut");
  seek.addEventListener("input", () => { video.currentTime = Number(seek.value); });
  video.addEventListener("timeupdate", () => { seek.value = String(video.currentTime); });
  seekLabel.append(seek);
  let playing = false;
  const play = actionButton("Play rendered rough cut", () => {
    if (playing) video.pause();
    else void video.play().catch(() => { status.textContent = "Could not play this master. Try the video controls or Open project draft."; });
  });
  play.disabled = true;
  video.addEventListener("play", () => { playing = true; play.textContent = "Pause rendered rough cut"; });
  const paused = () => { playing = false; play.textContent = "Play rendered rough cut"; };
  video.addEventListener("pause", paused);
  video.addEventListener("ended", paused);
  video.addEventListener("loadedmetadata", () => {
    const valid = Number.isFinite(video.duration) && video.duration > 0;
    seek.max = valid ? String(video.duration) : "0";
    seek.disabled = !valid || Boolean(host.busy);
    status.textContent = `${job.output_path} · ${video.duration.toFixed(2)}s · use the seek slider or video controls`;
  });
  video.addEventListener("error", () => { status.textContent = "Could not load the rendered master. Prepare a new project master if this file is missing or unreadable."; play.disabled = true; seek.disabled = true; });
  section.append(video, play, seekLabel, status);
  const list = document.createElement("ol");
  list.setAttribute("aria-label", "Saved rendered sequence order");
  for (const shot of job.assembly_snapshot ?? []) {
    const row = document.createElement("li");
    row.dataset.takeId = shot.take_id;
    row.textContent = `${shot.take_id} · ${shot.path} · from ${shot.start_seconds.toFixed(2)}s · ${shot.duration_seconds.toFixed(2)}s · ${shot.transition.replace(/_/g, " ")}`;
    list.append(row);
  }
  const approve = actionButton(approved ? "Rendered rough cut approved" : "Approve rendered rough cut", () => emit({ type: "approve-rendered", jobId: job.id }));
  approve.disabled = !current || approved || Boolean(host.busy);
  approve.title = "Record creative review of this exact saved rendered sequence, not permission to generate or spend. A stale master cannot be approved; prepare a new master after timing or take changes.";
  section.append(list, approve);
  for (const scene of host.state?.scenes ?? []) {
    if (!scene.animatic?.shots?.length) section.append(block("workflow-field-hint", `Scene ${scene.id}: no sketch animatic. Rendered review uses the full selected takes for this scene when assembled without timing.`));
  }
  const state = host.state;
  void host.mediaUrl(host.resolvePath(job.output_path) ?? job.output_path).then(url => {
    if (!video.isConnected || getSurfaceHost().state !== state) return;
    video.src = url;
    play.disabled = Boolean(host.busy);
  }).catch(() => {
    if (video.isConnected) status.textContent = "Could not load the rendered master. Try Open project draft or prepare a new master.";
  });
  return section;
}

function paintRoughStage(scene: SceneRecord, shot: AnimaticShot | null): void {
  const stage = document.querySelector<HTMLElement>(".workflow-rough-cut-stage");
  if (!stage || Number(stage.dataset.sceneId) !== scene.id) return;
  stage.replaceChildren();
  if (!shot) {
    stage.hidden = true;
    highlightRoughShot(scene, null);
    return;
  }
  stage.hidden = false;
  const cut = cutForShot(scene, shot);
  const title = document.createElement("h3");
  const index = (scene.animatic?.shots ?? []).findIndex((item) => item.id === shot.id);
  highlightRoughShot(scene, index);
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

function seekRoughCut(scene: SceneRecord, index: number): void {
  const shots = scene.animatic?.shots ?? [];
  if (index < 0 || index >= shots.length) return;
  const playback = getRoughPlayback();
  if (!playback || playback.sceneId !== scene.id) {
    stopRoughCut();
    setRoughPlayback({ sceneId: scene.id, index, timer: 0, audio: null });
    getSurfaceHost().rerender();
    void showRoughShot(scene, index);
    return;
  }
  window.clearTimeout(playback.timer);
  playback.audio?.pause();
  playback.audio = null;
  playback.index = index;
  getSurfaceHost().rerender();
  void showRoughShot(scene, index);
}

function highlightRoughShot(scene: SceneRecord, index: number | null): void {
  const wanted = String(scene.id);
  for (const section of document.querySelectorAll<HTMLElement>(".workflow-animatic")) {
    if ((section.dataset.sceneId ?? "") !== wanted) continue;
    section
      .querySelectorAll<HTMLElement>(".workflow-animatic-row")
      .forEach((row) => row.classList.toggle("playing", Number(row.dataset.index) === index));
    section
      .querySelectorAll<HTMLElement>(".workflow-animatic-segment")
      .forEach((segment) =>
        segment.classList.toggle("active", Number(segment.dataset.index) === index),
      );
  }
}

export function renderAnimatic(scene: SceneRecord, emit: (intent: SketchIntent) => void): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-animatic";
  section.setAttribute("aria-label", "Animatic");
  section.dataset.sceneId = String(scene.id);
  const shots = scene.animatic?.shots ?? [];
  const total = shots.reduce((sum, shot) => sum + (shot.duration_seconds || 0), 0);
  const heading = document.createElement("h2");
  heading.textContent = scene.animatic?.approved
    ? `Sketch animatic · planning approved · ${total.toFixed(1)}s`
    : `Sketch animatic · ${total.toFixed(1)}s`;
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Planning preview only: stills, scratch audio and temporary dialogue, not rendered film. Review the rendered sequence in Assemble / Edit after preparing a project master. Timing and order sit on the cut sketches; changes keep existing renders.";
  section.append(heading, note);

  if (!shots.length) {
    section.append(block("workflow-field-hint", `No sketch animatic for Scene ${scene.id}. Selected rendered takes can still be assembled and reviewed in Assemble / Edit.`));
    return section;
  }

  const stage = document.createElement("div");
  stage.className = "workflow-rough-cut-stage";
  stage.dataset.sceneId = String(scene.id);
  stage.hidden = getRoughPlayback()?.sceneId !== scene.id;
  section.append(stage);

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  controls.append(
    actionButton(getRoughPlayback()?.sceneId === scene.id ? "Stop sketch preview" : "Play sketch preview", () => {
      if (getRoughPlayback()?.sceneId === scene.id) {
        stopRoughCut();
        getSurfaceHost().rerender();
        return;
      }
      playRoughCut(scene);
    }),
    actionButton(scene.animatic?.approved ? "Sketch animatic approved" : "Approve sketch animatic", () => {
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
    item.dataset.index = String(index);
    const label = document.createElement("span");
    label.className = "workflow-animatic-label";
    label.textContent = `Cut ${shot.cut_id.cut} · ${cut?.sketch?.title || cut?.title || "Untitled"}`;
    label.title = "Seek to this shot";
    label.addEventListener("click", () => seekRoughCut(scene, index));
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
    item.append(renderAnimaticTiming(shot, cut, emit));
    list.append(item);
  });
  section.append(list);

  const timeline = document.createElement("div");
  timeline.className = "workflow-animatic-timeline";
  timeline.setAttribute("aria-label", "Sketch animatic timeline");
  const totalSecs = shots.reduce((sum, shot) => sum + (shot.duration_seconds || 0), 0);
  shots.forEach((shot, index) => {
    const segment = document.createElement("button");
    segment.type = "button";
    segment.className = "workflow-animatic-segment";
    segment.dataset.index = String(index);
    if (totalSecs > 0) {
      segment.style.width = `${((shot.duration_seconds || 0) / totalSecs) * 100}%`;
    }
    segment.title = `Seek to cut ${shot.cut_id.cut} (${shot.duration_seconds.toFixed(2)}s)`;
    segment.addEventListener("click", () => seekRoughCut(scene, index));
    timeline.append(segment);
  });
  section.append(timeline);

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