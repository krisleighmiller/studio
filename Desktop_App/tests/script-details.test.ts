import { describe, expect, it, vi } from "vitest";
import { scriptDetailTasks, runScriptDetailTasks } from "../src/studio/scriptDetails";
import type { SceneRecord } from "../src/workflowTypes";

function scene(id: number, source: string, extra: Partial<SceneRecord> = {}): SceneRecord {
  return { id, title: `Scene ${id}`, source_path: source, stable_id: `stable-${id}`, cuts: [], stages: {} as SceneRecord["stages"], ...extra };
}

describe("saved script detail drafting", () => {
  it("scopes to registered script scenes and preserves any existing details", () => {
    const tasks = scriptDetailTasks([
      scene(1, "a.md"), scene(2, "a.md", { intent: {}, look_board: {} as SceneRecord["look_board"] }),
      scene(3, "b.md"), scene(4, "a.md", { source_missing: true }), scene(5, "a.md", { stable_id: "" }),
      scene(6, "a.md", { intent_approved: true }),
    ], "a.md");
    expect(tasks.map(t => [t.sceneId, t.cmd])).toEqual([[1, "workflow_draft_intent"], [1, "workflow_draft_look"], [6, "workflow_draft_look"]]);
    expect(tasks[0]).toMatchObject({ sceneStableId: "stable-1", scriptSource: "a.md" });
  });
  it("stops on cancellation, failure, or project switch without dispatching more", async () => {
    const tasks = scriptDetailTasks([scene(1, "a.md"), scene(2, "a.md")], "a.md");
    const failed = vi.fn().mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    expect(await runScriptDetailTasks(tasks, () => true, failed)).toBe("stopped");
    expect(failed).toHaveBeenCalledTimes(2);
    let current = true;
    const switched = vi.fn(async () => { current = false; return true; });
    expect(await runScriptDetailTasks(tasks, () => current, switched)).toBe("stopped");
    expect(switched).toHaveBeenCalledTimes(1);
  });
  it("runs in order only after each previous request succeeds", async () => {
    const tasks = scriptDetailTasks([scene(1, "a.md")], "a.md");
    const run = vi.fn().mockResolvedValue(true);
    expect(await runScriptDetailTasks(tasks, () => true, run)).toBe("complete");
    expect(run.mock.calls.map(c => [c[0].cmd, c[1], c[2]])).toEqual([["workflow_draft_intent", 0, 2], ["workflow_draft_look", 1, 2]]);
  });
});
