import { afterEach, describe, expect, it, vi } from "vitest";
import type { SceneRecord, WorkflowImportResult } from "../src/workflowTypes";
const fixture = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../src/host", () => ({
  invoke: fixture.invoke, ask: vi.fn(), openDialog: vi.fn(), toMediaUrlAsync: vi.fn(),
}));
import { bindWorkflowDesk, clearWorkflowDesk, loadWorkflowDesk, refreshWorkflowDesk, renderWorkflowDesk } from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";

function result(note?: string, approved = false): WorkflowImportResult {
  const scene: SceneRecord = {
    id: 1, title: "Scene 1", stable_id: "scene-1",
    stages: {
      script: "present", characters: "present", scene_intent: approved ? "approved" : "draft",
      scene_look: "missing", cut_sketches: "missing", reference_images: "missing",
      production_notes: "missing", render: "missing", take_review: "missing", assemble: "missing",
    },
    cuts: [], intent: { narrative_purpose: "The room light changes" }, intent_approved: approved,
  };
  return {
    state: { version: 2, project_name: "Fixture", scenes: [scene], assets: [], characters: [], jobs: [], requirements: [], notes: [], scripts: [], settings: {} },
    report: { summary: "Fixture", scenes: [], recipes: [], warnings: [], recipe: { id: "narrative_dialogue", label: "Narrative dialogue", stages: [{ id: "scene_intent", label: "Scene Intent", work_area: "plan", depends_on: [], required_roles: [] }] } },
    note,
  } as unknown as WorkflowImportResult;
}
async function mount() {
  const container = document.createElement("div");
  document.body.append(container);
  bindWorkflowDesk({
    root: container, projectRoot: () => "/fixture/project",
    onOpenPath: () => {}, onAddToChat: () => {}, onOpenVoice: () => {}, onComposeChat: () => {},
    onSelectionChange: () => {}, chatModel: () => ({ apiId: "a", modelId: "m" }), onStatus: vi.fn(),
  });
  fixture.invoke.mockResolvedValueOnce(result());
  await loadWorkflowDesk("/fixture/project");
  expect(container.querySelector(".workflow-intent")).not.toBeNull();
  return container;
}
function button(root: HTMLElement, label: string) {
  const found = [...root.querySelectorAll("button")].find(item => item.textContent === label);
  expect(found, label).toBeTruthy();
  return found!;
}
afterEach(() => { clearWorkflowDesk(); document.body.innerHTML = ""; fixture.invoke.mockReset(); });

describe("S01 scene intent feedback provenance", () => {
  it.each([
    ["Draft from script", "workflow_draft_intent", "Scene intent drafted. Review and approve before relying on it for sketches.", false],
    ["Save draft", "workflow_save_intent", "Scene intent saved as draft.", false],
    ["Approve intent", "workflow_approve_intent", "Scene intent approved.", true],
  ] as const)("%s success remains status feedback, not an error", async (label, cmd, note, approved) => {
    const root = await mount();
    fixture.invoke.mockResolvedValueOnce(result(note, approved));
    button(root, label).click();
    await vi.waitFor(() => expect(root.textContent).toContain(note));
    expect(fixture.invoke).toHaveBeenLastCalledWith(cmd, expect.objectContaining({ projectRoot: "/fixture/project", sceneId: 1 }));
    expect(getSurfaceHost().error).toBeNull();
    expect(root.querySelector(".workflow-intent")?.getAttribute("data-tone")).not.toBe("error");
    expect(root.querySelector(".workflow-intent .workflow-banner.reject")).toBeNull();
    expect(root.querySelector(".workflow-intent")?.getAttribute("data-status")).toBe(approved ? "approved" : "draft");
  });

  it("real controller failure is visible and a subsequent successful save clears it", async () => {
    const root = await mount();
    fixture.invoke.mockRejectedValueOnce(new Error("scene intent JSON had no usable fields"));
    button(root, "Save draft").click();
    await vi.waitFor(() => expect(root.querySelector(".workflow-intent .workflow-banner.reject")?.textContent).toBe("scene intent JSON had no usable fields"));
    expect(getSurfaceHost().error).toBe("scene intent JSON had no usable fields");
    expect(root.querySelector(".workflow-intent")?.getAttribute("data-tone")).toBe("error");
    fixture.invoke.mockResolvedValueOnce(result("Scene intent saved as draft."));
    button(root, "Save draft").click();
    await vi.waitFor(() => expect(root.textContent).toContain("Scene intent saved as draft."));
    expect(getSurfaceHost().error).toBeNull();
    expect(root.querySelector(".workflow-intent .workflow-banner.reject")).toBeNull();
    expect(root.querySelector(".workflow-intent")?.getAttribute("data-tone")).not.toBe("error");
  });

  it("native refresh notes remain status feedback while preserving existing status text on silent refresh", async () => {
    const root = await mount();
    fixture.invoke.mockResolvedValueOnce(result("Scene intent saved as draft."));
    await refreshWorkflowDesk(() => true);
    expect(root.textContent).toContain("Scene intent saved as draft.");
    expect(getSurfaceHost().error).toBeNull();
    fixture.invoke.mockResolvedValueOnce(result());
    await refreshWorkflowDesk(() => true);
    expect(root.textContent).toContain("Scene intent saved as draft.");
    expect(root.querySelector(".workflow-intent .workflow-banner.reject")).toBeNull();
  });

  it("project clear removes real error provenance without asynchronous fixture work", async () => {
    const root = await mount();
    getSurfaceHost().setError("Stale error");
    renderWorkflowDesk();
    expect(getSurfaceHost().error).toBe("Stale error");
    expect(root.querySelector(".workflow-intent .workflow-banner.reject")?.textContent).toBe("Stale error");
    clearWorkflowDesk();
    expect(getSurfaceHost().error).toBeNull();
    expect(root.innerHTML).toBe("");
    await mount();
    expect(getSurfaceHost().error).toBeNull();
  });
});
