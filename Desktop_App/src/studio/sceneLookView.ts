import { LOOK_ROLES } from "../workflowTypes";
import type { LookVersionModel, SceneListItemModel, SceneLookModel } from "./sceneLookModel";

export type SceneLookIntent =
  | { type: "select-slot"; slotId: string }
  | { type: "select-version"; assetId: string }
  | { type: "draft-board" }
  | { type: "save-prompt"; slotId: string; role: string; title: string; prompt: string; negative: string }
  | { type: "attach"; slotId?: string; role?: string }
  | { type: "crop"; slotId: string; assetId: string; path: string }
  | { type: "generate"; slotId: string; role: string; title: string; prompt: string; negative: string; attachCharacterImage: boolean; referenceAssetIds: string[]; imageCapability?: string }
  | { type: "approve"; slotId: string; assetId: string }
  | { type: "reject"; slotId: string; assetId: string }
  | { type: "delete"; slotId: string; assetId: string }
  | { type: "open"; path: string }
  | { type: "attach-director"; slotId: string };

export function renderSceneListView(
  items: SceneListItemModel[],
  onSelect: (key: string) => void,
): HTMLElement {
  const aside = document.createElement("aside");
  aside.className = "workflow-scenes";
  aside.setAttribute("aria-label", "Scenes");
  const heading = document.createElement("h2");
  heading.textContent = "Scenes";
  aside.append(heading);
  if (!items.length) {
    aside.append(note("empty-state", "No scenes in this project."));
    return aside;
  }
  for (const item of items) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `workflow-scene scene-list-item${item.active ? " active" : ""}${item.warning ? " warning" : ""}`;
    button.dataset.sceneKey = item.key;
    button.append(thumbnail(item.thumbnailPath, item.title));
    const copy = document.createElement("span");
    copy.className = "scene-list-copy";
    const title = document.createElement("span");
    title.className = "workflow-scene-title";
    title.textContent = item.title;
    const meta = document.createElement("span");
    meta.className = "workflow-scene-mix";
    // Readiness only — the title already carries the scene identity, and
    // repeating "Scene N" here doubled every list item's heading.
    meta.textContent = item.readiness;
    copy.append(title, meta);
    if (item.audio) {
      const audio = document.createElement("span");
      audio.className = "workflow-scene-audio";
      audio.textContent = item.audio;
      copy.append(audio);
    }
    button.append(copy);
    button.addEventListener("click", () => onSelect(item.key));
    aside.append(button);
  }
  return aside;
}

export function renderSceneLookView(
  model: SceneLookModel,
  onIntent: (intent: SceneLookIntent) => void,
  compact = false,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "scene-look-workspace";
  const header = document.createElement("header");
  header.className = "scene-look-header";
  const title = document.createElement("h1");
  title.textContent = model.title;
  const counts = document.createElement("p");
  counts.className = "scene-look-counts";
  counts.textContent = model.counts;
  header.append(title, counts);
  const activeSlot =
    model.emptySlots || !model.selectedSlotId
      ? null
      : model.slots.find((item) => item.id === model.selectedSlotId) ?? null;
  if (activeSlot) header.append(approvalPill(activeSlot));
  wrap.append(header);
  if (model.affectedCuts.length) {
    const affected = document.createElement("section");
    affected.className = "scene-look-affected-cuts";
    affected.append(note("workflow-field-hint", "Cuts bound to this look slot. Approving a replacement does not swap their image or authorize video. Review each stale cut in Production, rebind its visual source, and refresh its production note before preparing video."));
    for (const cut of model.affectedCuts) {
      affected.append(note(cut.stale ? "workflow-banner warn" : "workflow-field-hint",
        `${cut.label} · source ${cut.assetId} · ${cut.stale ? "Needs review/rebind" : "Bound"} · note ${cut.notePath ?? "none"}${cut.pending ? " · pending video approval must be prepared again" : ""}`));
    }
    wrap.append(affected);
  }

  if (model.blockers.length || model.failedJobs.length) {
    wrap.append(compactBlockerSummary(model, onIntent));
  }

  if (model.emptySlots || !model.selectedSlotId) {
    const role = roleSelect("interior_reference");
    role.select.setAttribute("aria-label", "New look slot role");
    role.select.disabled = !!model.busyReason;
    wrap.append(
      note("workflow-missing", "No look slots yet. Choose a role and attach an existing image to create the first slot locally. This is a scene look reference, not a cut start frame; no generation or approval is performed."),
      role.label,
      button("Attach existing image", model.busyReason, () => onIntent({ type: "attach", role: role.select.value })),
      button("Draft look board", model.busyReason, () => onIntent({ type: "draft-board" })),
      note("workflow-field-hint", "Or draft a look board from scene intent using your configured model."),
    );
    return wrap;
  }

  const slot = model.slots.find((item) => item.id === model.selectedSlotId) ?? model.slots[0];
  const role = roleSelect(slot.role);
  const titleInput = textField("Title", slot.title);
  const promptInput = areaField("Prompt", slot.prompt, 3);
  const negativeInput = areaField("Negative", slot.negativePrompt, 2);
  const characterImage = document.createElement("input");
  characterImage.type = "checkbox";
  const promptValues = () => ({
    slotId: slot.id,
    role: role.select.value,
    title: titleInput.input.value,
    prompt: promptInput.input.value,
    negative: negativeInput.input.value,
  });

  const selectedReferences: string[] = [];
  const referenceLabel = document.createElement("label");
  referenceLabel.textContent = "Optional reference images (added in order)";
  const reference = document.createElement("select");
  reference.setAttribute("aria-label", "Reference image");
  const noReference = document.createElement("option"); noReference.value = ""; noReference.textContent = "Choose an image"; reference.append(noReference);
  for (const choice of model.referenceChoices ?? []) {
    const option = document.createElement("option"); option.value = choice.assetId; option.textContent = choice.label; reference.append(option);
  }
  const selectedList = document.createElement("ol");
  const redrawReferences = () => {
    selectedList.replaceChildren();
    selectedReferences.forEach((id, index) => {
      const item = document.createElement("li");
      item.textContent = model.referenceChoices?.find(choice => choice.assetId === id)?.label ?? id;
      const remove = document.createElement("button"); remove.type = "button"; remove.textContent = "Remove";
      remove.setAttribute("aria-label", `Remove reference ${index + 1}`);
      remove.addEventListener("click", () => { selectedReferences.splice(index, 1); redrawReferences(); });
      item.append(remove); selectedList.append(item);
    });
  };
  const add = document.createElement("button"); add.type = "button"; add.textContent = "Add reference";
  add.setAttribute("aria-label", "Add reference image");
  add.addEventListener("click", () => { if (reference.value && !selectedReferences.includes(reference.value)) { selectedReferences.push(reference.value); redrawReferences(); } });
  referenceLabel.append(reference); wrap.append(referenceLabel, add, selectedList);
  const route = document.createElement("select");
  route.setAttribute("aria-label", "Image generation model");
  for (const choice of model.imageRoutes ?? []) {
    const option = document.createElement("option"); option.value = choice.id; option.textContent = choice.label; route.append(option);
  }
  if (model.imageRoutes?.some(choice => choice.id === "image.text_to_image")) route.value = "image.text_to_image";
  if (model.imageRoutes?.length) {
    const label = document.createElement("label"); label.textContent = "Image generation model"; label.append(route); wrap.append(label);
  }
  const layout = document.createElement("div");
  layout.className = "scene-look scene-look-hero";
  layout.append(
    primaryStage(model, slot.id, onIntent),
    alternateColumn(model, onIntent, characterImage, () => onIntent({ type: "generate", ...promptValues(), attachCharacterImage: characterImage.checked, referenceAssetIds: [...selectedReferences], imageCapability: route.value || undefined }),
      () => onIntent({ type: "attach", slotId: slot.id })),
  );
  wrap.insertBefore(layout, referenceLabel);

  // The Reference Images page supplies its own description editor and one
  // workspace per slot; do not duplicate slot navigation or prompt editors.
  if (compact) return wrap;

  const slotRow = document.createElement("div");
  slotRow.className = "scene-look-slots";
  slotRow.setAttribute("role", "tablist");
  slotRow.setAttribute("aria-label", "Look slots");
  for (const item of model.slots) {
    const tab = document.createElement("button");
    tab.type = "button";
    tab.className = `scene-look-slot${item.id === slot.id ? " active" : ""}`;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", item.id === slot.id ? "true" : "false");
    tab.textContent = item.title || roleLabel(item.role);
    tab.addEventListener("click", () => onIntent({ type: "select-slot", slotId: item.id }));
    slotRow.append(tab);
  }
  // Redo-plan composition: the selected artifact's title and narrative
  // purpose read directly under the image, the slot tabs stay a persistent
  // row, and only the editable prompt fields collapse.
  const context = document.createElement("div");
  context.className = "scene-look-context";
  const heroTitle = document.createElement("h2");
  heroTitle.className = "scene-look-hero-title";
  heroTitle.textContent = slot.title || roleLabel(slot.role);
  context.append(heroTitle);
  if (model.narrativePurpose) context.append(note("scene-look-hero-purpose", model.narrativePurpose));
  wrap.append(context);

  const slotRowWrap = document.createElement("div");
  slotRowWrap.className = "scene-look-slotrow";
  const slotRowLabel = document.createElement("span");
  slotRowLabel.className = "scene-look-slotrow-label";
  slotRowLabel.textContent = `Look slots (${model.slots.length})`;
  slotRowWrap.append(slotRowLabel, slotRow);
  wrap.append(slotRowWrap);

  const metaFields = document.createElement("div");
  metaFields.className = "scene-look-fields";
  metaFields.append(role.label, titleInput.label, promptInput.label, negativeInput.label);
  wrap.append(disclosure("Prompt & metadata", metaFields, false));

  if (model.warnings.length || model.notes.length) {
    const list = document.createElement("div");
    for (const warning of model.warnings) list.append(note("workflow-banner warn", warning));
    for (const boardNote of model.notes) list.append(note("workflow-banner", boardNote));
    wrap.append(
      disclosure(`Warnings (${model.warnings.length})`, list, model.warnings.length > 0),
    );
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions scene-look-advanced";
  actions.append(
    button(model.directorAttached ? "Attached to Director" : "Attach to Director", model.busyReason, () =>
      onIntent({ type: "attach-director", slotId: slot.id }),
    ),
    button("Save prompt", model.busyReason, () => onIntent({ type: "save-prompt", ...promptValues() })),
    button("Draft look board", model.busyReason, () => onIntent({ type: "draft-board" })),
  );
  wrap.append(disclosure("Advanced actions", actions, false));
  return wrap;
}

/** Header approval pill: the artifact's approval state stays visible, never buried. */
function approvalPill(slot: SceneLookModel["slots"][number]): HTMLElement {
  const label = slot.approvalLabel || "Not approved";
  const approved = /approved/i.test(label) && !/needs|not approved/i.test(label);
  const pill = document.createElement("span");
  pill.className = `scene-look-pill${approved ? " is-approved" : /reject/i.test(label) ? " is-rejected" : " is-pending"}`;
  pill.textContent = approved ? `✓ ${label}` : label;
  pill.setAttribute("aria-label", `Approval state: ${label}`);
  return pill;
}

function compactBlockerSummary(
  model: SceneLookModel,
  _onIntent: (intent: SceneLookIntent) => void,
): HTMLElement {
  const details = document.createElement("details");
  details.className = "scene-look-blockers";
  const summary = document.createElement("summary");
  const count = model.blockers.length;
  summary.textContent =
    count > 0
      ? `⚠ ${count} requirement${count === 1 ? "" : "s"} blocking production · Review requirements`
      : `⚠ ${model.failedJobs.length} failed job${model.failedJobs.length === 1 ? "" : "s"}`;
  details.append(summary);
  for (const blocker of model.blockers) {
    details.append(note("workflow-field-hint", `Blocked on ${blocker}`));
  }
  for (const job of model.failedJobs) {
    details.append(note("workflow-banner reject", job.message));
  }
  return details;
}

function disclosure(title: string, content: HTMLElement, open: boolean): HTMLElement {
  const details = document.createElement("details");
  details.className = "scene-look-disclosure";
  if (open) details.open = true;
  const summary = document.createElement("summary");
  summary.textContent = title;
  details.append(summary, content);
  return details;
}

function primaryStage(
  model: SceneLookModel,
  slotId: string,
  onIntent: (intent: SceneLookIntent) => void,
): HTMLElement {
  const stage = document.createElement("div");
  stage.className = "scene-look-stage";
  const primary = model.primary;
  if (!primary) {
    stage.append(note("empty-state", "No still yet. Attach an image or generate one."));
    return stage;
  }
  stage.append(stillImage(primary, () => {
    if (primary.path) onIntent({ type: "open", path: primary.path });
  }));
  const caption = document.createElement("p");
  caption.className = "scene-look-caption";
  caption.textContent = primary.path ? primary.statusLabel : `${primary.statusLabel} · file missing`;
  const row = document.createElement("div");
  row.className = "workflow-inspector-actions";
  row.append(
    button("Approve", model.busyReason, () => onIntent({ type: "approve", slotId, assetId: primary.assetId })),
    button("Reject", model.busyReason, () => onIntent({ type: "reject", slotId, assetId: primary.assetId })),
    button("Open", primary.path ? model.busyReason : "This still has no file path.", () => {
      if (primary.path) onIntent({ type: "open", path: primary.path });
    }),
    button("Crop locally", primary.path ? model.busyReason : "This still has no file path.", () => {
      if (primary.path) onIntent({ type: "crop", slotId, assetId: primary.assetId, path: primary.path });
    }),
    button("Delete", model.busyReason, () => onIntent({ type: "delete", slotId, assetId: primary.assetId })),
  );
  stage.append(caption, row);
  return stage;
}

function alternateColumn(
  model: SceneLookModel,
  onIntent: (intent: SceneLookIntent) => void,
  characterImage: HTMLInputElement,
  onGenerate: () => void,
  onAttach: () => void,
): HTMLElement {
  const column = document.createElement("div");
  column.className = "scene-look-alts";
  column.setAttribute("aria-label", "Alternate stills");
  if (!model.versions.length) {
    column.append(note("empty-state", "No alternates yet."));
  }
  for (const version of model.versions) {
    // Per the mock: each alternate is a captioned preview with its own
    // action bar beneath it (approve / open live on the tile itself).
    const tile = document.createElement("article");
    tile.className = `scene-look-alt-tile${version.assetId === model.selectedAssetId ? " selected" : ""}${version.approved ? " approved" : ""}${version.rejected ? " rejected" : ""}`;
    const pick = document.createElement("button");
    pick.type = "button";
    pick.className = `scene-look-alt${version.assetId === model.selectedAssetId ? " active" : ""}${version.approved ? " approved" : ""}${version.rejected ? " rejected" : ""}`;
    pick.append(stillImage(version));
    const label = document.createElement("span");
    label.textContent = version.statusLabel;
    pick.append(label);
    pick.addEventListener("click", () => onIntent({ type: "select-version", assetId: version.assetId }));
    const actions = document.createElement("div");
    actions.className = "scene-look-alt-actions";
    const approve = button(version.approved ? "Approved" : "Approve", version.approved ? null : model.busyReason, () => {
      const slotId = model.selectedSlotId;
      if (!slotId) return;
      onIntent({ type: "approve", slotId, assetId: version.assetId });
    });
    if (version.approved) approve.disabled = true;
    const open = button("Open", version.path ? model.busyReason : "This still has no file path.", () => {
      if (version.path) onIntent({ type: "open", path: version.path });
    });
    actions.append(approve, open, button("Crop locally", version.path ? model.busyReason : "This still has no file path.", () => {
      if (version.path && model.selectedSlotId) onIntent({ type: "crop", slotId: model.selectedSlotId, assetId: version.assetId, path: version.path });
    }));
    if (!version.approved && !version.rejected) {
      actions.append(
        button("Reject", model.busyReason, () => {
          const slotId = model.selectedSlotId;
          if (!slotId) return;
          onIntent({ type: "reject", slotId, assetId: version.assetId });
        }),
      );
    }
    tile.append(pick, actions);
    column.append(tile);
  }
  const generate = document.createElement("button");
  generate.type = "button";
  generate.className = "scene-look-generate";
  generate.textContent = model.versions.length ? "Add alternate" : "Generate still";
  if (model.busyReason) {
    generate.disabled = true;
    generate.title = model.busyReason;
  }
  generate.addEventListener("click", onGenerate);
  const characterLabel = document.createElement("label");
  characterLabel.append(characterImage, document.createTextNode(" Attach character images and approved wardrobe references"));
  column.append(
    button("Attach existing image", model.busyReason, onAttach),
    characterLabel,
    generate,
    note("workflow-field-hint", "Attaches a project-owned copy to this selected scene look slot, not a cut start frame. Generation uses your configured image provider and requires approval before submission; provider charges may apply. Attaching an existing image does not generate media."),
  );
  return column;
}


function stillImage(version: LookVersionModel, onOpen?: () => void): HTMLElement {
  if (!version.path) {
    return fallback("File missing");
  }
  const img = document.createElement("img");
  img.alt = version.statusLabel;
  img.dataset.lookPath = version.path;
  img.addEventListener("error", () => {
    img.replaceWith(fallback("File missing"));
  });
  if (onOpen) {
    img.addEventListener("click", (event) => {
      event.stopPropagation();
      onOpen();
    });
  }
  return img;
}

function thumbnail(path: string | null, title: string): HTMLElement {
  if (!path) return fallback("No still");
  const img = document.createElement("img");
  img.className = "scene-list-thumb";
  img.alt = "";
  img.dataset.lookPath = path;
  img.addEventListener("error", () => {
    img.replaceWith(fallback("No still"));
  });
  img.title = title;
  return img;
}

function roleSelect(role: string): { label: HTMLLabelElement; select: HTMLSelectElement } {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const span = document.createElement("span");
  span.textContent = "Role";
  const select = document.createElement("select");
  for (const option of LOOK_ROLES) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    select.append(item);
  }
  select.value = role;
  label.append(span, select);
  return { label, select };
}

function textField(name: string, value: string): { label: HTMLLabelElement; input: HTMLInputElement } {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const span = document.createElement("span");
  span.textContent = name;
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  label.append(span, input);
  return { label, input };
}

function areaField(name: string, value: string, rows: number): { label: HTMLLabelElement; input: HTMLTextAreaElement } {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const span = document.createElement("span");
  span.textContent = name;
  const input = document.createElement("textarea");
  input.rows = rows;
  input.value = value;
  label.append(span, input);
  return { label, input };
}

function button(label: string, disabledReason: string | null, onClick: () => void): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "ghost-button";
  el.textContent = label;
  if (disabledReason) {
    el.disabled = true;
    el.title = disabledReason;
  }
  el.addEventListener("click", onClick);
  return el;
}

function fallback(text: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.className = "scene-thumb-fallback";
  el.textContent = text;
  return el;
}

function note(className: string, text: string): HTMLParagraphElement {
  const el = document.createElement("p");
  el.className = className;
  el.textContent = text;
  return el;
}

function roleLabel(role: string): string {
  return LOOK_ROLES.find((item) => item.id === role)?.label ?? role;
}
