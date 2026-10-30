import { afterEach, expect, it, vi } from "vitest";
import type { WorkflowImportResult } from "../src/workflowTypes";
const fixture = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("../src/host", () => ({ invoke: fixture.invoke, ask: vi.fn(), openDialog: vi.fn(), toMediaUrlAsync: vi.fn() }));
import { bindWorkflowDesk, clearWorkflowDesk, loadWorkflowDesk, refreshWorkflowDesk, workflowStatusLine } from "../src/workflowDesk";

function result(name: string): WorkflowImportResult {
  return {
    state: { version: 1, project_name: name, characters: [], scenes: [], assets: [], requirements: [], notes: [], budget: null, jobs: [] },
    report: { summary: name, scenes: [] }, summary: name,
  } as unknown as WorkflowImportResult;
}

afterEach(() => { clearWorkflowDesk(); document.body.innerHTML = ""; fixture.invoke.mockReset(); });

it.each(["different root", "same root", "cancelled dialog"])("discards an old Director refresh after %s epoch invalidation", async (transition) => {
  let root = "/fixture/project-A";
  let epoch = 1;
  const onStatus = vi.fn();
  const onSelectionChange = vi.fn();
  const container = document.createElement("div");
  document.body.append(container);
  bindWorkflowDesk({ root: container, projectRoot: () => root, onOpenPath: vi.fn(), onAddToChat: vi.fn(),
    onOpenVoice: vi.fn(), onComposeChat: vi.fn(), onSelectionChange, chatModel: () => null, onStatus });
  fixture.invoke.mockResolvedValueOnce(result("Initial A"));
  await loadWorkflowDesk(root);
  let finish!: (value: WorkflowImportResult) => void;
  fixture.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const captured = epoch;
  const pending = refreshWorkflowDesk(() => epoch === captured);
  epoch += 1;
  if (transition === "different root") root = "/fixture/project-B";
  fixture.invoke.mockResolvedValueOnce(result("Current project generation"));
  await loadWorkflowDesk(root);
  onStatus.mockClear(); onSelectionChange.mockClear();
  const currentHTML = container.innerHTML;
  finish(result("STALE Director A output UNIQUE"));
  await pending;
  expect(workflowStatusLine()).toBe("Current project generation");
  expect(container.innerHTML).toBe(currentHTML);
  expect(onStatus).not.toHaveBeenCalled();
  expect(onSelectionChange).not.toHaveBeenCalled();
});

it("refreshes real desk state without overwriting a terminal Director status", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const onStatus = vi.fn();
  bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/A", onOpenPath: vi.fn(), onAddToChat: vi.fn(),
    onOpenVoice: vi.fn(), onComposeChat: vi.fn(), onSelectionChange: vi.fn(), chatModel: () => null, onStatus });
  fixture.invoke.mockResolvedValueOnce(result("Initial"));
  await loadWorkflowDesk("/fixture/A");
  let finish!: (value: WorkflowImportResult) => void;
  fixture.invoke.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  const pending = refreshWorkflowDesk(() => true, false);
  onStatus.mockClear();
  finish(result("Actual completed observation"));
  await pending;
  expect(workflowStatusLine()).toBe("Actual completed observation");
  expect(onStatus).not.toHaveBeenCalled();
});

it("still applies a Director refresh whose owning operation remains current", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  bindWorkflowDesk({ root: container, projectRoot: () => "/fixture/A", onOpenPath: vi.fn(), onAddToChat: vi.fn(),
    onOpenVoice: vi.fn(), onComposeChat: vi.fn(), onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  fixture.invoke.mockResolvedValueOnce(result("Initial"));
  await loadWorkflowDesk("/fixture/A");
  fixture.invoke.mockResolvedValueOnce(result("Valid current refresh"));
  await refreshWorkflowDesk(() => true);
  expect(workflowStatusLine()).toBe("Valid current refresh");
});
