import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { afterEach, expect, it, vi } from "vitest";
import { clearWorkflowDesk } from "../src/workflowDesk";
import { renderFrontDoor } from "../src/studio/frontDoor";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { ProjectState, WorkflowImportResult } from "../src/workflowTypes";

const require = createRequire(import.meta.url);
const host = vi.hoisted(() => ({
  invoke: vi.fn(),
  openDialog: vi.fn(),
  ask: vi.fn(),
  toMediaUrlAsync: vi.fn(async () => "data:image/png;base64,"),
}));
vi.mock("../src/host", () => host);

afterEach(() => {
  clearWorkflowDesk();
  resetSurfaceSession();
  vi.clearAllMocks();
  document.body.replaceChildren();
});

function fixture(): WorkflowImportResult {
  return JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
}

function bindFakeHost(state: ProjectState | null): void {
  const fakeHost: SurfaceHost = {
    busy: null,
    busyControl: null,
    error: null,
    chatReady: false,
    state,
    report: null,
    selectedCut: null,
    selectedSceneId: null,
    selectedRequirementId: null,
    run: vi.fn(async () => true),
    requireChat: () => null,
    imageRoutes: () => [{ id: "image.text_to_image", label: "Seedream" }],
    rerender: vi.fn(),
    setError: vi.fn(),
    rememberScene: vi.fn(),
    recipeRequires: () => false,
    resolvePath: (path) => (path ? path : null),
    ask: vi.fn() as never,
    pickFile: vi.fn() as never,
    mediaUrl: host.toMediaUrlAsync as never,
    continuityDependents: () => [],
    reviewExportPath: () => null,
    activeRecipe: () => null,
    requirementKindLabel: () => "",
    requirementBlocks: () => false,
    blockerNames: () => [],
    hooks: null,
  };
  bindSurfaceHost(fakeHost);
}

it("renders the front door with its own nav and opens on Library", () => {
  bindFakeHost(null);
  const door = renderFrontDoor(() => {});
  const navItems = [...door.querySelectorAll(".frontdoor-nav-item")].map((item) => item.textContent);
  expect(navItems).toEqual(["Library", "Generate", "Projects", "Scripts"]);
  expect(door.querySelector('[data-frontdoor-panel="library"]')).toBeTruthy();
  expect(door.querySelector('[data-frontdoor-panel="generate"]')).toBeNull();
});

it("renders the live generate composer with no project open", () => {
  bindFakeHost(null);
  const door = renderFrontDoor(() => {});
  const generateNav = [...door.querySelectorAll(".frontdoor-nav-item")].find(
    (item) => item.textContent === "Generate",
  );
  generateNav?.dispatchEvent(new MouseEvent("click"));
  const panel = door.querySelector('[data-frontdoor-panel="generate"]');
  // Generate is never gated on a project; the composer is always present.
  expect(panel?.querySelector("[data-generate-submit]")).toBeTruthy();
});

it("renders the live generate composer and forwards intents", () => {
  const state = fixture().state;
  bindFakeHost(state);
  const intents: string[] = [];
  const door = renderFrontDoor((intent) => intents.push(intent.type));
  const generateNav = [...door.querySelectorAll(".frontdoor-nav-item")].find(
    (item) => item.textContent === "Generate",
  );
  generateNav?.dispatchEvent(new MouseEvent("click"));
  const panel = door.querySelector('[data-frontdoor-panel="generate"]');
  expect(panel?.querySelector("[data-generate-submit]")).toBeTruthy();
  const button = panel?.querySelector<HTMLButtonElement>("[data-generate-submit]");
  button?.dispatchEvent(new MouseEvent("click"));
  expect(intents).toEqual(["generate"]);
});

it("lists library assets from state", () => {
  const state = fixture().state;
  state.assets = [
    { id: "a1", path: "Preproduction/Reference/room.png", role: "interior_reference", status: "approved" },
    { id: "a2", path: "Preproduction/GeneratedImages/x_1_1.png", role: "generated_image", status: "draft" },
  ];
  bindFakeHost(state);
  const door = renderFrontDoor(() => {});
  const cards = door.querySelectorAll(".frontdoor-card");
  expect(cards.length).toBe(2);
  expect(door.textContent).toContain("room.png");
  expect(door.textContent).toContain("x_1_1.png");
});
