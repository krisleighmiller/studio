import { mkdtempSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettingsPanel: vi.fn(), renderSettingsPanel: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); });

it("explorer_manual_rescan_external_create", async () => {
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = mkdtempSync(join(process.env.TMPDIR!, "w6-01-"));
  const parent = join(root, "Expanded");
  mkdirSync(parent);
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const openProject = vi.fn(async () => root);
  const invoke = vi.fn(async (command: string, args: {path: string}) => {
    if (command === "list_dir") return readdirSync(args.path, { withFileTypes: true }).map(entry => ({
      name: entry.name, path: join(args.path, entry.name), isDir: entry.isDirectory(),
    }));
    if (command === "workflow_import") return structuredClone(fixture);
    throw new Error(`Unexpected host call: ${command}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
  const tree = document.querySelector<HTMLElement>("#file-tree")!;
  await vi.waitFor(() => expect(tree.querySelector(`[data-path="${parent}"]`)).not.toBeNull());
  tree.querySelector<HTMLButtonElement>(`[data-path="${parent}"]`)!.click();
  await vi.waitFor(() => expect(invoke.mock.calls.some(([cmd, args]) => cmd === "list_dir" && args.path === parent)).toBe(true));
  const externalFile = join(parent, "created-from-another-process.txt");
  const writer = spawnSync(process.execPath, ["-e", "require('node:fs').writeFileSync(process.argv[1], 'external bytes')", externalFile]);
  expect(writer.status).toBe(0);
  expect(readFileSync(externalFile, "utf8")).toBe("external bytes");
  expect(tree.querySelector(`[data-path="${externalFile}"]`)).toBeNull();
  const parentReads = invoke.mock.calls.filter(([cmd, args]) => cmd === "list_dir" && args.path === parent).length;
  const refresh = new KeyboardEvent("keydown", { key: "r", ctrlKey: true, bubbles: true, cancelable: true });
  document.dispatchEvent(refresh);
  expect(refresh.defaultPrevented).toBe(true);
  await vi.waitFor(() => expect(tree.querySelectorAll(`[data-path="${externalFile}"]`)).toHaveLength(1));
  const diskEntry = readdirSync(parent, { withFileTypes: true }).find(entry => entry.name === "created-from-another-process.txt")!;
  const row = tree.querySelector<HTMLButtonElement>(`[data-path="${externalFile}"]`)!;
  expect(row.querySelector("span:last-child")?.textContent).toBe(diskEntry.name);
  expect(row.dataset.path).toBe(join(parent, diskEntry.name));
  expect(row.querySelector(".kind")?.textContent).toBe("·");
  expect(invoke.mock.calls.filter(([cmd, args]) => cmd === "list_dir" && args.path === parent)).toHaveLength(parentReads + 1);
  expect(openProject).toHaveBeenCalledTimes(1);
});
