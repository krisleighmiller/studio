import type { ProjectSummary } from "../selectors/project";
import type { SceneSummary } from "../selectors/scenes";

/** Renders the project summary. It does not call the host. */
export function renderProjectSummary(model: ProjectSummary, scenes: SceneSummary[]): HTMLElement {
  const meta = document.createElement("p");
  meta.className = "workflow-scenes-meta";
  const pending = model.pendingApprovals.slice(0, 3);
  const extra = model.pendingApprovals.length - pending.length;
  const pendingText = pending.length
    ? `Awaiting ${pending.join(", ")}${extra > 0 ? ` +${extra}` : ""}`
    : "No pending approvals";
  const blockerText = model.blockers.length ? `${model.blockers.length} blocker${model.blockers.length === 1 ? "" : "s"}` : "No blockers";
  meta.textContent = `${model.name} · ${model.recipeLabel} · ${model.workArea} · ${scenes.length} scenes · ${blockerText} · ${pendingText}`;
  meta.title = [model.profileLabel, ...model.blockers, ...model.pendingApprovals].filter(Boolean).join("\n");
  return meta;
}
