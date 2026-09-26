import type { AssetRecord, CutRecord } from "../../workflowTypes";
import { applySurfaceActions } from "../surfaceActions";
import { getSurfaceHost } from "./session";
import type { SurfacePresentation } from "./surfaceModels";
import type { NotesIntent } from "./surfaceIntents";
import { renderBlocker } from "./surfaceSelectors";

export function block(className: string, text: string): HTMLElement {
  const el = document.createElement("p");
  el.className = className;
  el.textContent = text;
  return el;
}

export function missingPanel(title: string, detail: string): HTMLElement {
  const article = document.createElement("article");
  article.className = "workflow-missing";
  const h = document.createElement("h3");
  h.textContent = title;
  const p = document.createElement("p");
  p.textContent = detail;
  article.append(h, p);
  return article;
}

export function newAlternateAngleButton(onAlternate: () => void): HTMLButtonElement {
  const button = actionButton("New alternate angle", onAlternate);
  button.title =
    "Adds a new shot to this scene, with its own id, sketch, and animatic timing. It does not mark an existing video as an alternate take.";
  return button;
}

export function actionButton(label: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "ghost-button";
  button.textContent = label;
  button.disabled = Boolean(getSurfaceHost().busy);
  button.addEventListener("click", () => {
    if (button.disabled) return;
    onClick();
  });
  return button;
}

export function findAsset(id: string): AssetRecord | undefined {
  return getSurfaceHost().state?.assets?.find((asset) => asset.id === id);
}

export function queueLookThumbs(root: HTMLElement): void {
  root.querySelectorAll<HTMLImageElement>("img[data-look-path]").forEach((img) => {
    const path = img.dataset.lookPath;
    if (!path) return;
    void getSurfaceHost().mediaUrl(path).then((url) => {
      img.src = url;
    }).catch(() => {
      img.alt = "Could not load still";
    });
  });
}

export function queueTakeVideos(root: HTMLElement): void {
  root.querySelectorAll<HTMLVideoElement>("video[data-take-path]").forEach((video) => {
    const path = video.dataset.takePath;
    if (!path) return;
    void getSurfaceHost().mediaUrl(path).then((url) => {
      video.src = url;
      video.load();
    }).catch(() => {
      video.replaceWith(block("workflow-field-hint", "Could not load this take."));
    });
  });
}

export function presentSurface(view: HTMLElement, model: SurfacePresentation, syncBanner = false): HTMLElement {
  view.dataset.status = model.status;
  view.dataset.tone = model.tone;
  if (model.loading) view.setAttribute("aria-busy", "true");
  else view.removeAttribute("aria-busy");
  if (model.blocked.length) view.dataset.blocked = model.blocked.join(", ");
  else delete view.dataset.blocked;
  if (syncBanner && model.banner) {
    const existing = view.querySelector<HTMLElement>(":scope > .workflow-banner, :scope > .workflow-missing");
    if (existing) existing.textContent = model.banner;
  }
  applySurfaceActions(view, model.actions);
  return view;
}

/** Shared labeled-field primitives (sketch + character editors). */
export function sketchTextField(
  parent: HTMLElement,
  title: string,
  value: string,
): HTMLInputElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  label.append(heading, input);
  parent.append(label);
  return input;
}

export function sketchSelectField(
  parent: HTMLElement,
  title: string,
  value: string,
  options: { id: string; label: string }[],
): HTMLSelectElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const select = document.createElement("select");
  const blank = document.createElement("option");
  blank.value = "";
  blank.textContent = "—";
  select.append(blank);
  for (const option of options) {
    const item = document.createElement("option");
    item.value = option.id;
    item.textContent = option.label;
    select.append(item);
  }
  select.value = value;
  label.append(heading, select);
  parent.append(label);
  return select;
}

export function sketchTextarea(
  parent: HTMLElement,
  title: string,
  value: string,
  rows: number,
): HTMLTextAreaElement {
  const label = document.createElement("label");
  label.className = "workflow-field";
  const heading = document.createElement("span");
  heading.textContent = title;
  const textarea = document.createElement("textarea");
  textarea.rows = rows;
  textarea.value = value;
  label.append(heading, textarea);
  parent.append(label);
  return textarea;
}

/** Shared per-cut render action (render queue + production notes). */
export function renderCutButton(
  cut: CutRecord,
  emit: (intent: NotesIntent) => void,
): HTMLButtonElement {
  const button = actionButton(`Render Cut ${cut.id.cut} new take`, () => {
    emit({ type: "render", cut: cut.id.cut });
  });
  const blocker = renderBlocker(cut);
  button.disabled = Boolean(getSurfaceHost().busy) || Boolean(blocker);
  button.title = blocker
    ? `Render blocked: ${blocker}.`
    : "Prepare a provider request for approval.";
  return button;
}
