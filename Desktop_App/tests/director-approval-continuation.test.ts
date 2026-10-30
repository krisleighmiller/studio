import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Exercise the real app entry point, chat form, approval button and history.
// IPC and unrelated desk/settings panels are fixtures: this is renderer-boundary
// coverage, NOT proof of provider execution or an Electron/host end-to-end run.
const fixture = vi.hoisted(() => ({
  invoke: vi.fn(),
  openProjectFolder: vi.fn(async (): Promise<string | null> => "/fixture/black-hole-repro"),
  useRecentProject: vi.fn(async (_root: string) => {}),
  ask: vi.fn(async () => true),
  events: null as null | { onMenu: (id: string) => void; onDirectorProgress: (payload: unknown) => void; onSpendConfirmationRequired: (payload: unknown) => void },
  settings: { selectedChatModel: { apiId: "fixture-api", modelId: "fixture-planner" } },
  selection: {
    stage: "production_notes", sceneId: 1, sceneStableId: "scene-mom-sam",
    shotStableId: "shot-mom", assetId: null, lookSlotId: null,
    requirementId: null, workArea: "produce",
  },
}));

vi.mock("../src/host", () => ({
  invoke: fixture.invoke,
  bindHostEvents: vi.fn(async (events) => { fixture.events = events; }),
  ask: fixture.ask,
  decodeOpenRecentMenuAction: vi.fn((id: string) => id.startsWith("open-recent:") ? id.slice("open-recent:".length) : null),
  closeProjectSession: vi.fn(),
  openDialog: vi.fn(),
  openProjectFolder: fixture.openProjectFolder,
  rememberRecentProject: vi.fn(),
  useRecentProject: fixture.useRecentProject,
  removeRecentProject: vi.fn(),
  saveDialog: vi.fn(),
  toMediaUrlAsync: vi.fn(),
}));

vi.mock("../src/settingsPanel", () => ({
  bindSettingsHost: vi.fn(),
  getSettings: () => fixture.settings,
  loadAppSettings: vi.fn(),
  populateChatModelSelect: vi.fn(),
  persistSettings: vi.fn(),
  renderSettingsPanel: vi.fn(),
  setSelectedChatModelFromSelect: vi.fn(),
  settingsAreDirty: () => false,
}));

vi.mock("../src/voicePanel", () => ({
  bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn(),
}));

vi.mock("../src/workflowDesk", () => ({
  bindWorkflowDesk: vi.fn(), clearWorkflowDesk: vi.fn(),
  jobsSummary: () => ({ running: 0, failed: 0, total: 0 }),
  loadWorkflowDesk: vi.fn(), toggleJobsDrawer: vi.fn(), setStudioWorkArea: vi.fn(),
  refreshWorkflowDesk: vi.fn(), renderWorkflowDesk: vi.fn(),
  attachDirectorContext: vi.fn(), detachDirectorContext: vi.fn(),
  directorContextEntries: () => [], omittedDirectorEntries: () => [],
  directorScopeHeadline: () => "Mom/Sam car exchange",
  directorTurnSelection: () => fixture.selection,
  workflowSelection: () => ({ path: null, excerpt: null }),
  workflowStatusLine: () => "Fixture scene ready",
}));

const root = join(import.meta.dirname, "..");
const originalTask = "Save the approved production note for the Mom/Sam car exchange, then read it back and report whether the dialogue is preserved. Do not generate media.";
const completedAction = "Saved production note: /fixture/black-hole-repro/production-note.md";
const approvalReply = "Approve saving the production note.\n[[workbench_approve:fixtureAction1]]";
const response = (reply: string, continueSuggested = false) => ({
  reply, model: "fixture-planner", toolEvents: [] as string[], continueSuggested, stopped: false,
});

function element<T extends HTMLElement = HTMLElement>(selector: string): T {
  const found = document.querySelector<T>(selector);
  expect(found, `Missing app element ${selector}`).not.toBeNull();
  return found!;
}

function calls(command: string) {
  return fixture.invoke.mock.calls.filter(([name]) => name === command);
}

async function settle() {
  // main.ts yields twice via requestAnimationFrame before invoking the host.
  // Fake timers make both paint and the 2s jobs timer deterministic.
  await vi.advanceTimersByTimeAsync(100);
}

async function requestApproval() {
  fixture.invoke.mockImplementation(async (command: string) => {
    if (command === "list_dir") return [];
    if (command === "director_turn") return response(approvalReply);
    throw new Error(`Unexpected IPC command: ${command}`);
  });
  element<HTMLTextAreaElement>("#chat-input").value = originalTask;
  element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await settle();
  expect(calls("director_turn")).toHaveLength(1);
  expect(element("#chat-messages").textContent).toContain("Approve saving");
  expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  return element<HTMLButtonElement>("#chat-messages button");
}

beforeEach(async () => {
  vi.resetModules();
  vi.useFakeTimers();
  fixture.invoke.mockReset();
  fixture.ask.mockReset().mockResolvedValue(true);
  fixture.openProjectFolder.mockReset().mockResolvedValue("/fixture/black-hole-repro");
  fixture.useRecentProject.mockReset().mockResolvedValue(undefined);
  fixture.events = null;
  // Use the actual app body, excluding head stylesheet/script URLs so happy-dom
  // cannot try loading Vite resources. main.ts is imported explicitly below.
  document.body.innerHTML = readFileSync(join(root, "index.html"), "utf8")
    .match(/<body>([\s\S]*)<\/body>/)![1];
  // Never allow the fixture to escape through a network API.
  vi.stubGlobal("fetch", vi.fn(() => { throw new Error("Network forbidden in approval reproduction"); }));
  // These entry-point listeners outlive individual DOM nodes; track and remove
  // them so re-importing the app does not multiply keyboard/preview handlers.
  vi.spyOn(document, "addEventListener");
  vi.spyOn(window, "addEventListener");
  fixture.invoke.mockImplementation(async (command: string) => {
    if (command === "list_dir") return [];
    throw new Error(`Unexpected IPC command during boot: ${command}`);
  });
  await import("../src/main");
  await settle();
  expect(element("#status-left").textContent).toBe("Ready — open a project to begin");
  element<HTMLButtonElement>("#home-open-project").click();
  await settle();
  expect(element("#shell-project-name").textContent).toBe("black-hole-repro");
});

afterEach(() => {
  for (const target of [document, window]) {
    const spy = vi.mocked(target.addEventListener);
    for (const [type, listener, options] of spy.mock.calls) {
      target.removeEventListener(type, listener, options);
    }
  }
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

describe.each(["approval", "send"])("Operation-owned streamed progress (%s)", (mode) => {
  it.each(["finish", "reject", "stop", "close owner", "cancel dialog", "same root", "different project"])("routes and retires progress across %s", async (outcome) => {
    const approve = mode === "approval" ? await requestApproval() : null;
    let finish!: (value: ReturnType<typeof response>) => void;
    let reject!: (error: Error) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_cancel") return null;
      if (command === (mode === "approval" ? "director_approve" : "director_turn")) {
        return new Promise((resolve, fail) => { finish = resolve; reject = fail; });
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    if (approve) {
      approve.click();
      approve.click();
    } else {
      element<HTMLTextAreaElement>("#chat-input").value = originalTask;
      element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    }
    await settle();
    const command = mode === "approval" ? "director_approve" : "director_turn";
    expect(calls(command)).toHaveLength(1);
    const requestId = calls(command)[0][1].progressId;
    expect(requestId).toEqual(expect.any(String));
    const emit = (message: string) => fixture.events!.onDirectorProgress({ requestId, message });
    element<HTMLTextAreaElement>("#chat-input").value = "Unsent distinctive draft";
    element<HTMLButtonElement>("#new-chat-tab").click();
    emit("A owns progress UNIQUE");
    expect(element("#chat-messages").textContent).not.toContain("A owns progress UNIQUE");
    element<HTMLButtonElement>("#chat-tabs .tab").click();
    expect(element("#chat-messages").textContent).toContain("A owns progress UNIQUE");
    element<HTMLButtonElement>("#chat-tabs .tab:last-child").click();

    if (outcome === "stop") {
      element<HTMLButtonElement>("#chat-stop-btn").click();
      await settle();
      const stoppedStatus = element("#status-left").textContent;
      emit("Ignored progress after Stop UNIQUE");
      expect(element("#status-left").textContent).toBe(stoppedStatus);
    } else if (outcome === "close owner") {
      element<HTMLButtonElement>("#chat-tabs .tab .tab-close").click();
    } else if (["cancel dialog", "same root", "different project"].includes(outcome)) {
      fixture.openProjectFolder.mockResolvedValueOnce(outcome === "cancel dialog" ? null
        : outcome === "same root" ? "/fixture/black-hole-repro" : "/fixture/project-C");
      fixture.events!.onMenu("open-project");
      await settle();
      const transitionStatus = element("#status-left").textContent;
      emit("Invalidated project progress UNIQUE");
      expect(element("#status-left").textContent).toBe(transitionStatus);
    }
    const statusBeforeResult = [element("#status-left").textContent, element("#status-right").textContent];
    if (outcome === "reject") reject(new Error("Distinct operation rejected UNIQUE"));
    else finish(outcome === "stop"
      ? { ...response("Completed work before Stop UNIQUE"), stopped: true } : response(completedAction));
    await settle();
    const terminalStatus = [element("#status-left").textContent, element("#status-right").textContent];
    emit("Late terminal progress UNIQUE");
    expect([element("#status-left").textContent, element("#status-right").textContent]).toEqual(terminalStatus);
    if (["close owner", "cancel dialog", "same root", "different project"].includes(outcome)) {
      expect(terminalStatus).toEqual(statusBeforeResult);
    }
    expect(element("#chat-messages").textContent).not.toContain(completedAction);
    expect(element("#chat-messages").textContent).not.toContain("UNIQUE");
    expect(element<HTMLTextAreaElement>("#chat-input").value).toBe("Unsent distinctive draft");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(element<HTMLButtonElement>("#chat-stop-btn").classList.contains("hidden")).toBe(true);
    if (outcome !== "close owner") {
      element<HTMLButtonElement>("#chat-tabs .tab").click();
      expect(element("#chat-messages").textContent).toContain(originalTask);
      expect(element("#chat-messages").textContent).not.toContain("A owns progress UNIQUE");
      expect(element("#chat-messages").textContent).not.toContain("Late terminal progress UNIQUE");
      expect(element("#chat-messages").textContent).not.toContain("Ignored progress after Stop UNIQUE");
      expect(element("#chat-messages").textContent).toContain(outcome === "reject"
        ? "Distinct operation rejected UNIQUE" : outcome === "stop" ? "Completed work before Stop UNIQUE" : completedAction);
    }
    expect(calls("director_turn")).toHaveLength(1);
  });
});

describe("Director handoff after an approved action", () => {
  it("can continue an ordinary planner turn through the same app harness", async () => {
    let turns = 0;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") {
        turns += 1;
        return turns === 1
          ? response("Paused to keep the desk responsive. Say continue to resume.", true)
          : response("Read-back complete: dialogue preserved.");
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(calls("director_turn")).toHaveLength(2);
    expect(calls("director_turn")[1][1].request.messages).toContainEqual({ role: "user", content: originalTask });
    expect(calls("director_approve")).toHaveLength(0);
    expect(element("#chat-messages").textContent).toContain("Read-back complete: dialogue preserved.");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("fails when ordinary send explorer refresh throws before continuation", async () => {
    let turns = 0;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") {
        turns += 1;
        return turns === 1
          ? response("Actual inspection result unique", true)
          : response("Complete terminal result");
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    const refreshSpy = vi.spyOn(element("#file-tree"), "replaceChildren")
      .mockImplementationOnce(() => { throw new Error("Fixture explorer refresh failed"); });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(refreshSpy).toHaveBeenCalled();
    expect(calls("director_turn")).toHaveLength(2);
    const secondHistory = calls("director_turn")[1][1].request.messages;
    expect(secondHistory).toContainEqual({ role: "user", content: originalTask });
    expect(secondHistory).toContainEqual({ role: "assistant", content: "Actual inspection result unique" });
    expect(element("#chat-messages").textContent).not.toContain("Director turn failed");
    expect(element("#chat-messages").textContent).toContain("Complete terminal result");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("returns a completed local action to the planner with the original task, result and scene scope", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      // Completed specialists currently use SpecialistResult::done(false).
      // Completion of this action does not complete the user's two-step task.
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") return response("Read-back complete: dialogue preserved.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();

    expect(calls("director_approve")).toEqual([["director_approve", { id: "fixtureAction1", progressId: expect.any(String) }]]);
    expect(element("#chat-messages").textContent).toContain(completedAction);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(calls("director_turn"), "The completed action must reach the planner for the outstanding read-back step").toHaveLength(2);
    const request = calls("director_turn")[1][1].request;
    expect(request.messages).toEqual(expect.arrayContaining([
      { role: "user", content: originalTask },
      { role: "assistant", content: completedAction },
    ]));
    expect(request.resume, "Approval starts a history-backed planner turn, not a stale paused checkpoint").toBe(false);
    expect(request.messages.filter((message: { role: string }) => message.role === "user"))
      .toEqual([{ role: "user", content: originalTask }]);
    expect(request.context).toMatchObject({
      projectRoot: "/fixture/black-hole-repro",
      workflowSceneStableId: "scene-mom-sam", workflowShotStableId: "shot-mom",
    });
    expect(element("#chat-messages").textContent).toContain("Read-back complete: dialogue preserved.");
    expect(document.querySelector("#chat-messages button")).toBeNull();
  });

  it.each([false, true])("keeps automatic approval hops free of invented user input (plannerPaused=%s)", async (paused) => {
    const approve = await requestApproval();
    let handoffTurns = 0;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") {
        handoffTurns += 1;
        if (paused && handoffTurns === 1) {
          return { ...response("Planner yielding for paint."), plannerPaused: true };
        }
        return handoffTurns === (paused ? 2 : 1)
          ? response("Read-back complete; checking dialogue preservation.", true)
          : response("Read-back complete: dialogue preserved.");
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    const originalContext = calls("director_turn")[0][1].request.context;
    const previousScene = fixture.selection.sceneStableId;
    fixture.selection.sceneStableId = "scene-selected-after-approval-card";
    element<HTMLTextAreaElement>("#chat-input").value = "Unsent draft";
    try {
      approve.click();
      await settle();
      await settle();
      const requests = calls("director_turn").map(([, args]) => args.request);
      expect(requests).toHaveLength(paused ? 4 : 3);
      expect(requests.map((request) => request.resume)).toEqual(paused
        ? [false, false, true, false] : [false, false, false]);
      for (const request of requests.slice(1)) {
        expect.soft(request.messages.filter((message: { role: string }) => message.role === "user"))
          .toEqual([{ role: "user", content: originalTask }]);
        expect(request.context).toEqual(originalContext);
        expect(request.messages).toContainEqual({ role: "assistant", content: completedAction });
      }
      expect(element("#chat-messages").textContent).toContain("Read-back complete: dialogue preserved.");
      expect(document.querySelectorAll("#chat-messages .chat-bubble.user")).toHaveLength(1);
      expect(element<HTMLTextAreaElement>("#chat-input").value).toBe("Unsent draft");
      expect(calls("director_approve")).toHaveLength(1);
    } finally {
      fixture.selection.sceneStableId = previousScene;
    }
  });

  it("waits for a new explicit approval even when a pending result suggests continuation", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response("Review the next action.\n[[workbench_approve:fixtureAction2]]", true);
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    expect(calls("director_approve")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(1);
    expect(element<HTMLButtonElement>("#chat-messages button").textContent).toBe("Approve");
    expect(element("#chat-messages").textContent).toContain("Review the next action.");
  });

  it.each([
    ["pending work", "Work is pending.\n[[workbench_pending:fixturePending1]]", []],
    ["spend approval", "Additional spend approval required.", ["approve:awaiting-spend-authorization"]],
    ["structured failure", "Approved action failed: fixture write rejected.", ["approve:error"]],
  ])("does not continue after %s", async (_label, reply, toolEvents) => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return { ...response(reply, true), toolEvents };
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(calls("director_approve")).toHaveLength(1);
    expect(element("#chat-messages").textContent).toContain(reply.split("\n")[0]);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("keeps the approval scope and does not erase a draft or execute a new approval", async () => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      if (command === "director_turn") return response("Approve another write.\n[[workbench_approve:fixtureAction2]]", true);
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    approve.click();
    await settle();
    const previousScene = fixture.selection.sceneStableId;
    fixture.selection.sceneStableId = "scene-selected-during-approval";
    element<HTMLTextAreaElement>("#chat-input").value = "Unsent draft";
    try {
      finish(response(completedAction));
      await settle();
      expect(calls("director_turn")).toHaveLength(2);
      expect(calls("director_approve")).toHaveLength(1);
      expect(calls("director_turn")[1][1].request.context.workflowSceneStableId).toBe(previousScene);
      expect(element<HTMLTextAreaElement>("#chat-input").value).toBe("Unsent draft");
      expect(element<HTMLButtonElement>("#chat-messages button").textContent).toBe("Approve");
    } finally {
      fixture.selection.sceneStableId = previousScene;
    }
  });

  it("retains the original request scope if selection changes before approval", async () => {
    const approve = await requestApproval();
    const previousScene = fixture.selection.sceneStableId;
    fixture.selection.sceneStableId = "scene-selected-after-planning";
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") return response("Read-back complete: dialogue preserved.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    try {
      approve.click();
      await settle();
      expect(calls("director_turn")).toHaveLength(2);
      expect(calls("director_turn")[1][1].request.context.workflowSceneStableId).toBe(previousScene);
    } finally {
      fixture.selection.sceneStableId = previousScene;
    }
  });

  it("owns streamed approval progress in A while B is selected", async () => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    element<HTMLButtonElement>("#new-chat-tab").click();
    // Request identity follows the real IPC request; no active-chat lookup.
    fixture.events!.onDirectorProgress({ requestId: calls("director_approve")[0][1].progressId, message: "Host executing approved action UNIQUE" });
    expect.soft(element("#chat-messages").textContent).not.toContain("Host executing approved action UNIQUE");
    element<HTMLButtonElement>("#chat-tabs .tab").click();
    expect.soft(element("#chat-messages").textContent).toContain("Host executing approved action UNIQUE");
    element<HTMLButtonElement>("#chat-tabs .tab:last-child").click();
    finish(response(completedAction));
    await settle();
    expect.soft(element("#chat-messages").textContent).not.toContain("Host executing approved action UNIQUE");
    element<HTMLButtonElement>("#chat-tabs .tab").click();
    expect(element("#chat-messages").textContent).toContain(completedAction);
    expect(element("#chat-messages").textContent).not.toContain("Host executing approved action UNIQUE");
  });

  it("rejects uncorrelated progress after a new request starts", async () => {
    await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "director_turn") return new Promise((resolve) => { finish = resolve; });
      if (command === "list_dir") return [];
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = "New distinct request";
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    const status = element("#status-left").textContent;
    fixture.events!.onDirectorProgress("OLD request progress must be ignored");
    expect.soft(element("#status-left").textContent).toBe(status);
    expect.soft(element("#chat-messages").textContent).not.toContain("OLD request progress must be ignored");
    finish(response("New request actual result"));
    await settle();
  });

  it("matches progress only to its live IPC identity, including a newer request", async () => {
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") return new Promise((resolve) => { finish = resolve; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    const submit = async (text: string) => {
      element<HTMLTextAreaElement>("#chat-input").value = text;
      element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await settle();
      return calls("director_turn").at(-1)![1].progressId;
    };
    const oldId = await submit(originalTask);
    fixture.events!.onDirectorProgress({ requestId: oldId, message: "First request live progress UNIQUE" });
    expect(element("#chat-messages").textContent).toContain("First request live progress UNIQUE");
    finish(response("First real result"));
    await settle();
    const newId = await submit("New distinct task");
    expect(newId).not.toBe(oldId);
    expect(calls("director_turn")[1][1].request.messages).toContainEqual({ role: "user", content: originalTask });
    expect(calls("director_turn")[1][1].request.messages).toContainEqual({ role: "assistant", content: "First real result" });
    expect(JSON.stringify(calls("director_turn")[1][1].request.messages)).not.toContain("UNIQUE");
    const status = element("#status-left").textContent;
    for (const payload of [null, 7, {}, "Legacy string UNIQUE", { requestId: newId, message: 3 },
      { requestId: oldId, message: "Stale prior request UNIQUE" }]) fixture.events!.onDirectorProgress(payload);
    expect(element("#status-left").textContent).toBe(status);
    expect(element("#chat-messages").textContent).not.toContain("UNIQUE");
    fixture.events!.onDirectorProgress({ requestId: newId, message: "Second matched progress UNIQUE" });
    expect(element("#chat-messages").textContent).toContain("Second matched progress UNIQUE");
    expect(element("#status-left").textContent).toBe("Second matched progress UNIQUE");
    finish(response("Second real result"));
    await settle();
    fixture.events!.onDirectorProgress({ requestId: newId, message: "Late second request UNIQUE" });
    expect(element("#chat-messages").textContent).toContain("Second real result");
    expect(element("#chat-messages").textContent).not.toContain("UNIQUE");
    expect(element("#status-left").textContent).toBe("Director reply");
  });

  it("does not let a late Stop failure overwrite a replacement project's status", async () => {
    let finish!: (value: ReturnType<typeof response>) => void;
    let rejectStop!: (error: Error) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") return new Promise((resolve) => { finish = resolve; });
      if (command === "director_cancel") return new Promise((_resolve, reject) => { rejectStop = reject; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();
    finish({ ...response("Actual stopped result"), stopped: true });
    await settle();
    fixture.openProjectFolder.mockResolvedValueOnce("/fixture/project-B");
    fixture.events!.onMenu("open-project");
    await settle();
    const status = [element("#status-left").textContent, element("#status-right").textContent];
    rejectStop(new Error("Late cancellation rejected"));
    await settle();
    expect([element("#status-left").textContent, element("#status-right").textContent]).toEqual(status);
  });

  it("preserves terminal stopped status while its desk refresh and cancellation reply remain pending", async () => {
    const { refreshWorkflowDesk } = await import("../src/workflowDesk");
    let finishRefresh!: () => void;
    let finishTurn!: (value: ReturnType<typeof response>) => void;
    let rejectStop!: (error: Error) => void;
    vi.mocked(refreshWorkflowDesk).mockImplementationOnce(() => new Promise((resolve) => { finishRefresh = resolve; }));
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") return new Promise((resolve) => { finishTurn = resolve; });
      if (command === "director_cancel") return new Promise((_resolve, reject) => { rejectStop = reject; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    element<HTMLButtonElement>("#chat-stop-btn").click();
    finishTurn({ ...response("Stopped with an actual retained observation"), stopped: true });
    await settle();
    expect(element("#status-left").textContent).toBe("Director stopped");
    rejectStop(new Error("Late Stop response after terminal result"));
    await settle();
    expect(element("#status-left").textContent).toBe("Director stopped");
    finishRefresh();
    await settle();
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it.each(["approval", "send"])("finishes Stop cleanly during %s post-result refresh", async (mode) => {
    const approve = mode === "approval" ? await requestApproval() : null;
    const { refreshWorkflowDesk } = await import("../src/workflowDesk");
    let finishRefresh!: () => void;
    vi.mocked(refreshWorkflowDesk).mockImplementationOnce(() => new Promise((resolve) => { finishRefresh = resolve; }));
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve" || command === "director_turn") return response(completedAction);
      if (command === "director_cancel") return {};
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    if (approve) approve.click();
    else {
      element<HTMLTextAreaElement>("#chat-input").value = originalTask;
      element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    }
    await settle();
    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();
    finishRefresh();
    await settle();
    expect(element("#status-left").textContent).toBe("Director stopped");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(element("#chat-messages").textContent).not.toContain("Stopping Director");
    expect(calls("director_turn")).toHaveLength(1);
  });

  it.each([false, true])("keeps recursive paint owned and stoppable (plannerPaused=%s)", async (paused) => {
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_cancel") return null;
      if (command === "director_turn") return { ...response("First actual result", !paused), plannerPaused: paused };
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    // Complete request's two paint frames, then hold the recursive-hop paint.
    await vi.advanceTimersByTimeAsync(40);
    expect(calls("director_turn")).toHaveLength(1);
    expect(element<HTMLButtonElement>("#chat-stop-btn").classList.contains("hidden")).toBe(false);
    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(calls("director_cancel")).toHaveLength(1);
    expect(element("#status-left").textContent).toBe("Director stopped");
    expect(element("#chat-messages").textContent).not.toContain("Stopping Director…");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it.each([false, true])("bounds recursive progress lifetimes and excludes them from history (plannerPaused=%s)", async (paused) => {
    fixture.invoke.mockImplementation(async (command: string, args: { progressId?: string }) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") {
        fixture.events!.onDirectorProgress({ requestId: args.progressId, message: "System streamed progress UNIQUE" });
        return { ...response("Actual bounded observation", !paused), plannerPaused: paused };
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = originalTask;
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    element<HTMLTextAreaElement>("#chat-input").value = "Draft across every hop";
    await vi.advanceTimersByTimeAsync(10_000);
    const requests = calls("director_turn");
    expect(requests).toHaveLength(49); // first request + the existing 48-hop cap
    expect(new Set(requests.map(([, args]) => args.progressId)).size).toBe(49);
    for (const [, args] of requests) {
      expect(args.request.messages.filter((m: { role: string }) => m.role === "user"))
        .toEqual([{ role: "user", content: originalTask }]);
      expect(JSON.stringify(args.request.messages)).not.toContain("System streamed progress UNIQUE");
    }
    expect(element("#chat-messages").textContent).not.toContain("System streamed progress UNIQUE");
    expect(element<HTMLTextAreaElement>("#chat-input").value).toBe("Draft across every hop");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    const status = element("#status-left").textContent;
    fixture.events!.onDirectorProgress({ requestId: requests.at(-1)![1].progressId, message: "Late bounded event" });
    expect(element("#status-left").textContent).toBe(status);
  });

  it.each(["approval", "send"])("passes operation validity through a delayed %s desk refresh", async (mode) => {
    const { refreshWorkflowDesk } = await import("../src/workflowDesk");
    const refresh = vi.mocked(refreshWorkflowDesk);
    const approve = mode === "approval" ? await requestApproval() : null;
    refresh.mockClear();
    let finish!: () => void;
    let isCurrent!: () => boolean;
    refresh.mockImplementationOnce((valid) => new Promise((resolve) => {
      isCurrent = valid!;
      finish = resolve;
    }));
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve" || command === "director_turn") return response(completedAction);
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    if (approve) approve.click();
    else {
      element<HTMLTextAreaElement>("#chat-input").value = originalTask;
      element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    }
    await settle();
    expect(refresh).toHaveBeenCalledWith(expect.any(Function), false);
    expect(isCurrent()).toBe(true);
    const requestId = calls(mode === "approval" ? "director_approve" : "director_turn").at(-1)![1].progressId;
    const terminalStatus = element("#status-left").textContent;
    fixture.events!.onDirectorProgress({ requestId, message: "Late during result refresh UNIQUE" });
    expect(element("#status-left").textContent).toBe(terminalStatus);
    fixture.openProjectFolder.mockResolvedValueOnce(null);
    fixture.events!.onMenu("open-project");
    await settle();
    expect(isCurrent()).toBe(false);
    finish();
    await settle();
    expect(element("#chat-messages").textContent).not.toContain("Late during result refresh UNIQUE");
    expect(element("#chat-messages").textContent).toContain(completedAction);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("does not hand an approval result to another chat tab", async () => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    element<HTMLButtonElement>("#new-chat-tab").click();
    finish(response(completedAction));
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(element("#chat-messages").textContent).not.toContain(completedAction);
  });

  it.each(["dialog", "recent"])("does not continue during a %s project switch while the renderer still shows the old root", async (entry) => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    let finishDirectory!: (value: never[]) => void;
    let hostRoot = "/fixture/black-hole-repro";
    fixture.openProjectFolder.mockImplementationOnce(async () => {
      hostRoot = "/fixture/another-project";
      return hostRoot;
    });
    fixture.useRecentProject.mockImplementationOnce(async (root) => { hostRoot = root; });
    fixture.invoke.mockImplementation(async (command: string, args: { path?: string }) => {
      if (command === "list_dir") return args.path === "/fixture/another-project"
        ? new Promise((resolve) => { finishDirectory = resolve; }) : [];
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      if (command === "director_turn") return response("Unexpected planner call in another project.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    fixture.events!.onMenu(entry === "dialog" ? "open-project" : "open-recent:/fixture/another-project");
    await settle();
    expect(hostRoot).toBe("/fixture/another-project");
    expect(element("#shell-project-name").textContent).toBe("black-hole-repro");
    finish(response(completedAction));
    await settle();
    const turnsBeforeDirectoryCompletes = calls("director_turn").length;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    finishDirectory([]);
    await settle();
    expect(turnsBeforeDirectoryCompletes).toBe(1);
    expect(calls("director_turn")).toHaveLength(1);
  });

  it.each(["cancel", "same root"])("invalidates an old chain even after a project dialog completes with %s", async (outcome) => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    fixture.openProjectFolder.mockResolvedValueOnce(outcome === "cancel" ? null : "/fixture/black-hole-repro");
    approve.click();
    await settle();
    fixture.events!.onMenu("open-project");
    await settle();
    finish(response(completedAction));
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(element("#shell-project-name").textContent).toBe("black-hole-repro");

    // The invalidation applies to the old chain, not to later explicit work.
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") return response("New explicit task completed.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = "New explicit task";
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(calls("director_turn")).toHaveLength(2);
  });

  it("blocks a new approval while a project dialog is pending", async () => {
    const approve = await requestApproval();
    let finishDialog!: (value: string | null) => void;
    fixture.openProjectFolder.mockImplementationOnce(() => new Promise((resolve) => { finishDialog = resolve; }));
    fixture.events!.onMenu("open-project");
    await settle();
    approve.click();
    await settle();
    const approvalsDuringDialog = calls("director_approve").length;
    finishDialog(null);
    await settle();
    expect(approvalsDuringDialog).toBe(0);
    expect(calls("director_turn")).toHaveLength(1);
  });

  it("honors Stop while approval is in flight and allows a later explicit approval", async () => {
    const approve = await requestApproval();
    let finish!: (value: ReturnType<typeof response>) => void;
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_cancel") return null;
      if (command === "director_approve") return new Promise((resolve) => { finish = resolve; });
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();
    expect(calls("director_cancel")).toHaveLength(1);
    finish(response("Review the next action.\n[[workbench_approve:fixtureAction2]]", true));
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(element("#status-left").textContent).toBe("Director stopped");

    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") return response("Read-back complete: dialogue preserved.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLButtonElement>("#chat-messages button").click();
    await settle();
    expect(calls("director_turn")).toHaveLength(2);
    expect(calls("director_approve")).toHaveLength(2);
    expect(element("#chat-messages").textContent).toContain("Read-back complete: dialogue preserved.");
  });

  it.each(["canceled dialog", "new project", "new chat"])("cleans up an abandoned handoff during pre-request paint: %s", async (transition) => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") return response("Later explicit task completed.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(element("#status-left").textContent).toBe("Director working…");
    expect(calls("director_turn")).toHaveLength(1);

    if (transition === "new chat") {
      element<HTMLButtonElement>("#new-chat-tab").click();
    } else {
      fixture.openProjectFolder.mockResolvedValueOnce(
        transition === "canceled dialog" ? null : "/fixture/another-project",
      );
      fixture.events!.onMenu("open-project");
      await vi.advanceTimersByTimeAsync(0);
    }
    const transitionStatus = [element("#status-left").textContent, element("#status-right").textContent];
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(element("#status-left").textContent).toBe(
      transition === "new project" ? "Studio" : "Director paused",
    );
    if (transition === "new project") {
      expect([element("#status-left").textContent, element("#status-right").textContent]).toEqual(transitionStatus);
    }
    if (transition === "new chat") element<HTMLButtonElement>("#chat-tabs .tab").click();
    expect(element("#chat-messages").textContent).not.toContain("Director working…");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(element<HTMLButtonElement>("#chat-stop-btn").classList.contains("hidden")).toBe(true);

    element<HTMLTextAreaElement>("#chat-input").value = "Later explicit task";
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(calls("director_turn")).toHaveLength(2);
    expect(calls("director_turn")[1][1].request.messages).not.toContainEqual({
      role: "assistant", content: "Director working…",
    });
  });

  it("cleans up Stop during the post-approval handoff's pre-request paint", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_cancel") return null;
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    // Flush approval/refresh promises without running the handoff's paint frames.
    await vi.advanceTimersByTimeAsync(0);
    expect(element("#chat-messages").textContent).toContain(completedAction);
    expect(element("#status-left").textContent).toBe("Director working…");
    expect(calls("director_turn")).toHaveLength(1);

    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();
    expect(calls("director_cancel")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(1);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect.soft(element("#chat-messages").textContent).not.toContain("Stopping Director…");
    expect.soft(element("#chat-messages").textContent).not.toContain("Director working…");
    expect(element("#status-left").textContent).toBe("Director stopped");
  });

  it("cleans up Stop during an ordinary explicit send's pre-request paint", async () => {
    // Set up an explicit send (no autoContinue, no afterApproval)
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_cancel") return null;
      if (command === "director_turn") return response("Director reply.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });

    element<HTMLTextAreaElement>("#chat-input").value = "Explicit task";
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));

    // Flush the submit microtask and enter paintSoon, but do NOT complete the
    // two requestAnimationFrame yields yet. This holds the pre-request paint.
    await vi.advanceTimersByTimeAsync(0);

    // At this point the send is in-flight and "Director working…" is showing.
    expect(element("#status-left").textContent).toBe("Director working…");
    expect(calls("director_turn")).toHaveLength(0);

    // User clicks Stop while paint is held.
    element<HTMLButtonElement>("#chat-stop-btn").click();
    await settle();

    // Stop must have been invoked, but NO director_turn must have been dispatched.
    expect(calls("director_cancel")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(0);

    // Progress bubble removed, busy/stopping chrome released, terminal status.
    expect(element("#chat-messages").textContent).not.toContain("Director working…");
    expect(element("#chat-messages").textContent).not.toContain("Stopping Director…");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
    expect(element<HTMLButtonElement>("#chat-stop-btn").classList.contains("hidden")).toBe(true);
    expect(element("#status-left").textContent).toBe("Director stopped");

    // A later explicit send must still work.
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_turn") return response("Later reply.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    element<HTMLTextAreaElement>("#chat-input").value = "Later task";
    element("#chat-form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await settle();
    expect(calls("director_turn")).toHaveLength(1);
    expect(element("#chat-messages").textContent).toContain("Later reply.");
  });

  it("hands successful approval back to the planner when explorer refresh rejects", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return response(completedAction);
      if (command === "director_turn") return response("Read-back complete: dialogue preserved.");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    // A list_dir rejection is already handled inside renderTreeNode. Fail the
    // actual explorer render instead, without replacing the approval/chat path.
    const refresh = vi.spyOn(element("#file-tree"), "replaceChildren")
      .mockImplementationOnce(() => { throw new Error("Fixture explorer refresh failed"); });
    approve.click();
    await settle();
    expect(refresh).toHaveBeenCalled();
    expect(element("#chat-messages").textContent).toContain(completedAction);
    expect.soft(element("#chat-messages").textContent).not.toContain("Approval failed:");
    expect(calls("director_turn")).toHaveLength(2);
    expect(calls("director_approve")).toHaveLength(1);
    expect(calls("director_turn")[1][1].request.messages).toContainEqual({
      role: "assistant", content: completedAction,
    });
    expect(element("#chat-messages").textContent).toContain("Read-back complete: dialogue preserved.");
    expect(element("#status-left").textContent).toBe("Director reply");
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("does not continue after the approved action fails", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "director_approve") throw new Error("Fixture write rejected");
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    expect(element("#chat-messages").textContent).toContain("Approval failed: Error: Fixture write rejected");
    // A transport rejection can arrive after the host has consumed the action.
    // Rebuilding the originating chat must not recreate a replay affordance.
    expect([...document.querySelectorAll("#chat-messages button")].some(b => b.textContent === "Approve")).toBe(false);
    element<HTMLButtonElement>("#new-chat-tab").click();
    element<HTMLButtonElement>("#chat-tabs .tab").click();
    await settle();
    expect([...document.querySelectorAll("#chat-messages button")].some(b => b.textContent === "Approve")).toBe(false);
    expect(calls("director_turn")).toHaveLength(1);
    expect(calls("director_approve")).toHaveLength(1);
    expect(element<HTMLButtonElement>("#chat-send-btn").disabled).toBe(false);
  });

  it("publishes delivered media continuation as a fresh approval without a planner hop", async () => {
    const approve = await requestApproval();
    const continuationText = "Resume Mom using the delivered performance WAV.";
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") {
        return { ...response("Paid media awaits authorization."), toolEvents: ["approve:awaiting-spend-authorization"] };
      }
      if (command === "confirm_unbounded_spend") return "exact-child-permit";
      if (command === "media_generate_confirmed") {
        return {
          mediaResult: JSON.stringify({ saved: { saved: true, path: "/fixture/black-hole-repro/Scenes/Scene1/Cut1/S01C01.wav" } }),
          continuation: {
            ...response(`${continuationText}\n[[workbench_approve:resumeaudiotest]]`),
            workflowContinuation: true,
          },
        };
      }
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    expect(calls("director_approve")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(1);
    fixture.events!.onSpendConfirmationRequired({
      provider: "atlas-cloud", category: "media", operation: "atlas_generate_audio",
      model: "bytedance/seed-audio-1.0", projectRoot: "/fixture/black-hole-repro",
      ownerRequestId: calls("director_approve")[0][1].progressId,
      quote: { currency: "USD", requestHash: "a".repeat(64), ledgerRevision: "revision-one", disclosure: "Exact audio operation requires consent." },
      args: { model: "bytedance/seed-audio-1.0", save_path: "/fixture/black-hole-repro/Scenes/Scene1/Cut1/S01C01.wav" },
    });
    await settle();
    expect(fixture.ask).toHaveBeenCalledTimes(1);
    expect(calls("confirm_unbounded_spend")).toHaveLength(1);
    expect(calls("media_generate_confirmed")).toHaveLength(1);
    expect(calls("director_approve")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(1);
    expect(element("#chat-messages").textContent).toContain(continuationText);
    const freshApprovals = Array.from(document.querySelectorAll<HTMLButtonElement>("#chat-messages button"))
      .filter((button) => button.textContent?.trim() === "Approve" && !button.disabled);
    expect(freshApprovals).toHaveLength(1);
  });

  it("does not send a bounded native workflow completion back to the planner", async () => {
    const approve = await requestApproval();
    fixture.invoke.mockImplementation(async (command: string) => {
      if (command === "list_dir") return [];
      if (command === "director_approve") return { ...response("The bounded talking shot was saved."), workflowContinuation: true };
      throw new Error(`Unexpected IPC command: ${command}`);
    });
    approve.click();
    await settle();
    expect(element("#chat-messages").textContent).toContain("The bounded talking shot was saved.");
    expect(calls("director_approve")).toHaveLength(1);
    expect(calls("director_turn")).toHaveLength(1);
  });

});
