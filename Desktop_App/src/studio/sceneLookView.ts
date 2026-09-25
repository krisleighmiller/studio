import { LOOK_ROLES } from "../workflowTypes";
import type { LookVersionModel, PlateModel, SceneListItemModel, SceneLookModel } from "./sceneLookModel";

export type SceneLookIntent =
  | { type: "select-slot"; slotId: string }
  | { type: "select-version"; assetId: string }
  | { type: "draft-board" }
  | { type: "save-prompt"; slotId: string; role: string; title: string; prompt: string; negative: string }
  | { type: "attach"; slotId: string }
  | { type: "generate"; slotId: string; role: string; title: string; prompt: string; negative: string }
  | { type: "approve"; slotId: string; assetId: string }
  | { type: "reject"; slotId: string; assetId: string }
  | { type: "delete"; slotId: string; assetId: string }
  | { type: "open"; path: string }
  | { type: "attach-director"; slotId: string }
  | { type: "build-plates" }
  | { type: "delete-plate"; assetId: string };

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
    meta.textContent = `${item.numberLabel} · ${item.readiness}`;
    copy.append(title, meta);
    button.append(copy);
    button.addEventListener("click", () => onSelect(item.key));
    aside.append(button);
  }
  return aside;
}

export function renderSceneLookView(
  model: SceneLookModel,
  onIntent: (intent: SceneLookIntent) => void,
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
  wrap.append(header);

  for (const blocker of model.blockers) {
    wrap.append(note("workflow-banner warn", `Blocked on ${blocker}`));
  }
  for (const job of model.failedJobs) {
    wrap.append(note("workflow-banner reject", job.message));
  }

  wrap.append(
    button("Draft look board", model.busyReason, () => onIntent({ type: "draft-board" })),
  );

  if (model.emptySlots || !model.selectedSlotId) {
    wrap.append(
      note(
        "workflow-missing",
        "No look slots yet. Draft from scene intent, or wait until intent is approved if you want the model to use it.",
      ),
    );
    return wrap;
  }

  const slot = model.slots.find((item) => item.id === model.selectedSlotId) ?? model.slots[0];
  const role = roleSelect(slot.role);
  const titleInput = textField("Title", slot.title);
  const promptInput = areaField("Prompt", slot.prompt, 3);
  const negativeInput = areaField("Negative", slot.negativePrompt, 2);
  const promptValues = () => ({
    slotId: slot.id,
    role: role.select.value,
    title: titleInput.input.value,
    prompt: promptInput.input.value,
    negative: negativeInput.input.value,
  });
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
  wrap.append(slotRow);

  const layout = document.createElement("div");
  layout.className = "scene-look";
  layout.append(
    primaryStage(model, slot.id, onIntent),
    alternateColumn(model, onIntent, () => onIntent({ type: "generate", ...promptValues() })),
  );
  wrap.append(layout);

  for (const warning of model.warnings) {
    wrap.append(note("workflow-banner warn", warning));
  }
  for (const boardNote of model.notes) {
    wrap.append(note("workflow-banner", boardNote));
  }
  const purpose = document.createElement("p");
  purpose.className = "scene-look-purpose";
  purpose.textContent = model.narrativePurpose;
  wrap.append(purpose);

  const fields = document.createElement("div");
  fields.className = "scene-look-fields";
  const approval = document.createElement("p");
  approval.className = "workflow-look-status";
  approval.textContent = slot.approvalLabel;
  fields.append(role.label, titleInput.label, promptInput.label, negativeInput.label, approval);
  wrap.append(fields);

  if (model.identityHint) wrap.append(note("workflow-look-identity", model.identityHint));

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    button(model.directorAttached ? "Attached to Director" : "Attach to Director", model.busyReason, () =>
      onIntent({ type: "attach-director", slotId: slot.id }),
    ),
    button("Save prompt", model.busyReason, () => onIntent({ type: "save-prompt", ...promptValues() })),
    button("Attach image", model.busyReason, () => onIntent({ type: "attach", slotId: slot.id })),
    button("Generate still", model.busyReason, () => onIntent({ type: "generate", ...promptValues() })),
  );
  if (model.canBuildPlates) {
    actions.append(
      button("Build wardrobe plates", model.busyReason, () => onIntent({ type: "build-plates" })),
    );
  }
  wrap.append(actions);

  if (model.showPlates) {
    const plates = document.createElement("section");
    plates.className = "scene-look-plates";
    plates.append(note("workflow-field-hint", model.plateNote));
    for (const plate of model.plates) {
      plates.append(plateCard(plate, model.busyReason, onIntent));
    }
    wrap.append(plates);
  }
  return wrap;
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
    button("Delete", model.busyReason, () => onIntent({ type: "delete", slotId, assetId: primary.assetId })),
  );
  stage.append(caption, row);
  return stage;
}

function alternateColumn(
  model: SceneLookModel,
  onIntent: (intent: SceneLookIntent) => void,
  onGenerate: () => void,
): HTMLElement {
  const column = document.createElement("div");
  column.className = "scene-look-alts";
  column.setAttribute("aria-label", "Alternate stills");
  if (!model.versions.length) {
    column.append(note("empty-state", "No alternates yet."));
  }
  for (const version of model.versions) {
    const buttonEl = document.createElement("button");
    buttonEl.type = "button";
    buttonEl.className = `scene-look-alt${version.assetId === model.selectedAssetId ? " active" : ""}${
      version.approved ? " approved" : ""
    }${version.rejected ? " rejected" : ""}`;
    buttonEl.append(stillImage(version));
    const label = document.createElement("span");
    label.textContent = version.statusLabel;
    buttonEl.append(label);
    buttonEl.addEventListener("click", () => onIntent({ type: "select-version", assetId: version.assetId }));
    column.append(buttonEl);
  }
  const generate = document.createElement("button");
  generate.type = "button";
  generate.className = "scene-look-generate";
  generate.textContent = "Add alternate";
  if (model.busyReason) {
    generate.disabled = true;
    generate.title = model.busyReason;
  }
  generate.addEventListener("click", onGenerate);
  column.append(generate);
  return column;
}

function plateCard(
  plate: PlateModel,
  busyReason: string | null,
  onIntent: (intent: SceneLookIntent) => void,
): HTMLElement {
  const fig = document.createElement("figure");
  fig.className = "scene-look-alt";
  fig.append(stillImage(
    { assetId: plate.assetId, path: plate.path, statusLabel: plate.label, approved: true, rejected: false },
    () => {
      if (plate.path) onIntent({ type: "open", path: plate.path });
    },
  ));
  const caption = document.createElement("figcaption");
  caption.textContent = plate.path ? plate.label : `${plate.label} · file missing`;
  fig.append(caption, button("Delete", busyReason, () => onIntent({ type: "delete-plate", assetId: plate.assetId })));
  return fig;
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
