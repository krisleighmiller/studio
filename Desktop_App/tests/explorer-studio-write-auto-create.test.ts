import { mkdtempSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_studio_write_auto_create", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-02-"));
  const parent = join(root, "Production");
  mkdirSync(parent);
  const file = join(parent, "cut-prompt.txt");
  const bytes = "An intentionally distinct saved production prompt.";
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  let finishHost: (() => void) | undefined;
  const invoke = vi.fn(async (command: string, args: any) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "workflow_save_production_prompt") {
      expect(args.projectRoot).toBe(root);
      expect(args.prompt).toBe(bytes);
      await new Promise<void>(resolve => { finishHost = resolve; });
      writeFileSync(file, args.prompt);
      return structuredClone(fixture);
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
  await vi.waitFor(() => expect(invoke.mock.calls.some(([cmd, args]) => cmd === "list_dir" && args.path === parent)).toBe(true));
  expect(tree.querySelector(`[data-path="${file}"]`)).toBeNull();
  const saving = getSurfaceHost().run("Saving production prompt…", "workflow_save_production_prompt", {
    sceneId: 1, cut: 1, prompt: bytes, negativePrompt: "",
  });
  await vi.waitFor(() => expect(finishHost).toBeDefined());
  expect(tree.querySelector(`[data-path="${file}"]`)).toBeNull();
  finishHost!();
  expect(await saving).toBe(true);
  await vi.waitFor(() => expect(tree.querySelectorAll(`[data-path="${file}"]`)).toHaveLength(1));
  expect(tree.querySelector(`[data-path="${file}"]`)?.textContent).toContain("cut-prompt.txt");
  expect(readFileSync(file, "utf8")).toBe(bytes);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_save_production_prompt")).toHaveLength(1);
});
