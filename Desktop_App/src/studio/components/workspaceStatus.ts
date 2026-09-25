import type { WorkspaceModel } from "../selectors/workspace";

/** Renders the selected workspace. Actions stay on the surface that owns them. */
export function renderWorkspaceStatus(model: WorkspaceModel): HTMLElement {
  const line = document.createElement("p");
  line.className = "studio-workspace-status";
  const alternate = model.alternates.length ? `${model.alternates.length} alternate${model.alternates.length === 1 ? "" : "s"}` : "no alternates";
  const blocked = model.actions.find((action) => !action.enabled && action.reason);
  line.textContent = `${model.artifactType}${model.stableId ? ` ${model.stableId}` : ""} · ${model.primary} · ${model.approval} · ${model.dependency} · ${alternate}`;
  line.title = blocked?.reason ?? model.alternates.join("\n");
  return line;
}
