import { renderGenerateStage } from "./surfaces/generateStage";
import { getSurfaceHost } from "./surfaces/session";
import { block, queueLookThumbs } from "./surfaces/shared";
import type { GenerateIntent } from "./surfaces/surfaceIntents";

/**
 * The new front door: a separate interface surface, not a workflow stage.
 * Own sub-navigation (Library / Generate / Projects / Scripts) rendered
 * inside the Home shell surface. The legacy production workflow lives
 * under the Studio surface and is untouched by this module.
 */
export function renderFrontDoor(onGenerate: (intent: GenerateIntent) => void): HTMLElement {
  const root = document.createElement("section");
  root.className = "frontdoor";
  root.dataset.frontdoor = "";

  const nav = document.createElement("div");
  nav.className = "frontdoor-nav";
  const buttons = new Map<string, HTMLButtonElement>();
  for (const id of ["library", "generate", "projects", "scripts"] as FrontDoorPanelId[]) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "frontdoor-nav-item";
    button.dataset.frontdoorNav = id;
    button.textContent = id === "library" ? "Library" : id === "generate" ? "Generate" : id === "projects" ? "Projects" : "Scripts";
    button.addEventListener("click", () => select(id));
    buttons.set(id, button);
    nav.append(button);
  }
  root.append(nav);

  const body = document.createElement("div");
  body.className = "frontdoor-body";
  body.dataset.frontdoorBody = "";
  root.append(body);

  function select(id: FrontDoorPanelId): void {
    for (const [key, button] of buttons) button.classList.toggle("active", key === id);
    body.replaceChildren(renderFrontDoorPanel(id, onGenerate));
  }
  select("library");
  return root;
}

export type FrontDoorPanelId = "library" | "generate" | "projects" | "scripts";

/** Content-only renderer for one front-door panel; the new interface shell
 * owns its own navigation and mounts these directly. */
export function renderFrontDoorPanel(id: FrontDoorPanelId, onGenerate: (intent: GenerateIntent) => void): HTMLElement {
  switch (id) {
    case "library":
      return libraryPanel();
    case "generate":
      return generatePanel(onGenerate);
    case "projects":
      return placeholderPanel("Projects", "Projects and their productions will live here. A project is where scripted scenes and cuts are produced with the library.");
    case "scripts":
      return placeholderPanel("Scripts", "Scripts are optional. Import or write one here, then produce cuts from it, or ignore scripts entirely.");
  }
}

function libraryPanel(): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "frontdoor-panel";
  panel.dataset.frontdoorPanel = "library";
  const host = getSurfaceHost();
  const assets = host?.state?.assets ?? [];
  const heading = document.createElement("h3");
  heading.textContent = "Library";
  panel.append(heading);
  if (!assets.length) {
    panel.append(
      block(
        "frontdoor-empty",
        "Nothing here yet. Images you generate appear in the library automatically, and anything you add belongs to the studio, not to one project.",
      ),
    );
    return panel;
  }
  const grid = document.createElement("div");
  grid.className = "frontdoor-cards";
  for (const asset of assets) {
    const card = document.createElement("div");
    card.className = "frontdoor-card";
    const image = document.createElement("img");
    image.alt = asset.path;
    image.dataset.lookPath = asset.path;
    const name = document.createElement("div");
    name.className = "frontdoor-card-name";
    name.textContent = asset.path.split("/").pop() ?? asset.path;
    const role = document.createElement("div");
    role.className = "frontdoor-card-role";
    role.textContent = String(asset.role ?? "");
    card.append(image, name, role);
    grid.append(card);
  }
  panel.append(grid);
  queueLookThumbs(panel);
  return panel;
}

function generatePanel(onGenerate: (intent: GenerateIntent) => void): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "frontdoor-panel";
  panel.dataset.frontdoorPanel = "generate";
  // Generate is never gated on a project: it writes to the studio library,
  // which exists on its own. The composer is always available.
  panel.append(renderGenerateStage(onGenerate));
  return panel;
}

function placeholderPanel(title: string, text: string): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "frontdoor-panel";
  panel.dataset.frontdoorPanel = title.toLowerCase();
  const heading = document.createElement("h3");
  heading.textContent = title;
  panel.append(heading, block("frontdoor-empty", text));
  return panel;
}
