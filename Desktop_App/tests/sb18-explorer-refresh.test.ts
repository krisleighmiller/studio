import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.useRealTimers(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("refreshes real local files after successful Studio writes only, and rejects stale command/tree completion", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const base = mkdtempSync(join(process.env.TMPDIR || tmpdir(), "sb18-"));
  const a = join(base, "A"), b = join(base, "B"); mkdirSync(a); mkdirSync(b);
  writeFileSync(join(a, "existing.txt"), "existing"); writeFileSync(join(b, "B-only.txt"), "B");
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let chosen = a, fail = false;
  let finishCommand: (() => void) | undefined, finishTree: (() => void) | undefined;
  let holdCommand = false, holdTree = false;
  const invoke = vi.fn(async (cmd: string, args: any) => {
    if (cmd === "list_dir") {
      const entries = readdirSync(args.path, { withFileTypes: true }).map(e => ({ name: e.name, path: join(args.path, e.name), isDir: e.isDirectory() }));
      if (holdTree && args.path === a) { holdTree = false; await new Promise<void>(resolve => { finishTree = resolve; }); }
      return entries;
    }
    if (cmd === "workflow_import" || cmd === "workflow_set_max_cuts") return structuredClone(fixture);
    if (cmd === "workflow_save_production_prompt") {
      if (fail) throw Error("Fixture write refused");
      if (holdCommand) { holdCommand = false; await new Promise<void>(resolve => { finishCommand = resolve; }); }
      writeFileSync(join(args.projectRoot, args.prompt), `saved ${args.prompt}`);
      return structuredClone(fixture);
    }
    throw Error(`Unexpected ${cmd}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => chosen, ask: async () => true, rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  const flush = () => vi.advanceTimersByTimeAsync(150);
  const open = async (path: string) => { chosen = path; document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush(); };
  const tree = () => document.querySelector<HTMLElement>("#file-tree")!;
  const lists = () => invoke.mock.calls.filter(([cmd]) => cmd === "list_dir").length;
  const save = (prompt: string) => getSurfaceHost().run("Saving production prompt…", "workflow_save_production_prompt", { sceneId: 1, cut: 1, prompt });
  await flush(); await open(a);
  expect(tree().textContent).toContain("existing.txt");
  const selected = tree().querySelector<HTMLButtonElement>('[data-path$="/existing.txt"]')!;
  selected.focus(); tree().scrollTop = 27;
  const count = lists();
  await save("created.txt"); await flush();
  expect(readFileSync(join(a, "created.txt"), "utf8")).toBe("saved created.txt");
  expect(tree().textContent).toContain("created.txt"); expect(lists()).toBe(count + 1);
  expect(tree().scrollTop).toBe(27);
  const afterCreate = tree().firstElementChild, afterCount = lists();
  await getSurfaceHost().run("Saving cut limit", "workflow_set_max_cuts", { sceneId: 1, maxCuts: 2 }); await flush();
  expect(lists()).toBe(afterCount); expect(tree().firstElementChild).toBe(afterCreate);
  fail = true; expect(await save("refused.txt")).toBe(false); await flush(); fail = false;
  expect(lists()).toBe(afterCount); expect(tree().firstElementChild).toBe(afterCreate);
  holdCommand = true; const late = save("late.txt"); await flush(); expect(finishCommand).toBeDefined();
  await open(b); const bTree = tree().innerHTML, bLists = lists();
  finishCommand!(); await late; await flush();
  expect(readFileSync(join(a, "late.txt"), "utf8")).toBe("saved late.txt");
  expect(lists()).toBe(bLists); expect(tree().innerHTML).toBe(bTree); expect(tree().textContent).not.toContain("late.txt");
  // A valid completion can start a read which itself finishes after replacement.
  await open(a); holdTree = true; await save("slow-tree.txt"); await flush(); expect(finishTree).toBeDefined();
  await open(b); const replacementTree = tree().innerHTML;
  finishTree!(); await flush(); expect(tree().innerHTML).toBe(replacementTree); expect(tree().textContent).toContain("B-only.txt");
  // Same root is still a new session: root equality alone is insufficient.
  await open(a); holdCommand = true; const sameRoot = save("same-root-late.txt"); await flush();
  await open(a); const reopenedTree = tree().innerHTML, reopenedCount = lists(); finishCommand!(); await sameRoot; await flush();
  expect(lists()).toBe(reopenedCount); expect(tree().innerHTML).toBe(reopenedTree);
  if (process.env.SB18_RECEIPT) writeFileSync(process.env.SB18_RECEIPT, JSON.stringify({ fixtureRoot: base, createdBytes: readFileSync(join(a, "created.txt"), "utf8"), stateOnlyTreeUnchanged: true, refusedWriteTreeUnchanged: true, lateOtherProjectUnchanged: true, lateTreeReadUnchanged: true, sameRootReopenUnchanged: true, providerCalls: 0, commands: invoke.mock.calls.map(([cmd, args]) => ({ cmd, args })) }, null, 2));
});
