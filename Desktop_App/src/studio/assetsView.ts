import type { AssetFilterId, AssetsModel } from "./selectors/assets";
import { ASSET_FILTERS } from "./selectors/assets";

export function renderAssetsView(
  model: AssetsModel,
  onFilter: (id: AssetFilterId) => void,
): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "studio-assets";

  const bar = document.createElement("div");
  bar.className = "studio-assets-filters";
  bar.setAttribute("role", "group");
  bar.setAttribute("aria-label", "Filter assets by role");
  for (const facet of ASSET_FILTERS) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = `studio-asset-chip${facet.id === model.filter ? " active" : ""}`;
    chip.setAttribute("aria-pressed", String(facet.id === model.filter));
    chip.textContent = `${facet.label} ${model.counts[facet.id]}`;
    chip.addEventListener("click", () => onFilter(facet.id));
    bar.append(chip);
  }
  wrap.append(bar);

  if (!model.rows.length) {
    const empty = document.createElement("p");
    empty.className = "empty-state";
    empty.textContent = model.filter === "all" ? "No assets in this project." : "No assets match this filter.";
    wrap.append(empty);
    return wrap;
  }

  const list = document.createElement("ul");
  list.className = "studio-assets-list";
  for (const row of model.rows) {
    const item = document.createElement("li");
    item.className = "studio-asset-row";
    const name = document.createElement("span");
    name.className = "studio-asset-name";
    name.textContent = row.name;
    const role = document.createElement("span");
    role.className = `studio-asset-role studio-asset-role-${row.role}`;
    role.textContent = row.roleLabel;
    const meta = document.createElement("span");
    meta.className = "studio-asset-meta";
    meta.textContent = [
      row.sceneId !== null ? `Scene ${row.sceneId}` : null,
      row.status !== "unknown" ? row.status : null,
    ]
      .filter(Boolean)
      .join(" · ");
    item.append(name, role, meta);
    list.append(item);
  }
  wrap.append(list);
  return wrap;
}