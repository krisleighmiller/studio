import { VISUAL_SOURCE_STRATEGIES, type AssetRecord, type CutRecord, type SceneRecord, type SceneReport, type VisualSourceStrategy } from "../../workflowTypes";
import { continuityChoice } from "../selectors/continuityChoice";
import { getSurfaceHost, startFrameEditBriefs } from "./session";
import { actionButton, block, findAsset, missingPanel, presentSurface, queueLookThumbs, queueTakeVideos } from "./shared";
import { approvedSceneLookAssets, cutImageAssets, effectiveVisualSourceStrategy, visualSourceHint, visualSourceStatus } from "./surfaceSelectors";
import { buildReferenceModel } from "./surfaceModels";
import type { ReferenceIntent } from "./surfaceIntents";

function paintReferenceImagesStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  emit: (intent: ReferenceIntent) => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-start-frames";
  const planned = sceneStatus?.planned_visual_sources ?? 0;
  const ready = sceneStatus?.render_ready_sources ?? 0;
  wrap.append(
    block(
      planned ? "workflow-banner" : "workflow-missing",
      planned
        ? `${planned} of ${scene.cuts.length} cuts have a planned visual-source strategy; ${ready} are render ready. Continuity-linked cuts remain correctly planned while they wait for a selected predecessor take.`
        : "No visual-source strategies are approved. Choose how each cut obtains its opening image; not every cut needs a preproduction start frame.",
    ),
  );
  if (!scene.cuts.length) {
    wrap.append(
      missingPanel(
        "No cuts are available.",
        "Draft cut sketches before generating cut-specific start frames.",
      ),
    );
    return wrap;
  }
  for (const cut of scene.cuts) {
    wrap.append(renderStartFrameCard(scene, cut, emit));
  }
  queueLookThumbs(wrap);
  return wrap;
}

export function renderStartFrameCard(
  scene: SceneRecord,
  cut: CutRecord,
  emit: (intent: ReferenceIntent) => void,
  embedded = false,
): HTMLElement {
  const card = document.createElement(embedded ? "section" : "article");
  card.className = `workflow-start-frame-card${embedded ? " embedded" : ""}`;
  const header = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = embedded
    ? "Visual source and reference image"
    : `Cut ${cut.id.cut} · ${cut.title?.trim() || "Untitled"}`;
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  const currentStrategy = effectiveVisualSourceStrategy(cut);
  status.textContent = visualSourceStatus(cut);
  header.append(heading, status);
  card.append(header);

  const sourceControls = document.createElement("section");
  sourceControls.className = "workflow-source-controls";
  const strategyLabel = document.createElement("label");
  strategyLabel.className = "workflow-field";
  const strategyTitle = document.createElement("span");
  strategyTitle.textContent = "Visual-source strategy";
  const strategySelect = document.createElement("select");
  for (const option of VISUAL_SOURCE_STRATEGIES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    strategySelect.append(item);
  }
  strategySelect.value = currentStrategy ?? "preproduction_start_frame";
  strategyLabel.append(strategyTitle, strategySelect);

  const predecessorLabel = document.createElement("label");
  predecessorLabel.className = "workflow-field";
  const predecessorTitle = document.createElement("span");
  predecessorTitle.textContent = "Predecessor cut";
  const predecessorSelect = document.createElement("select");
  const earlierCuts = scene.cuts.filter((candidate) => candidate.id.cut < cut.id.cut);
  for (const candidate of earlierCuts) {
    const item = document.createElement("option");
    item.value = String(candidate.id.cut);
    item.textContent = `Cut ${candidate.id.cut} · ${candidate.title?.trim() || "Untitled"}`;
    predecessorSelect.append(item);
  }
  predecessorSelect.value = String(
    cut.visual_source?.predecessor?.cut ??
      earlierCuts[earlierCuts.length - 1]?.id.cut ??
      "",
  );
  predecessorLabel.append(predecessorTitle, predecessorSelect);

  const sharedLabel = document.createElement("label");
  sharedLabel.className = "workflow-field";
  const sharedTitle = document.createElement("span");
  sharedTitle.textContent = "Approved shared reference";
  const sharedSelect = document.createElement("select");
  const emptyShared = document.createElement("option");
  emptyShared.value = "";
  emptyShared.textContent = "Choose an approved scene reference";
  sharedSelect.append(emptyShared);
  for (const asset of approvedSceneLookAssets(scene, getSurfaceHost().state?.assets ?? [])) {
    const item = document.createElement("option");
    item.value = asset.id;
    item.textContent = asset.path.split("/").pop() ?? asset.id;
    sharedSelect.append(item);
  }
  sharedSelect.value = cut.visual_source?.asset_id ?? "";
  sharedLabel.append(sharedTitle, sharedSelect);

  const sourceHint = document.createElement("p");
  sourceHint.className = "workflow-field-hint";
  const boundAsset = cut.visual_source?.asset_id
    ? findAsset(cut.visual_source.asset_id)
    : undefined;
  sourceHint.textContent = visualSourceHint(cut, boundAsset);

  const sourceActions = document.createElement("div");
  sourceActions.className = "workflow-inspector-actions";
  const saveSource = actionButton("Save source strategy", () => {
    const strategy = strategySelect.value;
    emit({
      type: "set-source",
      cut: cut.id.cut,
      strategy,
      predecessorCut: strategy === "previous_selected_take_exit_frame" ? Number(predecessorSelect.value) : null,
      assetId: strategy === "shared_reference" && sharedSelect.value ? sharedSelect.value : null,
    });
  });
  sourceActions.append(saveSource);
  const frameTimeLabel = document.createElement("label");
  frameTimeLabel.className = "workflow-field";
  const frameTimeTitle = document.createElement("span");
  frameTimeTitle.textContent = "Frame time (seconds)";
  const frameTime = document.createElement("input");
  frameTime.type = "number";
  frameTime.min = "0";
  frameTime.step = "0.1";
  frameTime.value =
    cut.visual_source?.frame_time_seconds != null
      ? String(cut.visual_source.frame_time_seconds)
      : "";
  frameTimeLabel.append(frameTimeTitle, frameTime);
  const preview = document.createElement("video");
  preview.controls = true;
  preview.muted = true;
  preview.preload = "metadata";
  preview.className = "workflow-take-video";
  let frameTimeEdited = frameTime.value !== "";
  frameTime.addEventListener("input", () => {
    frameTimeEdited = true;
  });
  preview.addEventListener("loadedmetadata", () => {
    if (frameTimeEdited || !Number.isFinite(preview.duration)) return;
    const choice = continuityChoice({
      strategy: strategySelect.value,
      predecessorTakeSelected: true,
      durationSeconds: preview.duration,
      editedTimeSeconds: null,
      boundAssetId: cut.visual_source?.asset_id,
      dependentLabels: [],
      busy: getSurfaceHost().busy,
    });
    if (choice.timeSeconds != null) frameTime.value = choice.timeSeconds.toFixed(3);
  });
  const bindContinuity = actionButton("Bind selected frame", () => {
    emit({
      type: "bind-frame",
      cut: cut.id.cut,
      predecessorCut: Number(predecessorSelect.value),
      frameTimeSeconds: Number(frameTime.value),
    });
  });
  sourceActions.append(bindContinuity);
  sourceControls.append(
    strategyLabel,
    predecessorLabel,
    frameTimeLabel,
    preview,
    sharedLabel,
    sourceHint,
    sourceActions,
  );
  card.append(sourceControls);

  const toggleSourceFields = (): void => {
    const strategy = strategySelect.value as VisualSourceStrategy;
    const continuity = strategy === "previous_selected_take_exit_frame";
    predecessorLabel.hidden = !continuity;
    frameTimeLabel.hidden = !continuity;
    preview.hidden = !continuity;
    sharedLabel.hidden = strategy !== "shared_reference";
    bindContinuity.hidden = !continuity;
    const predecessor = earlierCuts.find(
      (candidate) => candidate.id.cut === Number(predecessorSelect.value),
    );
    const take = predecessor?.takes?.find(
      (candidate) =>
        candidate.id === predecessor?.selected_take ||
        candidate.path === predecessor?.selected_take,
    );
    const choice = continuityChoice({
      strategy,
      predecessorTakeSelected: Boolean(take),
      durationSeconds: Number.isFinite(preview.duration) ? preview.duration : null,
      editedTimeSeconds: frameTimeEdited && frameTime.value !== "" ? Number(frameTime.value) : null,
      boundAssetId: cut.visual_source?.asset_id,
      dependentLabels: predecessor ? getSurfaceHost().continuityDependents(predecessor) : [],
      busy: getSurfaceHost().busy,
    });
    bindContinuity.disabled = !choice.bindEnabled;
    bindContinuity.title =
      choice.bindReason ??
      "Extract the frame at the chosen time. The successor stays blocked until this bind is saved.";
    if (take) {
      preview.dataset.takePath = getSurfaceHost().resolvePath(take.path) ?? take.path;
    } else {
      delete preview.dataset.takePath;
      preview.removeAttribute("src");
    }
    queueTakeVideos(sourceControls);
  };
  strategySelect.addEventListener("change", toggleSourceFields);
  predecessorSelect.addEventListener("change", toggleSourceFields);
  toggleSourceFields();

  const directSource = document.createElement("section");
  directSource.className = "workflow-direct-start-frame";
  const promptLabel = document.createElement("label");
  promptLabel.className = "workflow-field";
  const promptTitle = document.createElement("span");
  promptTitle.textContent = "Start-frame prompt";
  const prompt = document.createElement("textarea");
  prompt.rows = 4;
  prompt.value = cut.start_frame_prompt ?? cut.sketch?.visual_prompt ?? cut.prompt ?? "";
  promptLabel.append(promptTitle, prompt);
  const negativeLabel = document.createElement("label");
  negativeLabel.className = "workflow-field";
  const negativeTitle = document.createElement("span");
  negativeTitle.textContent = "Negative prompt";
  const negative = document.createElement("textarea");
  negative.rows = 2;
  negative.value =
    cut.start_frame_negative_prompt ??
    cut.sketch?.negative_prompt ??
    cut.negative_prompt ??
    "";
  negativeLabel.append(negativeTitle, negative);
  directSource.append(promptLabel, negativeLabel);

  const promptActions = document.createElement("div");
  promptActions.className = "workflow-inspector-actions";
  promptActions.append(
    actionButton("Draft image prompt", () => {
      emit({ type: "draft-prompt", cut: cut.id.cut });
    }),
    actionButton("Save prompt", () => {
      emit({
        type: "save-prompt",
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    }),
    actionButton("Generate start frame", () => {
      emit({ type: "generate", cut: cut.id.cut });
    }),
    actionButton("Attach still", () => {
      emit({ type: "attach-still", cut: cut.id.cut });
    }),
  );
  directSource.append(promptActions);

  const assets = cutImageAssets(getSurfaceHost().state?.assets ?? [], scene.id, cut.id.cut);
  const versions = document.createElement("div");
  versions.className = "workflow-start-frame-versions";
  if (!assets.length) {
    versions.append(
      block(
        "empty-state",
        "No versions yet. Generated and attached images are saved under this cut’s start_frames folder.",
      ),
    );
  } else {
    for (const asset of assets) {
      versions.append(renderStartFrameVersion(scene, cut, asset, emit));
    }
  }
  directSource.append(versions);

  const editKey = `${scene.id}.${cut.id.cut}`;
  const editLabel = document.createElement("label");
  editLabel.className = "workflow-field workflow-start-frame-edit";
  const editTitle = document.createElement("span");
  editTitle.textContent = "Edit brief";
  const editBrief = document.createElement("textarea");
  editBrief.rows = 2;
  editBrief.placeholder =
    "Describe only the change; the selected source version and approved references are preserved.";
  editBrief.value = startFrameEditBriefs.get(editKey) ?? "";
  editBrief.addEventListener("input", () => {
    startFrameEditBriefs.set(editKey, editBrief.value);
  });
  editLabel.append(editTitle, editBrief);
  directSource.append(editLabel);
  directSource.hidden = strategySelect.value !== "preproduction_start_frame";
  strategySelect.addEventListener("change", () => {
    directSource.hidden = strategySelect.value !== "preproduction_start_frame";
  });
  card.append(directSource);
  return card;
}

function renderStartFrameVersion(
  scene: SceneRecord,
  cut: CutRecord,
  asset: AssetRecord,
  emit: (intent: ReferenceIntent) => void,
): HTMLElement {
  const figure = document.createElement("figure");
  figure.className = `workflow-start-frame-version${
    cut.approved_start_frame === asset.path ? " approved" : ""
  }${asset.status === "rejected" ? " rejected" : ""}`;
  const image = document.createElement("img");
  image.alt = `Scene ${scene.id} Cut ${cut.id.cut} start-frame version`;
  image.dataset.lookPath = getSurfaceHost().resolvePath(asset.path) ?? "";
  image.addEventListener("click", () => {
    getSurfaceHost().hooks?.onOpenPath(getSurfaceHost().resolvePath(asset.path) ?? asset.path);
  });
  const caption = document.createElement("figcaption");
  caption.textContent = `${asset.path.split("/").pop() ?? asset.id} · ${
    cut.approved_start_frame === asset.path ? "approved source" : (asset.status ?? "draft")
  }`;
  const actions = document.createElement("div");
  actions.className = "workflow-start-frame-version-actions";
  actions.append(
    actionButton("Use", () => {
      emit({ type: "approve", cut: cut.id.cut, assetId: asset.id });
    }),
    actionButton("Reject", () => {
      emit({ type: "reject", cut: cut.id.cut, assetId: asset.id });
    }),
    actionButton("Edit", () => {
      emit({
        type: "edit",
        cut: cut.id.cut,
        assetId: asset.id,
        brief: startFrameEditBriefs.get(`${scene.id}.${cut.id.cut}`)?.trim() ?? "",
      });
    }),
    actionButton("Delete", () => {
      emit({ type: "delete", cut: cut.id.cut, assetId: asset.id, path: asset.path });
    }),
  );
  figure.append(image, caption, actions);
  return figure;
}

export function renderReferenceImagesStage(
  scene: SceneRecord,
  sceneStatus: SceneReport | undefined,
  onIntent: (intent: ReferenceIntent) => void,
): HTMLElement {
  const model = buildReferenceModel({
    scene,
    report: sceneStatus,
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
    blocked: getSurfaceHost().blockerNames(scene.id),
    chatReady: getSurfaceHost().chatReady,
  });
  return presentSurface(paintReferenceImagesStage(scene, sceneStatus, onIntent), model, true);
}
