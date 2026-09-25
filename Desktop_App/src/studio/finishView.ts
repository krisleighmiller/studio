import type { FinishModel } from "./finishModel";

export type FinishIntent =
  | { type: "open-review" }
  | { type: "assemble" }
  | { type: "check"; deliverableId: string };

export function renderFinishView(
  model: FinishModel,
  onIntent: (intent: FinishIntent) => void,
): HTMLElement {
  const aside = document.createElement("section");
  aside.className = "studio-finish";
  aside.setAttribute("aria-label", "Finish");
  const heading = document.createElement("h2");
  heading.textContent = "Finish";
  const scope = document.createElement("p");
  scope.className = "studio-finish-scope";
  scope.textContent = `${model.sceneLabel} · ${model.headline}`;
  aside.append(heading, scope);

  if (!model.items.length) {
    const empty = document.createElement("p");
    empty.className = "studio-jobs-empty";
    empty.textContent = "Select a scene to see takes, the review export, and post.";
    aside.append(empty);
    return aside;
  }

  const list = document.createElement("div");
  list.className = "studio-finish-list";
  for (const item of model.items) {
    const card = document.createElement("article");
    card.className = `studio-finish-card ${item.state}`;
    const row = document.createElement("div");
    row.className = "studio-finish-row";
    const dot = document.createElement("span");
    dot.className = `studio-job-dot ${item.state}`;
    dot.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.className = "studio-finish-label";
    label.textContent = item.label;
    const state = document.createElement("span");
    state.className = "studio-finish-state";
    state.textContent = item.stateLabel;
    row.append(dot, label, state);
    const detail = document.createElement("p");
    detail.textContent = item.detail;
    card.append(row, detail);
    list.append(card);
  }
  aside.append(list);

  const actions = document.createElement("div");
  actions.className = "studio-finish-actions";
  const open = button("Open review export", !model.reviewExportPath || Boolean(model.busyReason), () =>
    onIntent({ type: "open-review" }),
  );
  open.title = model.reviewExportPath ?? "No review export yet.";
  const assemble = button("Prepare assembly", !model.canAssemble, () => onIntent({ type: "assemble" }));
  assemble.title = model.assembleReason ?? "Prepare a local assembly from the selected takes.";
  actions.append(open, assemble);

  if (model.deliverables.length) {
    const select = document.createElement("select");
    select.setAttribute("aria-label", "Deliverable");
    select.disabled = Boolean(model.busyReason);
    for (const deliverable of model.deliverables) {
      const option = document.createElement("option");
      option.value = deliverable.id;
      option.textContent = deliverable.name;
      option.selected = deliverable.id === model.selectedDeliverableId;
      select.append(option);
    }
    const check = button("Check deliverable", Boolean(model.busyReason), () =>
      onIntent({ type: "check", deliverableId: select.value }),
    );
    actions.append(select, check);
  }
  aside.append(actions);
  return aside;
}

function button(label: string, disabled: boolean, onClick: () => void): HTMLButtonElement {
  const element = document.createElement("button");
  element.type = "button";
  element.className = "ghost-button";
  element.textContent = label;
  element.disabled = disabled;
  element.addEventListener("click", onClick);
  return element;
}
