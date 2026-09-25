import type { JobFilter, JobRowModel, JobsModel } from "./jobsModel";

export type JobsIntent =
  | { type: "toggle" }
  | { type: "filter"; filter: JobFilter | "all" }
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
  section.className = "studio-jobs";
  section.setAttribute("aria-label", "Jobs");
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "studio-jobs-toggle";
  const total = model.total;
  toggle.textContent = `Jobs (${total})`;
  toggle.setAttribute("aria-expanded", open ? "true" : "false");
  toggle.addEventListener("click", () => onIntent({ type: "toggle" }));
  section.append(toggle);
  if (!open) return section;

  const filters = document.createElement("div");
  filters.className = "studio-job-filters";
  for (const option of FILTERS) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = option.id === filter ? "active" : "";
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
  const button = document.createElement("button");
  button.type = "button";
  button.className = "studio-job";
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
  meta.textContent = row.elapsed === "unknown" ? `${row.detail} · time unknown` : `${row.elapsed} · ${row.detail}`;
  copy.append(title, meta);
  button.append(dot, copy);
  if (row.actionLabel) {
    button.addEventListener("click", () => onIntent({ type: "open", jobId: row.id }));
  }
  return button;
}
