import { VISUAL_SOURCE_STRATEGIES, type AssetRecord, type CutRecord, type SceneRecord, type SceneReport, type VisualSourceStrategy } from "../../workflowTypes";
import { continuityChoice } from "../selectors/continuityChoice";
import { getSurfaceHost, startFrameEditBriefs } from "./session";
import { actionButton, block, findAsset, queueTakeVideos } from "./shared";
import { approvedSceneLookAssets, cutImageAssets, effectiveVisualSourceStrategy, visualSourceHint, visualSourceStatus } from "./surfaceSelectors";
import type { ReferenceIntent } from "./surfaceIntents";
import { orderedReferencePicker } from "../orderedReferencePicker";

function paintReferenceImagesStage(
  scene: SceneRecord,
  _sceneStatus: SceneReport | undefined,
  emit: (intent: ReferenceIntent) => void,
  imageWorkspace?: (slotId: string) => HTMLElement,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "workflow-reference-descriptions";
  wrap.append(block("workflow-field-hint", "Describe the images you want for this scene. Suggested descriptions can be edited; adding a description does not generate an image."));
  const choices = [
    ["user_defined", "User Defined"], ["interior_reference", "Interior"], ["exterior_reference", "Exterior"],
    ["prop_reference", "Prop"], ["wardrobe_reference", "Wardrobe"], ["mood_reference", "Mood"],
  ];
  const entries: { description: HTMLTextAreaElement; category: HTMLSelectElement; save: HTMLButtonElement; remove?: HTMLButtonElement; controls?: HTMLFieldSetElement; hint?: HTMLElement; initialDescription: string; initialCategory: string }[] = [];
  const updateDirty = () => {
    const dirty = entries.find((item) => item.description.value !== item.initialDescription || item.category.value !== item.initialCategory);
    for (const item of entries) {
      item.description.disabled = !!dirty && item !== dirty;
      item.category.disabled = !!dirty && item !== dirty;
      item.save.disabled = !!dirty && item !== dirty;
      if (item.remove) item.remove.disabled = !!dirty;
      if (item.controls) item.controls.disabled = !!dirty;
      if (item.hint) item.hint.hidden = !dirty;
    }
  };
  const entry = (slot: NonNullable<NonNullable<SceneRecord["look_board"]>["slots"]>[number] | null) => {
    const section = document.createElement("section");
    const title = document.createElement("h3"); title.textContent = slot?.title || "New reference image";
    const category = document.createElement("select"); category.setAttribute("aria-label", slot ? `Category for ${slot.title}` : "New reference category");
    for (const [value, label] of choices) { const option = document.createElement("option"); option.value = value; option.textContent = label; category.append(option); }
    category.value = slot ? (slot.title === "User Defined" ? "user_defined" : slot.role) : "user_defined";
    const description = document.createElement("textarea"); description.value = slot?.prompt ?? ""; description.setAttribute("aria-label", slot ? `Description for ${slot.title}` : "New reference description");
    const save = actionButton(slot ? "Save description" : "Add description", () => {
      if (description.value.trim()) emit({ type: "save-description", slotId: slot?.id ?? null, description: description.value, category: category.value });
    });
    const item: typeof entries[number] = { description, category, save, initialDescription: description.value, initialCategory: category.value };
    entries.push(item);
    description.addEventListener("input", updateDirty);
    category.addEventListener("change", updateDirty);
    section.append(title, category, description, save);
    if (slot) {
      item.remove = actionButton("Delete look", () => emit({ type: "delete-look-slot", slotId: slot.id, title: slot.title || "Untitled" }));
      section.append(item.remove);
    }
    if (slot && imageWorkspace) {
      const hint = block("workflow-field-hint", "Save changed descriptions before using image actions.");
      hint.hidden = true;
      const controls = document.createElement("fieldset");
      controls.append(imageWorkspace(slot.id));
      item.controls = controls; item.hint = hint;
      section.append(hint, controls);
    }
    return section;
  };
  wrap.append(entry(null));
  for (const slot of scene.look_board?.slots ?? []) wrap.append(entry(slot));
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
  const previousScene = cut.id.cut === 1
    ? getSurfaceHost().state?.scenes?.find((candidate) => candidate.id === scene.id - 1)
    : undefined;
  const previousFinal = previousScene?.cuts.reduce<CutRecord | undefined>(
    (last, candidate) => !last || candidate.id.cut > last.id.cut ? candidate : last, undefined,
  );
  const predecessors = [...earlierCuts, ...(previousFinal ? [previousFinal] : [])];
  for (const candidate of predecessors) {
    const item = document.createElement("option");
    item.value = `${candidate.id.source_scene}:${candidate.id.cut}`;
    item.textContent = `Scene ${candidate.id.source_scene} Cut ${candidate.id.cut} · ${candidate.title?.trim() || "Untitled"}`;
    predecessorSelect.append(item);
  }
  const savedPredecessor = cut.visual_source?.predecessor;
  const defaultPredecessor = earlierCuts[earlierCuts.length - 1] ?? previousFinal;
  predecessorSelect.value = savedPredecessor
    ? `${savedPredecessor.source_scene}:${savedPredecessor.cut}`
    : defaultPredecessor ? `${defaultPredecessor.id.source_scene}:${defaultPredecessor.id.cut}` : "";
  const chosenPredecessor = () => predecessors.find(
    (candidate) => `${candidate.id.source_scene}:${candidate.id.cut}` === predecessorSelect.value,
  );
  const savedChoice = () => chosenPredecessor()?.id.source_scene === savedPredecessor?.source_scene
    && chosenPredecessor()?.id.cut === savedPredecessor?.cut;
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
      predecessorCut: strategy === "previous_selected_take_exit_frame" ? chosenPredecessor()?.id.cut ?? null : null,
      predecessorScene: strategy === "previous_selected_take_exit_frame" ? chosenPredecessor()?.id.source_scene ?? null : null,
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
  const refreshBindState = (): void => {
    const predecessor = chosenPredecessor();
    const take = predecessor?.takes?.find(
      (candidate) =>
        candidate.id === predecessor?.selected_take ||
        candidate.path === predecessor?.selected_take,
    );
    const editedTime = frameTimeEdited && frameTime.value !== "" ? Number(frameTime.value) : null;
    const predecessorScene = predecessor?.id.source_scene === scene.id ? scene : getSurfaceHost().state?.scenes?.find(item => item.id === predecessor?.id.source_scene);
    const shot = predecessorScene?.animatic?.shots?.find(item => item.cut_id.source_scene === predecessor?.id.source_scene && item.cut_id.cut === predecessor?.id.cut);
    const end = shot ? shot.duration_seconds - (shot.trim_out_seconds ?? 0) : NaN;
    const start = shot?.trim_in_seconds ?? 0;
    const editedEnding = take && shot?.trim_source_take_id === take.id && Number.isFinite(preview.duration)
      && Number.isFinite(end) && Number.isFinite(start) && start >= 0 && end > start && end <= preview.duration + 0.001
      ? Math.max(start, end - 0.1) : null;
    const choice = continuityChoice({
      strategy: strategySelect.value as VisualSourceStrategy,
      predecessorTakeSelected: Boolean(take) && savedChoice(),
      durationSeconds: Number.isFinite(preview.duration) ? preview.duration : null,
      editedTimeSeconds: editedTime ?? editedEnding,
      boundAssetId: cut.visual_source?.asset_id,
      dependentLabels: predecessor ? getSurfaceHost().continuityDependents(predecessor) : [],
      busy: getSurfaceHost().busy,
    });
    if (editedTime == null && choice.timeSeconds != null) {
      frameTime.value = choice.timeSeconds.toFixed(3);
    }
    bindContinuity.disabled = !choice.bindEnabled;
    bindContinuity.title =
      choice.bindReason ??
      "Extract the frame at the chosen time. The successor stays blocked until this bind is saved.";
  };
  frameTime.addEventListener("input", () => {
    frameTimeEdited = true;
    refreshBindState();
  });
  preview.addEventListener("loadedmetadata", () => {
    if (!Number.isFinite(preview.duration)) return;
    refreshBindState();
  });
  const bindContinuity = actionButton("Bind selected frame", () => {
    emit({
      type: "bind-frame",
      cut: cut.id.cut,
      predecessorCut: chosenPredecessor()?.id.cut ?? 0,
      predecessorScene: chosenPredecessor()?.id.source_scene ?? scene.id,
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
    const predecessor = chosenPredecessor();
    const take = predecessor?.takes?.find(
      (candidate) =>
        candidate.id === predecessor?.selected_take ||
        candidate.path === predecessor?.selected_take,
    );
    const choice = continuityChoice({
      strategy,
      predecessorTakeSelected: Boolean(take) && savedChoice(),
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
  const references = orderedReferencePicker((getSurfaceHost().state?.assets ?? [])
    .filter(asset => asset.status !== "rejected" && /\.(png|jpe?g|webp|gif)$/i.test(asset.path))
    .map(asset => ({ id: asset.id, label: asset.path })));
  directSource.append(promptLabel, negativeLabel, references.element);

  const promptActions = document.createElement("div");
  promptActions.className = "workflow-inspector-actions";
  const draftPrompt = actionButton("Draft image prompt", () => {
    if (!getSurfaceHost().busy && getSurfaceHost().chatReady) emit({ type: "draft-prompt", cut: cut.id.cut });
  });
  const draftReason = getSurfaceHost().busy ?? (getSurfaceHost().chatReady ? null : "Pick a chat model in the dropdown first.");
  if (draftReason) { draftPrompt.disabled = true; draftPrompt.title = draftReason; }
  promptActions.append(
    draftPrompt,
    actionButton("Save prompt", () => {
      emit({
        type: "save-prompt",
        cut: cut.id.cut,
        prompt: prompt.value,
        negativePrompt: negative.value,
      });
    }),
    actionButton("Generate start frame", () => {
      emit({ type: "generate", cut: cut.id.cut, referenceAssetIds: references.selected() });
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
  imageWorkspace?: (slotId: string) => HTMLElement,
): HTMLElement {
  const view = paintReferenceImagesStage(scene, sceneStatus, onIntent, imageWorkspace);
  const error = getSurfaceHost().error;
  if (error) view.prepend(block("workflow-banner reject", error));
  return view;
}
