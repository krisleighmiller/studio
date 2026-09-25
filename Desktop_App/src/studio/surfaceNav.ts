import type { SurfaceId, SurfaceNavItem } from "./surfaces";

export function renderSurfaceNav(
  items: SurfaceNavItem[],
  onSelect: (id: SurfaceId) => void,
): HTMLElement {
  const nav = document.createElement("nav");
  nav.className = "studio-surface-nav";
  nav.setAttribute("aria-label", "Workspace surfaces");
  if (!items.length) {
    const empty = document.createElement("p");
    empty.className = "studio-jobs-empty";
    empty.textContent = "This work area has no surfaces in the active recipe.";
    nav.append(empty);
    return nav;
  }
  for (const item of items) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = item.active ? "active" : "";
    button.dataset.status = item.status;
    if (item.active) button.setAttribute("aria-current", "page");
    const dot = document.createElement("span");
    dot.className = "studio-surface-dot";
    dot.setAttribute("aria-hidden", "true");
    const label = document.createElement("span");
    label.textContent = item.label;
    button.append(dot, label);
    if (item.statusLabel) {
      const status = document.createElement("span");
      status.className = "studio-surface-status";
      status.textContent = item.statusLabel;
      button.append(status);
    }
    button.addEventListener("click", () => onSelect(item.id));
    nav.append(button);
  }
  return nav;
}
