import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { renderGenerateStage } from "../src/studio/surfaces/generateStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import { buildGenerateModel } from "../src/studio/surfaces/surfaceModels";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { GenerateIntent } from "../src/studio/surfaces/surfaceIntents";
import type { ProjectState, WorkflowImportResult } from "../src/workflowTypes";

const require = createRequire(import.meta.url);
const host = vi.hoisted(() => ({
  invoke: vi.fn(),
  openDialog: vi.fn(),
  ask: vi.fn(),
  toMediaUrlAsync: vi.fn(async () => "data:image/png;base64,"),
}));
vi.mock("../src/host", () => host);

const unused = vi.fn();

afterEach(() => {
  desk.clearWorkflowDesk();
  resetSurfaceSession();
  vi.clearAllMocks();
  document.body.replaceChildren();
});

function fixture(): WorkflowImportResult {
  return JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
}

function settledState(): ProjectState {
  const f = fixture();
  f.state.assets = [
    { id: "ref_a", path: "Preproduction/Reference/ref_a.png", role: "interior_reference", status: "approved" },
    { id: "g1", path: "Preproduction/GeneratedImages/alley_1_1.png", role: "generated_image", status: "draft", source_job_id: "job_1" },
    { id: "g2", path: "Preproduction/GeneratedImages/alley_1_2.png", role: "generated_image", status: "draft", source_job_id: "job_2" },
    { id: "g3", path: "Preproduction/GeneratedImages/city_2_1.png", role: "generated_image", status: "draft", source_job_id: "job_3" },
  ];
  f.state.jobs = [
    { id: "job_1", kind: "generated_image", raw_provider_id: "run_1", output_asset_ids: ["g1"] },
    { id: "job_2", kind: "generated_image", raw_provider_id: "run_1", output_asset_ids: ["g2"] },
    { id: "job_3", kind: "generated_image", raw_provider_id: "run_2", output_asset_ids: ["g3"] },
  ];
  return f.state;
}

function bindFakeHost(state: ProjectState): void {
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
    imageRoutes: () => [
      { id: "image.text_to_image", label: "Text to image · Offline · text-model" },
      { id: "image.image_to_image", label: "Image to image · Offline · chosen-model" },
    ],
    rerender: vi.fn(),
    setError: vi.fn(),
    rememberScene: vi.fn(),
    recipeRequires: () => false,
    resolvePath: (path) => path ?? null,
    ask: vi.fn() as unknown as SurfaceHost["ask"],
    pickFile: vi.fn() as unknown as SurfaceHost["pickFile"],
    mediaUrl: vi.fn(async () => "data:image/png;base64,") as unknown as SurfaceHost["mediaUrl"],
    continuityDependents: () => [],
    reviewExportPath: () => null,
    activeRecipe: () => null,
    requirementKindLabel: (kind) => kind,
    requirementBlocks: () => false,
    blockerNames: () => [],
    hooks: null,
  };
  bindSurfaceHost(fakeHost);
}

it("groups settled generated_image assets by run through the composer model", () => {
  const state = settledState();
  const model = buildGenerateModel({ state, imageRoutes: [], resolvePath: (path) => path ?? null });
  expect(model.results.map((group) => [group.runId, group.items.length])).toEqual([
    ["run_1", 2],
    ["run_2", 1],
  ]);
  expect(model.results[0].items.map((item) => item.assetId)).toEqual(["g1", "g2"]);
  expect(model.totalResults).toBe(3);
  // Every un-rejected image asset is referenceable, generated ones included.
  expect(model.referenceChoices.map((choice) => choice.assetId)).toEqual(["ref_a", "g1", "g2", "g3"]);
});

it("composer dispatches the exact generate intent payload", () => {
  const state = settledState();
  bindFakeHost(state);
  const emitted: GenerateIntent[] = [];
  const view = renderGenerateStage((intent) => emitted.push(intent));
  document.body.append(view);
  const prompt = view.querySelector<HTMLTextAreaElement>("textarea[data-generate-prompt]")!;
  prompt.value = "a neon alley at night";
  const count = view.querySelector<HTMLInputElement>("input[data-generate-count]")!;
  count.value = "3";
  const aspect = view.querySelector<HTMLSelectElement>("select[data-generate-aspect]")!;
  aspect.value = "16:9";
  const route = view.querySelector<HTMLSelectElement>("select[data-generate-model]")!;
  route.value = "image.image_to_image";
  const pickerSelect = view.querySelector<HTMLSelectElement>("select[aria-label=\"Reference image\"]")!;
  pickerSelect.value = "ref_a";
  view.querySelector<HTMLButtonElement>("button[data-add-reference]")!.click();
  view.querySelector<HTMLButtonElement>("button[data-generate-submit]")!.click();
  expect(emitted).toEqual([
    {
      type: "generate",
      prompt: "a neon alley at night",
      count: 3,
      aspectRatio: "16:9",
      imageCapability: "image.image_to_image",
      referenceAssetIds: ["ref_a"],
    },
  ]);
});

it("empty prompt with references means variations and Generate is never disabled", () => {
  const state = settledState();
  bindFakeHost(state);
  const emitted: GenerateIntent[] = [];
  const view = renderGenerateStage((intent) => emitted.push(intent));
  document.body.append(view);
  const submit = view.querySelector<HTMLButtonElement>("button[data-generate-submit]")!;
  expect(submit.disabled).toBe(false);
  const pickerSelect = view.querySelector<HTMLSelectElement>("select[aria-label=\"Reference image\"]")!;
  pickerSelect.value = "ref_a";
  view.querySelector<HTMLButtonElement>("button[data-add-reference]")!.click();
  expect(view.textContent).toContain("Generate makes variations");
  submit.click();
  expect(emitted).toEqual([
    { type: "generate", prompt: "", count: 1, aspectRatio: "9:16", imageCapability: "image.text_to_image", referenceAssetIds: ["ref_a"] },
  ]);
  // Empty prompt with no references: still clickable, never blocked client-side.
  const second: GenerateIntent[] = [];
  const view2 = renderGenerateStage((intent) => second.push(intent));
  document.body.append(view2);
  const submit2 = view2.querySelector<HTMLButtonElement>("button[data-generate-submit]")!;
  expect(submit2.disabled).toBe(false);
  expect(view2.textContent).not.toContain("makes variations");
  submit2.click();
  expect(second).toEqual([
    { type: "generate", prompt: "", count: 1, aspectRatio: "9:16", imageCapability: "image.text_to_image", referenceAssetIds: [] },
  ]);
});

it("results canvas renders settled runs with a working delete affordance", () => {
  const state = settledState();
  bindFakeHost(state);
  const emitted: GenerateIntent[] = [];
  const view = renderGenerateStage((intent) => emitted.push(intent));
  document.body.append(view);
  const canvas = view.querySelector<HTMLElement>("[data-generate-results]")!;
  const runs = canvas.querySelectorAll<HTMLElement>("[data-generate-run]");
  expect(runs.length).toBe(2);
  expect(runs[0].dataset.generateRun).toBe("run_1");
  expect(runs[1].dataset.generateRun).toBe("run_2");
  const tiles = canvas.querySelectorAll<HTMLElement>("[data-generate-result]");
  expect(tiles.length).toBe(3);
  expect(canvas.querySelector("img[data-look-path]")?.getAttribute("data-look-path")).toBe(
    "Preproduction/GeneratedImages/alley_1_1.png",
  );
  canvas.querySelector<HTMLButtonElement>("button[data-generate-delete=\"g1\"]")!.click();
  expect(emitted).toEqual([{ type: "delete", assetId: "g1" }]);
});

it("mode tabs keep only Image live and stub model sheet on the existing specialist", () => {
  bindFakeHost(settledState());
  const view = renderGenerateStage(unused);
  document.body.append(view);
  const modes = view.querySelectorAll<HTMLButtonElement>("[data-generate-mode]");
  expect(modes.length).toBe(4);
  const image = view.querySelector<HTMLButtonElement>("[data-generate-mode=\"image\"]")!;
  expect(image.disabled).toBe(false);
  for (const id of ["video", "audio", "model_sheet"]) {
    const tab = view.querySelector<HTMLButtonElement>(`[data-generate-mode="${id}"]`)!;
    expect(tab.disabled).toBe(true);
    expect(tab.title).toContain("Coming later");
  }
  expect(view.textContent).toContain("model_sheet specialist");
  expect(view.textContent).toContain("DEFAULT_MODEL_SHEET_SHOTS");
});

it("Generate dispatches workflow_generate_image and delete through the real desk", async () => {
  const f = fixture();
  f.state.assets = [
    { id: "ref_a", path: "Preproduction/Reference/ref_a.png", role: "interior_reference", status: "approved" },
  ];
  host.invoke.mockImplementation(async (command: string) => {
    if (command === "workflow_import" || command === "workflow_generate_image" || command === "workflow_delete_generated_image") {
      return structuredClone(f);
    }
    throw Error(`unexpected command ${command}`);
  });
  const root = document.createElement("main");
  document.body.append(root);
  desk.bindWorkflowDesk({
    root,
    projectRoot: () => "/fixture/project",
    onOpenPath: unused,
    onAddToChat: unused,
    onOpenVoice: unused,
    onComposeChat: unused,
    onSelectionChange: unused,
    chatModel: () => null,
    onStatus: unused,
    onPendingPlan: unused,
  });
  await desk.loadWorkflowDesk("/fixture/project");
  const view = renderGenerateStage((intent) => { void desk.handleGenerateIntent(intent); });
  document.body.append(view);
  const prompt = view.querySelector<HTMLTextAreaElement>("textarea[data-generate-prompt]")!;
  prompt.value = "desk path";
  const count = view.querySelector<HTMLInputElement>("input[data-generate-count]")!;
  count.value = "2";
  view.querySelector<HTMLButtonElement>("button[data-generate-submit]")!.click();
  await vi.waitFor(() =>
    // Generate is studio-backed: it never sends a projectRoot, so assets
    // land in the studio library even when a project happens to be open.
    expect(host.invoke).toHaveBeenCalledWith("workflow_generate_image", {
      prompt: "desk path",
      count: 2,
      aspectRatio: "9:16",
      imageCapability: undefined,
      referenceAssetIds: [],
    }),
  );
  expect(host.invoke.mock.calls.map((call) => call[0])).toEqual(["workflow_import", "workflow_generate_image"]);
});

it("registers both generate commands in the Electron allowlist and native dispatcher", () => {
  const { HOST_COMMANDS } = require("../electron/commandAllowlist.cjs");
  expect(HOST_COMMANDS.has("workflow_generate_image")).toBe(true);
  expect(HOST_COMMANDS.has("workflow_delete_generated_image")).toBe(true);
  const source = readFileSync("host/src/lib.rs", "utf8");
  expect(source).toContain("\"workflow_generate_image\" => workflow::cmd_generate_image");
  expect(source).toContain("\"workflow_delete_generated_image\" => workflow::cmd_delete_generated_image");
});
