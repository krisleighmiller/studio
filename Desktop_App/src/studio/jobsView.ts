import type { JobFilter, JobRowModel, JobsModel } from "./jobsModel";

export type JobsIntent =
  | { type: "toggle" }
  | { type: "filter"; filter: JobFilter | "all" }
  | { type: "output-context"; jobId: string; x: number; y: number }
  | { type: "open"; jobId: string };

const FILTERS: Array<{ id: JobFilter | "all"; label: string }> = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "failed", label: "Failed" },
  { id: "awaiting", label: "Awaiting" },
  { id: "completed", label: "Complete" },
];

export function renderJobsView(
  model: JobsModel,
  open: boolean,
  filter: JobFilter | "all",
  onIntent: (intent: JobsIntent) => void,
): HTMLElement {
  const section = document.createElement("section");
  section.className = `studio-jobs${open ? " jobs-open" : " jobs-closed"}`;
  section.setAttribute("aria-label", "Jobs");
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "studio-jobs-toggle";
  const total = model.total;
  const active = model.counts.active ?? 0;
  const failed = model.counts.failed ?? 0;
  toggle.textContent = open
    ? `Jobs (${total})`
    : total === 0
      ? `Jobs`
      : failed > 0
        ? `Jobs • ${active} active • ${failed} failed`
        : active > 0
          ? `Jobs • ${active} active`
          : `Jobs • ${total}`;
  toggle.setAttribute("aria-expanded", open ? "true" : "false");
  toggle.addEventListener("click", () => onIntent({ type: "toggle" }));
  section.append(toggle);
  if (!open) {
    if (failed > 0) {
      const warn = document.createElement("span");
      warn.className = "studio-jobs-closed-warn";
      warn.textContent = `${failed} failed — open for details`;
      section.append(warn);
    }
    return section;
  }

  const filters = document.createElement("div");
  filters.className = "studio-job-filters";
  for (const option of FILTERS) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = option.id === filter ? "active" : "";
    button.dataset.jobFilter = option.id;
    const count = option.id === "all" ? total : model.counts[option.id];
    button.textContent = `${option.label} ${count}`;
    button.addEventListener("click", () => onIntent({ type: "filter", filter: option.id }));
    filters.append(button);
  }
  section.append(filters);

  const body = document.createElement("div");
  body.className = "workflow-jobs-body";
  if (model.empty) {
    const empty = document.createElement("p");
    empty.className = "studio-jobs-empty";
    empty.textContent = model.empty;
    body.append(empty);
  }
  for (const row of model.rows) body.append(jobRow(row, onIntent));
  section.append(body);
  return section;
}

function jobRow(row: JobRowModel, onIntent: (intent: JobsIntent) => void): HTMLElement {
  const container = document.createElement("div");
  container.className = "studio-job-row";
  const button = document.createElement("button");
  button.type = "button";
  button.className = "studio-job";
  button.dataset.jobId = row.id;
  button.title = row.actionLabel ? `${row.actionLabel}. ${row.tooltip}` : row.tooltip;
  button.disabled = !row.actionLabel;
  const dot = document.createElement("span");
  dot.className = `studio-job-dot ${row.bucket}`;
  dot.setAttribute("aria-hidden", "true");
  const copy = document.createElement("span");
  copy.className = "studio-job-copy";
  const title = document.createElement("span");
  title.className = "studio-job-title";
  title.textContent = `${row.title}: ${row.statusLabel}`;
  const meta = document.createElement("span");
  meta.className = "studio-job-meta";
  if (row.failureError) {
    meta.append(document.createTextNode(row.elapsed === "unknown" ? "" : `${row.elapsed} · `));
    const error = document.createElement("span");
    error.className = "studio-job-error";
    error.textContent = row.failureError;
    error.addEventListener("contextmenu", () => {
      const selection = window.getSelection();
      if (!selection) return;
      const range = document.createRange();
      range.selectNodeContents(error);
      selection.removeAllRanges();
      selection.addRange(range);
    });
    meta.append(error);
    if (row.elapsed === "unknown") meta.append(document.createTextNode(" · time unknown"));
  } else {
    meta.textContent = row.elapsed === "unknown" ? `${row.detail} · time unknown` : `${row.elapsed} · ${row.detail}`;
  }
  const facts = document.createElement("span");
  facts.className = "studio-job-facts";
  facts.append(document.createTextNode([
    `Created ${row.created}`,
    `Started ${row.started}`,
    `Completed ${row.completed}`,
    `Scope ${row.scope}`,
  ].join(" · ") + " · Progress "));
  const progress = document.createElement("span");
  progress.className = "studio-job-progress";
  progress.textContent = row.progress;
  progress.addEventListener("contextmenu", () => {
    const selection = window.getSelection();
    if (!selection) return;
    const range = document.createRange();
    range.selectNodeContents(progress);
    selection.removeAllRanges();
    selection.addRange(range);
  });
  facts.append(progress, document.createTextNode(` · Retry ${row.retry} · Prediction ${row.predictionId ?? "unknown"}`));
  copy.append(meta, facts);
  const support = document.createElement("span");
  support.className = "studio-job-support";
  support.textContent = `Cancel: ${row.cancellation} · Resume: ${row.resume}`;
  copy.append(support);

  button.append(dot, title);
  if (row.actionLabel) {
    const action = document.createElement("span");
    action.className = "studio-job-action";
    action.textContent = row.actionLabel;
    button.append(action);
    button.addEventListener("click", () => onIntent({ type: "open", jobId: row.id }));
  }
  container.append(button, copy);
  if (row.bucket === "completed" || row.bucket === "awaiting") {
    container.addEventListener("contextmenu", (event) => {
      if (!row.outputPath) return;
      event.preventDefault();
      event.stopPropagation();
      onIntent({ type: "output-context", jobId: row.id, x: event.clientX, y: event.clientY });
    });
  }
  return container;
}
