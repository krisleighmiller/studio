import { type SceneRecord, type TakeRecord } from "../../workflowTypes";
import { getSurfaceHost } from "./session";
import { actionButton, block, missingPanel, presentSurface } from "./shared";
import { buildAssembleModel } from "./surfaceModels";
import type { AssembleIntent } from "./surfaceIntents";
import { renderRenderedSequence } from "./sequenceReview";
import { renderVideoTakeEdit } from "./videoTakeEdit";

function paintAssembleStage(scene: SceneRecord, emit: (intent: AssembleIntent) => void): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-production-notes";
  wrap.append(renderProjectAssembly(emit));
  const missing: number[] = [];
  const staleEdits: number[] = [];
  const selected: Array<{ cut: number; take: TakeRecord }> = [];
  for (const cut of scene.cuts) {
    const take = (cut.takes ?? []).find(
      (candidate) =>
        candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    if (take) selected.push({ cut: cut.id.cut, take });
    else missing.push(cut.id.cut);
    const shot = scene.animatic?.shots?.find(shot => shot.cut_id.source_scene === cut.id.source_scene && shot.cut_id.cut === cut.id.cut);
    if (shot?.trim_source_take_id && shot.trim_source_take_id !== take?.id) staleEdits.push(cut.id.cut);
  }

  wrap.append(
    block(
      missing.length ? "workflow-missing" : "workflow-banner",
      missing.length
        ? `Select one take for every cut before assembly. Missing: ${missing
            .map((cut) => `Cut ${cut}`)
            .join(", ")}.`
        : `${selected.length} selected takes are ready to assemble in animatic order.`,
    ),
  );

  const summary = document.createElement("article");
  if (staleEdits.length) wrap.append(block("workflow-missing", `Saved video edit belongs to a different take on cut ${staleEdits.join(", ")}. Preview and save a new source window before assembly.`));
  summary.className = "workflow-script";
  const heading = document.createElement("h3");
  heading.textContent = `Scene ${scene.id} master`;
  const list = document.createElement("ol");
  list.className = "workflow-readiness";
  const timedCuts = scene.animatic?.shots?.map(shot => scene.cuts.find(cut => cut.id.source_scene === shot.cut_id.source_scene && cut.id.cut === shot.cut_id.cut));
  const displayCuts = timedCuts && timedCuts.length === scene.cuts.length && new Set(timedCuts).size === scene.cuts.length && timedCuts.every(Boolean) ? timedCuts as typeof scene.cuts : scene.cuts;
  for (const [index, cut] of displayCuts.entries()) {
    const entry = document.createElement("li");
    const take = selected.find((item) => item.cut === cut.id.cut)?.take;
    entry.className = take ? "ready" : "missing";
    entry.textContent = take
      ? `Cut ${cut.id.cut} · ${take.path.split("/").pop() ?? take.id}`
      : `Cut ${cut.id.cut} · no selected take`;
    const timing = scene.animatic?.shots?.find(shot => shot.cut_id.source_scene === cut.id.source_scene && shot.cut_id.cut === cut.id.cut);
    if (timing) entry.textContent += ` · start ${(timing.trim_in_seconds ?? 0).toFixed(2)}s · ${(timing.duration_seconds - (timing.trim_in_seconds ?? 0) - (timing.trim_out_seconds ?? 0)).toFixed(2)}s · ${(timing.transition ?? "cut").replace(/_/g, " ")}`;
    if (scene.animatic?.shots?.length === scene.cuts.length && timing) {
      for (const [label, delta] of [["Move earlier", -1], ["Move later", 1]] as const) {
        const move = actionButton(label, () => {
          const ids = scene.animatic!.shots!.map(shot => shot.id);
          const [id] = ids.splice(index, 1);
          ids.splice(index + delta, 0, id!);
          emit({ type: "reorder-sequence", shotIds: ids });
        });
        move.disabled = Boolean(getSurfaceHost().busy) || index + delta < 0 || index + delta >= displayCuts.length;
        move.setAttribute("aria-label", `${label} rendered cut ${cut.id.cut}`);
        entry.append(move);
      }
    }
    if (timing && take) entry.append(renderVideoTakeEdit(timing, take, emit));
    list.append(entry);
  }

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  const assemble = actionButton(`Prepare Scene ${scene.id} master`, () => {
    emit({ type: "prepare" });
  });
  assemble.disabled = Boolean(getSurfaceHost().busy) || missing.length > 0 || staleEdits.length > 0 || scene.cuts.length === 0;
  assemble.title = missing.length
    ? "Select a take for every cut in Production Notes first."
    : "Prepare a local ffmpeg assembly for approval. No provider credits are used.";
  const reviewPath = getSurfaceHost().reviewExportPath(scene);
  const openReview = actionButton("Open review export", () => {
    if (!reviewPath) return;
    getSurfaceHost().hooks?.onOpenPath(getSurfaceHost().resolvePath(reviewPath) ?? reviewPath);
  });
  openReview.disabled = Boolean(getSurfaceHost().busy) || !reviewPath;
  openReview.title = reviewPath
    ? reviewPath
    : "No review export yet. Prepare an assembly and refresh after the file is written.";
  controls.append(assemble, openReview);
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Assembly follows saved animatic order, trims and transitions when present. The approval card shows host-validated source windows; FFmpeg writes a versioned local master to Final/.";
  summary.append(heading, list, controls, note);
  wrap.append(summary, renderPostPanel(scene, missing.length > 0, emit));
  return wrap;
}

function renderProjectAssembly(emit: (intent: AssembleIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const scenes = [...(host.state?.scenes ?? [])].sort((a, b) => a.id - b.id);
  const article = document.createElement("article");
  article.className = "workflow-script";
  const heading = document.createElement("h3");
  heading.textContent = "Project draft · all selected takes";
  const list = document.createElement("ol");
  list.className = "workflow-readiness";
  let missing = scenes.length === 0;
  for (const scene of scenes) {
    if (!scene.cuts.length) missing = true;
    const timed = scene.animatic?.shots;
    const cuts = timed?.length
      ? timed.map(shot => scene.cuts.find(cut => cut.id.cut === shot.cut_id.cut)).filter((cut): cut is typeof scene.cuts[number] => Boolean(cut))
      : [...scene.cuts].sort((a, b) => a.id.cut - b.id.cut);
    for (const cut of cuts) {
      const take = cut.takes?.find(take => take.id === cut.selected_take || take.path === cut.selected_take);
      if (!take) missing = true;
      const shot = scene.animatic?.shots?.find(shot => shot.cut_id.source_scene === cut.id.source_scene && shot.cut_id.cut === cut.id.cut);
      if (shot?.trim_source_take_id && shot.trim_source_take_id !== take?.id) missing = true;
      const item = document.createElement("li");
      item.className = take ? "ready" : "missing";
      item.textContent = `Scene ${scene.id} Cut ${cut.id.cut} · ${take?.path ?? "no selected take"}`;
      list.append(item);
    }
  }
  const prepare = actionButton("Prepare project master", () => emit({ type: "prepare-project" }));
  prepare.disabled = Boolean(host.busy) || missing;
  prepare.title = "Prepare all scenes in scene-number order, with saved animatic order/timing or full selected takes. Requires approval before local FFmpeg runs.";
  const completed = [...(host.state?.jobs ?? [])].reverse().find(job => job.kind === "assemble" && job.status === "completed" && job.scope?.kind === "project" && job.raw_provider_id === "project" && job.output_path);
  const path = completed?.output_path;
  article.prepend(renderRenderedSequence(completed, emit));
  const open = actionButton("Open project draft", () => {
    if (path) host.hooks?.onOpenPath(host.resolvePath(path) ?? path);
  });
  open.disabled = Boolean(host.busy) || !path;
  open.title = path ?? "No completed project draft yet.";
  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  controls.append(prepare, open);
  article.append(heading, list, controls, block("workflow-field-hint", "Local FFmpeg only. Preparation shows the exact sources and timing for authorization; the resulting master stays a draft, not a creative approval."));
  return article;
}

const CUT_VERSION_STAGES: Array<{ id: string; label: string }> = [
  { id: "selects", label: "Selects" },
  { id: "assembly", label: "Assembly" },
  { id: "fine_cut", label: "Fine cut" },
  { id: "picture_lock", label: "Picture lock" },
];

const POST_FIELDS: Array<{ id: string; label: string }> = [
  { id: "audio_mix", label: "Audio mix" },
  { id: "vfx", label: "VFX" },
  { id: "compositing", label: "Compositing" },
  { id: "captions", label: "Captions" },
  { id: "mastering", label: "Mastering" },
  { id: "qc", label: "QC" },
];

const POST_STATUSES: Array<{ id: string; label: string }> = [
  { id: "not_started", label: "Not started" },
  { id: "in_progress", label: "In progress" },
  { id: "needs_review", label: "Needs review" },
  { id: "approved", label: "Approved" },
];

function postStatus(post: SceneRecord["post"], field: string): string {
  switch (field) {
    case "audio_mix":
      return post?.audio_mix ?? "not_started";
    case "vfx":
      return post?.vfx ?? "not_started";
    case "compositing":
      return post?.compositing ?? "not_started";
    case "captions":
      return post?.captions ?? "not_started";
    case "mastering":
      return post?.mastering ?? "not_started";
    case "qc":
      return post?.qc ?? "not_started";
    default:
      return "not_started";
  }
}

function renderPostPanel(
  scene: SceneRecord,
  takesMissing: boolean,
  emit: (intent: AssembleIntent) => void,
): HTMLElement {
  const post = scene.post;
  const article = document.createElement("article");
  article.className = "workflow-script";
  const heading = document.createElement("h3");
  heading.textContent = "Cut versions";
  article.append(heading);

  const versions = post?.versions ?? [];
  if (versions.length === 0) {
    article.append(
      block(
        "workflow-banner",
        "No cut version yet. Marking snapshots the selected takes and leaves those files in place.",
      ),
    );
  }
  for (const version of versions) {
    const row = document.createElement("div");
    row.className = "workflow-field inline";
    const label = document.createElement("span");
    const locked = post?.picture_locked_id === version.id;
    label.textContent = `${version.label} · ${version.shots.length} shots${
      version.assembly_path ? ` · ${version.assembly_path}` : ""
    }${locked ? " · locked" : ""}`;
    const stage = document.createElement("select");
    stage.setAttribute("aria-label", `${version.label} stage`);
    stage.disabled = Boolean(getSurfaceHost().busy);
    for (const option of CUT_VERSION_STAGES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === version.stage;
      stage.append(choice);
    }
    stage.addEventListener("change", () => {
      emit({
        type: "set-version-stage",
        versionId: version.id,
        stage: stage.value,
        label: version.label,
      });
    });
    row.append(label, stage);
    article.append(row);
  }

  const controls = document.createElement("div");
  controls.className = "workflow-inspector-actions";
  const mark = actionButton("Mark selects as cut version", () => {
    emit({ type: "mark-version" });
  });
  mark.disabled = Boolean(getSurfaceHost().busy) || takesMissing || scene.cuts.length === 0;
  mark.title = takesMissing
    ? "Select a take for every cut in Production Notes first."
    : "Snapshot the selected takes as a cut version. Files stay where they are.";
  controls.append(mark);
  const latest = versions[versions.length - 1];
  if (latest && post?.picture_locked_id !== latest.id) {
    const lock = actionButton("Lock picture", () => {
      emit({ type: "lock-picture", versionId: latest.id });
    });
    controls.append(lock);
  }
  article.append(controls);

  const editorial = document.createElement("div");
  editorial.className = "workflow-field";
  const editorialLabel = document.createElement("span");
  editorialLabel.textContent = "Editorial path";
  const pathSelect = document.createElement("select");
  pathSelect.setAttribute("aria-label", "Editorial path");
  pathSelect.disabled = Boolean(getSurfaceHost().busy);
  for (const option of [
    { id: "host_ffmpeg", label: "Host ffmpeg" },
    { id: "external_nle", label: "External NLE" },
  ]) {
    const choice = document.createElement("option");
    choice.value = option.id;
    choice.textContent = option.label;
    choice.selected = option.id === (post?.editorial_path ?? "host_ffmpeg");
    pathSelect.append(choice);
  }
  const noteInput = document.createElement("input");
  noteInput.type = "text";
  noteInput.placeholder = "Why this edit leaves the host";
  noteInput.setAttribute("aria-label", "External NLE note");
  noteInput.value = post?.external_nle_note ?? "";
  noteInput.disabled = Boolean(getSurfaceHost().busy);
  const saveEditorial = actionButton("Record editorial path", () => {
    emit({ type: "set-editorial", path: pathSelect.value, note: noteInput.value });
  });
  editorial.append(editorialLabel, pathSelect, noteInput, saveEditorial);
  const editorialHint = document.createElement("p");
  editorialHint.className = "workflow-field-hint";
  editorialHint.textContent =
    "Host ffmpeg stays the assembly above. Recording an external NLE stores that decision and leaves the cut version and take files in place.";
  article.append(editorial, editorialHint);

  const statuses = document.createElement("div");
  statuses.className = "workflow-post-statuses";
  for (const field of POST_FIELDS) {
    const row = document.createElement("label");
    row.className = "workflow-field inline";
    row.textContent = field.label;
    const select = document.createElement("select");
    select.setAttribute("aria-label", field.label);
    select.disabled = Boolean(getSurfaceHost().busy);
    const current = postStatus(post, field.id);
    for (const option of POST_STATUSES) {
      const choice = document.createElement("option");
      choice.value = option.id;
      choice.textContent = option.label;
      choice.selected = option.id === current;
      select.append(choice);
    }
    select.addEventListener("change", () => {
      emit({ type: "set-post-status", field: field.id, status: select.value, label: field.label });
    });
    row.append(select);
    statuses.append(row);
  }
  article.append(statuses);

  const deliverables = getSurfaceHost().state?.profile?.deliverables ?? [];
  const checkRow = document.createElement("div");
  checkRow.className = "workflow-inspector-actions";
  if (deliverables.length === 0) {
    article.append(
      block("workflow-banner", "Add a named deliverable on the profile before a QC check."),
    );
  } else {
    const deliverableSelect = document.createElement("select");
    deliverableSelect.setAttribute("aria-label", "Deliverable");
    deliverableSelect.disabled = Boolean(getSurfaceHost().busy);
    for (const deliverable of deliverables) {
      const choice = document.createElement("option");
      choice.value = deliverable.id;
      const spec = [deliverable.resolution, deliverable.aspect_ratio].filter(Boolean).join(" ");
      choice.textContent = spec ? `${deliverable.name} · ${spec}` : deliverable.name;
      choice.selected = post?.last_check?.deliverable_id === deliverable.id;
      deliverableSelect.append(choice);
    }
    const check = actionButton("Check deliverable", () => {
      emit({ type: "check-deliverable", deliverableId: deliverableSelect.value });
    });
    checkRow.append(deliverableSelect, check);
    article.append(checkRow);
  }
  const last = post?.last_check;
  if (last) {
    article.append(
      block(
        last.passed ? "workflow-banner" : "workflow-missing",
        `${last.passed ? "Passed" : "Failed"}. ${(last.messages ?? []).join(" ")}`,
      ),
    );
  }
  return article;
}

export function renderAssembleStage(
  scene: SceneRecord,
  onIntent: (intent: AssembleIntent) => void,
): HTMLElement {
  const model = buildAssembleModel({
    scene,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
    reviewPath: getSurfaceHost().reviewExportPath(scene),
  });
  const view = paintAssembleStage(scene, onIntent);
  if (model.emptyTitle && model.emptyDetail) view.append(missingPanel(model.emptyTitle, model.emptyDetail));
  return presentSurface(view, model, true);
}
