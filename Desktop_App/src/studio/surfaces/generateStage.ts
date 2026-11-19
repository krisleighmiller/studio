import { orderedReferencePicker } from "../orderedReferencePicker";
import { buildGenerateModel } from "./surfaceModels";
import { getSurfaceHost } from "./session";
import { block, queueLookThumbs } from "./shared";
import { currentStudioProgressNote } from "../../workflowDesk";
import { openImagePreviewForAsset } from "./imagePreview";
import type { GenerateIntent } from "./surfaceIntents";

const ASPECT_CHOICES = [
  { id: "9:16", label: "9:16 (portrait)" },
  { id: "16:9", label: "16:9 (landscape)" },
  { id: "1:1", label: "1:1 (square)" },
  { id: "4:5", label: "4:5" },
] as const;

const DEFAULT_ASPECT = "9:16";
const MIN_COUNT = 1;
const MAX_COUNT = 8;

const MODE_TABS = [
  { id: "image", label: "Image", live: true },
  { id: "video", label: "Video", live: false },
  { id: "audio", label: "Audio", live: false },
  { id: "model_sheet", label: "Model sheet", live: false },
] as const;

/**
 * Gate-free Generate surface. Image mode is the only live mode in this
 * slice; Video and Audio are visibly disabled with a "coming later" note,
 * and Model sheet mode is stubbed with a note that it reuses the existing
 * model_sheet specialist (DEFAULT_MODEL_SHEET_SHOTS in host/src/specialists.rs).
 * Generate is never gated: a prompt makes the described image, references
 * without a prompt make variations, and the desk's busy guard (runAction)
 * is the only thing that defers a click.
 */
export function renderGenerateStage(onIntent: (intent: GenerateIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const model = buildGenerateModel({
    state: host.state,
    imageRoutes: host.imageRoutes ? host.imageRoutes() : [],
    resolvePath: host.resolvePath,
  });
  const wrap = document.createElement("section");
  wrap.className = "workflow-generate";
  wrap.dataset.surface = "generate";

  const header = document.createElement("header");
  header.className = "workflow-generate-header";
  const title = document.createElement("h2");
  title.textContent = "Generate";
  const counts = document.createElement("p");
  counts.className = "workflow-generate-counts";
  counts.textContent = model.totalResults
    ? `${model.totalResults} generated image${model.totalResults === 1 ? "" : "s"}`
    : "No generated images yet";
  header.append(title, counts);
  wrap.append(header);

  wrap.append(modeTabs());

  const fields = document.createElement("div");
  fields.className = "workflow-generate-fields";

  const picker = orderedReferencePicker(
    model.referenceChoices.map((choice) => ({ id: choice.assetId, label: choice.label })),
  );
  const promptLabel = document.createElement("label");
  promptLabel.className = "workflow-field";
  const promptHeading = document.createElement("span");
  promptHeading.textContent = "Prompt";
  const prompt = document.createElement("textarea");
  prompt.rows = 4;
  prompt.dataset.generatePrompt = "";
  promptLabel.append(promptHeading, prompt);
  const countLabel = document.createElement("label");
  countLabel.className = "workflow-field";
  const countHeading = document.createElement("span");
  countHeading.textContent = "Images";
  const count = document.createElement("input");
  count.type = "number";
  count.min = String(MIN_COUNT);
  count.max = String(MAX_COUNT);
  count.step = "1";
  count.value = "1";
  count.dataset.generateCount = "";
  countLabel.append(countHeading, count);
  const aspectLabel = document.createElement("label");
  aspectLabel.className = "workflow-field";
  const aspectHeading = document.createElement("span");
  aspectHeading.textContent = "Aspect ratio";
  const aspect = document.createElement("select");
  aspect.dataset.generateAspect = "";
  for (const choice of ASPECT_CHOICES) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    aspect.append(option);
  }
  aspect.value = DEFAULT_ASPECT;
  aspectLabel.append(aspectHeading, aspect);
  const routeLabel = document.createElement("label");
  routeLabel.className = "workflow-field";
  const routeHeading = document.createElement("span");
  routeHeading.textContent = "Image generation model";
  const route = document.createElement("select");
  route.dataset.generateModel = "";
  for (const choice of model.imageRoutes) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    route.append(option);
  }
  if (model.imageRoutes.some((choice) => choice.id === "image.text_to_image")) {
    route.value = "image.text_to_image";
  }
  routeLabel.append(routeHeading, route);
  fields.append(
    picker.element,
    promptLabel,
    countLabel,
    aspectLabel,
    ...(model.imageRoutes.length ? [routeLabel] : []),
  );
  wrap.append(fields);

  const composerValues = () => ({
    prompt: prompt.value,
    count: Math.min(Math.max(Number.parseInt(count.value, 10) || 1, MIN_COUNT), MAX_COUNT),
    aspectRatio: aspect.value || DEFAULT_ASPECT,
    imageCapability: route.value || undefined,
    referenceAssetIds: picker.selected(),
  });
  const refreshHint = () => {
    hint.replaceChildren();
    const values = composerValues();
    if (!values.prompt.trim() && values.referenceAssetIds.length) {
      hint.append(
        block(
          "workflow-field-hint",
          "With references but no description, Generate makes variations of what was picked.",
        ),
      );
    }
  };
  const hint = document.createElement("div");
  hint.className = "workflow-generate-hint";
  refreshHint();
  prompt.addEventListener("input", refreshHint);
  const addReferenceButton = picker.element.querySelector<HTMLButtonElement>("button[data-add-reference]");
  addReferenceButton?.addEventListener("click", refreshHint);

  const generate = document.createElement("button");
  generate.type = "button";
  generate.className = "workflow-generate-submit";
  generate.dataset.generateSubmit = "";
  generate.textContent = "Generate";
  // Generate is never disabled: with a prompt it makes the described image,
  // with references only it makes variations. The desk's busy guard and
  // operation-approval flow own back-pressure, not this button.
  generate.addEventListener("click", () => {
    onIntent({ type: "generate", ...composerValues() });
  });
  const actions = document.createElement("div");
  actions.className = "workflow-generate-actions";
  actions.append(generate, hint);
  wrap.append(actions);

  wrap.append(resultsCanvas(model, onIntent));
  queueLookThumbs(wrap);
  return wrap;
}

function modeTabs(): HTMLElement {
  const row = document.createElement("div");
  row.className = "workflow-generate-modes";
  row.setAttribute("role", "tablist");
  row.setAttribute("aria-label", "Generate mode");
  for (const tab of MODE_TABS) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `workflow-generate-mode${tab.live ? " active" : ""}`;
    button.setAttribute("role", "tab");
    button.dataset.generateMode = tab.id;
    button.setAttribute("aria-selected", tab.live ? "true" : "false");
    button.textContent = tab.label;
    if (!tab.live) {
      button.disabled = true;
      button.title =
        tab.id === "model_sheet"
          ? "Coming later. Model sheet mode will reuse the existing model_sheet specialist (DEFAULT_MODEL_SHEET_SHOTS)."
          : "Coming later.";
    }
    row.append(button);
  }
  const note = document.createElement("p");
  note.className = "workflow-field-hint";
  note.textContent =
    "Video, Audio and Model sheet modes are coming later. Model sheet mode will reuse the existing model_sheet specialist (DEFAULT_MODEL_SHEET_SHOTS).";
  row.append(note);
  return row;
}

function resultsCanvas(
  model: ReturnType<typeof buildGenerateModel>,
  onIntent: (intent: GenerateIntent) => void,
): HTMLElement {
  const canvas = document.createElement("section");
  canvas.className = "workflow-generate-results";
  canvas.dataset.generateResults = "";
  const heading = document.createElement("h3");
  heading.textContent = "Results";
  canvas.append(heading);
  // In-flight runs: pending/running generated_image jobs from the last 10
  // minutes plus the latest host progress note, so a submitted generation
  // visibly shows work. Stale pending jobs (attempts that failed before
  // dispatch and were never settled) must not masquerade as live work.
  const tenMinutes = 10 * 60 * 1000;
  const pending = (getSurfaceHost().state?.jobs ?? []).filter(
    (job) =>
      job.kind === "generated_image" &&
      (job.status === "pending" || job.status === "running") &&
      typeof job.created_at === "string" &&
      Date.now() - Date.parse(job.created_at) < tenMinutes,
  );
  const note = currentStudioProgressNote();
  if (pending.length || note) {
    const strip = document.createElement("div");
    strip.className = "workflow-generate-progress";
    const line = document.createElement("p");
    line.dataset.generateProgress = "";
    line.textContent = note ?? `${pending.length} image${pending.length === 1 ? "" : "s"} generating…`;
    strip.append(line);
    for (const job of pending.slice(0, 4)) {
      const chip = document.createElement("span");
      chip.className = "workflow-generate-progress-chip";
      chip.textContent = job.status === "running" ? "running" : "queued";
      strip.append(chip);
    }
    canvas.append(strip);
  }
  if (!model.results.length) {
    canvas.append(
      block(
        "workflow-generate-empty",
        "Generated images settle here. Run one request and each still registers as its own asset you can delete.",
      ),
    );
    return canvas;
  }
  for (const group of model.results) {
    const section = document.createElement("div");
    section.className = "workflow-generate-run";
    section.dataset.generateRun = group.runId;
    const label = document.createElement("h4");
    label.textContent = group.label;
    section.append(label);
    const tiles = document.createElement("div");
    tiles.className = "workflow-generate-tiles";
    for (const item of group.items) {
      const tile = document.createElement("figure");
      tile.className = "workflow-generate-tile";
      tile.dataset.generateResult = item.assetId;
      const image = document.createElement("img");
      image.alt = item.label;
      image.dataset.lookPath = item.path ?? "";
      if (item.path) {
        image.title = "Click to view the full image";
        image.addEventListener("click", () => openImagePreviewForAsset(item.path ?? ""));
      }
      if (!item.path) image.alt = "Could not load generated image";
      const caption = document.createElement("figcaption");
      caption.textContent = item.label;
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "Delete";
      remove.dataset.generateDelete = item.assetId;
      remove.setAttribute("aria-label", `Delete generated image ${item.assetId}`);
      remove.addEventListener("click", () => onIntent({ type: "delete", assetId: item.assetId }));
      caption.append(remove);
      tile.append(image, caption);
      tiles.append(tile);
    }
    section.append(tiles);
    canvas.append(section);
  }
  return canvas;
}
