import { existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_refused_write_no_refresh", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-03-"));
  const parent = join(root, "Production");
  mkdirSync(parent);
  const file = join(parent, "refused-prompt.txt");
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let refuseHost: (() => void) | undefined;
  const invoke = vi.fn(async (command: string, args: any) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "workflow_save_production_prompt") {
      expect(args.projectRoot).toBe(root);
      expect(args.prompt).toBe("refused prompt bytes");
      await new Promise<void>(resolve => { refuseHost = resolve; });
      throw new Error("Fixture host refused the save");
    }
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
  const tree = document.querySelector<HTMLElement>("#file-tree")!;
  await vi.waitFor(() => expect(tree.querySelector(`[data-path="${parent}"]`)).not.toBeNull());
  tree.querySelector<HTMLButtonElement>(`[data-path="${parent}"]`)!.click();
  await vi.waitFor(() => expect(tree.querySelector(`[data-path="${parent}"] + div > ul.tree-children`)).not.toBeNull());
  const before = tree.firstElementChild;
  const beforeHtml = tree.innerHTML;
  const listCount = invoke.mock.calls.filter(([cmd]) => cmd === "list_dir").length;
  const saving = getSurfaceHost().run("Saving production prompt…", "workflow_save_production_prompt", {
    sceneId: 1, cut: 1, prompt: "refused prompt bytes", negativePrompt: "",
  });
  await vi.waitFor(() => expect(refuseHost).toBeDefined());
  refuseHost!();
  expect(await saving).toBe(false);
  await new Promise(resolve => setTimeout(resolve, 150));
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_save_production_prompt")).toHaveLength(1);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "list_dir")).toHaveLength(listCount);
  expect(tree.firstElementChild).toBe(before);
  expect(tree.innerHTML).toBe(beforeHtml);
  expect(tree.querySelector(`[data-path="${file}"]`)).toBeNull();
  expect(existsSync(file)).toBe(false);
});
