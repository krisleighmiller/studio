import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

const original = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
afterEach(() => { desk.clearWorkflowDesk(); document.body.replaceChildren(); vi.unstubAllGlobals(); });

it("generates saved-script details only after explicit confirmation, and stops on failure", async () => {
  const fixture = structuredClone(original);
  const scene = fixture.state.scenes[0];
  scene.intent = null; scene.intent_approved = false; scene.look_board = null;
  fixture.state.scenes = [scene];
  let fail = false;
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "workflow_import") return structuredClone(fixture);
    if (cmd === "workflow_draft_intent") {
      if (fail) throw new Error("fake provider failure");
      fixture.state.scenes[0].intent = { narrative_purpose: "Draft" };
      return structuredClone(fixture);
    }
    if (cmd === "workflow_draft_look") return structuredClone(fixture);
    throw Error(`Unexpected command ${cmd}`);
  });
  let resolveAsk!: (value: boolean) => void;
  const ask = vi.fn((_message: string, _options: unknown) => new Promise<boolean>(resolve => { resolveAsk = resolve; }));
  vi.stubGlobal("window", { workbench: { invoke, ask } });
  const root = document.createElement("main"); document.body.append(root);
  const noop = vi.fn();
  desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture", onOpenPath: noop, onAddToChat: noop, onOpenVoice: noop,
    onComposeChat: noop, onSelectionChange: noop, chatModel: () => ({ apiId: "fake", modelId: "fake" }), onStatus: noop });
  await desk.loadWorkflowDesk("/fixture"); invoke.mockClear();
  const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent?.trim() === text)!;
  const nav = [...root.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")].find(b => b.textContent?.includes("Script"))!;
  nav.click();
  const generate = button("Generate details"); expect(generate).toBeTruthy(); generate.click(); generate.click();
  expect(ask).toHaveBeenCalledTimes(1);
  expect(String(ask.mock.calls[0][0])).toContain("charges may apply");
  expect(invoke).not.toHaveBeenCalled();
  resolveAsk(false);
  await vi.waitFor(() => expect(button("Generate details").disabled).toBe(false));
  expect(invoke).not.toHaveBeenCalled();
  fail = true;
  await vi.waitFor(() => { button("Generate details").click(); expect(ask).toHaveBeenCalledTimes(2); });
  resolveAsk(true);
  await vi.waitFor(() => expect(root.textContent).toContain("fake provider failure"));
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("workflow_draft_intent", expect.objectContaining({ projectRoot: "/fixture", sceneId: scene.id, sceneStableId: scene.stable_id, scriptSource: scene.source_path }));
  fail = false; invoke.mockClear(); button("Generate details").click();
  expect(ask).toHaveBeenCalledTimes(3); resolveAsk(true);
  await vi.waitFor(() => expect(invoke.mock.calls.map(call => call[0])).toEqual(["workflow_draft_intent", "workflow_draft_look"]));
  expect(ask).toHaveBeenCalledTimes(3);
});
