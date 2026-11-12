import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_state_only_no_refresh", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-04-"));
  const parent = join(root, "Production");
  mkdirSync(parent);
  writeFileSync(join(parent, "existing.txt"), "untouched");
  const native = JSON.parse(readFileSync("tests/fixtures/sb12-native-performance.json", "utf8"));
  const assetId = native.selected.state.scenes[0].cuts[0].selected_performance_asset_id;
  let finishSelection: (() => void) | undefined;
  const invoke = vi.fn(async (command: string, args: any) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(native.version);
    if (command === "workflow_select_performance") {
      expect(args).toMatchObject({ projectRoot: root, sceneId: 1, cut: 1, assetId });
      await new Promise<void>(resolve => { finishSelection = resolve; });
      return structuredClone(native.selected);
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
  await vi.waitFor(() => expect(tree.querySelector(`[data-path="${join(parent, "existing.txt")}"]`)).not.toBeNull());
  const before = tree.firstElementChild;
  const listCount = invoke.mock.calls.filter(([cmd]) => cmd === "list_dir").length;
  const selecting = getSurfaceHost().run("Saving performance selection…", "workflow_select_performance", {
    sceneId: 1, cut: 1, assetId, reviewedAssetId: null,
  });
  await vi.waitFor(() => expect(finishSelection).toBeDefined());
  finishSelection!();
  expect(await selecting).toBe(true);
  await new Promise(resolve => setTimeout(resolve, 150));
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_select_performance")).toHaveLength(1);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "list_dir")).toHaveLength(listCount);
  expect(tree.firstElementChild).toBe(before);
});
