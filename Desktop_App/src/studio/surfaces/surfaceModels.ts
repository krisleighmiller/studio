import type { CharacterRecord, SceneRecord, SceneReport } from "../../workflowTypes";

export type SurfaceTone = "ready" | "missing" | "blocked" | "loading" | "error" | "empty";

export type SurfaceAction = {
  id: string;
  label: string;
  enabled: boolean;
  reason: string | null;
};

export type SurfacePresentation = {
  status: string;
  tone: SurfaceTone;
  banner: string | null;
  emptyTitle: string | null;
  emptyDetail: string | null;
  loading: string | null;
  error: string | null;
  blocked: string[];
  actions: SurfaceAction[];
};

function action(id: string, label: string, enabled: boolean, reason: string | null): SurfaceAction {
  return { id, label, enabled, reason: enabled ? null : reason };
}

function chrome(input: {
  status: string;
  tone: SurfaceTone;
  banner?: string | null;
  emptyTitle?: string | null;
  emptyDetail?: string | null;
  busy: string | null;
  error: string | null;
  blocked?: string[];
  actions?: SurfaceAction[];
}): SurfacePresentation {
  const loading = input.busy;
  const tone: SurfaceTone = loading ? "loading" : input.error ? "error" : input.blocked?.length ? "blocked" : input.tone;
  return {
    status: input.status,
    tone,
    banner: input.banner ?? null,
    emptyTitle: input.emptyTitle ?? null,
    emptyDetail: input.emptyDetail ?? null,
    loading,
    error: input.error,
    blocked: input.blocked ?? [],
    actions: input.actions ?? [],
  };
}

export function buildIntentModel(input: {
  scene: SceneRecord;
  busy: string | null;
  error: string | null;
  chatReady: boolean;
  blocked: string[];
}): SurfacePresentation {
  const approved = Boolean(input.scene.intent_approved);
  const drafted = Boolean(input.scene.intent);
  const status = approved ? "approved" : drafted ? "draft" : "missing";
  const banner = approved
    ? "Intent is approved. You can still edit and save a new draft."
    : drafted
      ? "Intent is a draft. Approve it before relying on it for sketches."
      : "No scene intent yet. Draft from the script, then edit and approve.";
  const wait = input.busy;
  return chrome({
    status,
    tone: approved ? "ready" : "missing",
    banner,
    busy: input.busy,
    error: input.error,
    blocked: input.blocked,
    actions: [
      action("draft", "Draft from script", !wait && input.chatReady, wait ?? "Pick a chat model in the dropdown first."),
      action("save", "Save draft", !wait, wait),
      action("approve", "Approve intent", !wait && drafted, wait ?? "Save a draft before approving."),
    ],
  });
}

export function buildReferenceModel(input: {
  scene: SceneRecord;
  report: SceneReport | undefined;
  busy: string | null;
  error: string | null;
  blocked: string[];
  chatReady: boolean;
}): SurfacePresentation {
  const planned = input.report?.planned_visual_sources ?? 0;
  const ready = input.report?.render_ready_sources ?? 0;
  const cuts = input.scene.cuts.length;
  if (!cuts) {
    return chrome({
      status: "empty",
      tone: "empty",
      banner: "No visual-source strategies are approved. Choose how each cut obtains its opening image; not every cut needs a preproduction start frame.",
      emptyTitle: "No cuts are available.",
      emptyDetail: "Draft cut sketches before generating cut-specific start frames.",
      busy: input.busy,
      error: input.error,
      blocked: input.blocked,
    });
  }
  return chrome({
    status: planned ? (ready === cuts ? "ready" : "planned") : "missing",
    tone: planned ? "ready" : "missing",
    banner: planned
      ? `${planned} of ${cuts} cuts have a planned visual-source strategy; ${ready} are render ready. Continuity-linked cuts remain correctly planned while they wait for a selected predecessor take.`
      : "No visual-source strategies are approved. Choose how each cut obtains its opening image; not every cut needs a preproduction start frame.",
    busy: input.busy,
    error: input.error,
    blocked: input.blocked,
    actions: [
      action("save-source", "Save source strategy", !input.busy, input.busy),
      action("bind-frame", "Bind selected frame", !input.busy, input.busy),
      action(
        "draft-prompt",
        "Draft image prompt",
        !input.busy && input.chatReady,
        input.busy ?? "Pick a chat model in the dropdown first.",
      ),
      action("save-prompt", "Save prompt", !input.busy, input.busy),
      action("generate", "Generate start frame", !input.busy, input.busy),
      action("attach-still", "Attach still", !input.busy, input.busy),
      action("approve", "Use", !input.busy, input.busy),
      action("reject", "Reject", !input.busy, input.busy),
      action("edit", "Edit", !input.busy, input.busy),
      action("delete", "Delete", !input.busy, input.busy),
    ],
  });
}

export function buildSketchModel(input: {
  scene: SceneRecord;
  report: SceneReport | undefined;
  busy: string | null;
  error: string | null;
  blocked: string[];
  chatReady: boolean;
}): SurfacePresentation {
  const verdict = input.report?.mix_verdict;
  const counted =
    verdict && verdict.mix.performance + verdict.mix.other_dialogue + verdict.mix.silent > 0
      ? verdict
      : null;
  const backfill = Boolean(input.report?.can_backfill_sketches);
  const hasNotes = input.scene.cuts.some((cut) => cut.production_note_path);
  let emptyTitle: string | null = null;
  let emptyDetail: string | null = null;
  let banner: string | null = null;
  let tone: SurfaceTone = "ready";
  let status = "present";
  if (counted) {
    status = counted.status;
    tone = counted.status === "reject" || counted.status === "warn" ? "blocked" : "ready";
    banner = `${counted.strategy.replace(/_/g, " ")} · ${counted.mix.performance} performance / ${counted.mix.other_dialogue} other dialogue / ${counted.mix.silent} silent${
      counted.messages.length ? `\n${counted.messages.join("\n")}` : ""
    }`;
  } else if (backfill) {
    status = "missing";
    tone = "missing";
    emptyTitle = "Cut sketches are missing.";
    emptyDetail =
      "Backfill from production notes, then change shot roles so a cinematic-balanced mix is not all lip-sync. Or draft sketches from scene intent.";
  } else if (!input.scene.intent) {
    status = "blocked";
    tone = "blocked";
    emptyTitle = "Scene intent is missing.";
    emptyDetail = "Draft and approve scene intent first. Sketches need that planning layer.";
  } else if (!input.scene.cuts.length) {
    status = "empty";
    tone = "empty";
    emptyTitle = "No cuts yet.";
    emptyDetail = "Draft sketches from scene intent, or backfill them from production notes.";
  }
  const wait = input.busy;
  return chrome({
    status,
    tone,
    banner,
    emptyTitle,
    emptyDetail,
    busy: input.busy,
    error: input.error,
    blocked: input.blocked,
    actions: [
      action("strategy", "Set strategy", !wait, wait),
      action("alternate", "New alternate angle", !wait, wait),
      action("backfill", "Backfill from notes", !wait && (backfill || hasNotes), wait ?? (hasNotes ? null : "No production notes to backfill.")),
      action(
        "draft",
        "Draft sketches",
        !wait && input.chatReady,
        wait ?? "Pick a chat model in the dropdown first.",
      ),
      action(
        "approve-animatic",
        "Approve rough cut",
        !wait && !input.scene.animatic?.approved && (input.scene.animatic?.shots?.length ?? 0) > 0,
        wait ?? (input.scene.animatic?.approved ? "Rough cut is already approved." : "Add animatic shots before approving."),
      ),
    ],
  });
}

export function buildNotesModel(input: {
  scene: SceneRecord;
  busy: string | null;
  error: string | null;
  blocked: string[];
  chatReady: boolean;
}): SurfacePresentation {
  const cuts = input.scene.cuts.length;
  const notes = input.scene.cuts.filter((cut) => cut.production_note_path).length;
  const wait = input.busy;
  return chrome({
    status: cuts === 0 ? "empty" : notes === cuts ? "ready" : "missing",
    tone: cuts === 0 ? "empty" : notes ? "ready" : "missing",
    banner:
      "Build complete moving-image prompts from the script, scene intent, ordered cut sketches, dialogue, approved scene references, and each cut’s approved visual source. Use this after planning or reference images change.",
    emptyTitle: cuts === 0 ? "No cuts to write notes for." : null,
    emptyDetail: cuts === 0 ? "Draft cut sketches before building production notes." : null,
    busy: input.busy,
    error: input.error,
    blocked: input.blocked,
    actions: [
      action(
        "sync",
        "Build all production video prompts",
        !wait && input.chatReady,
        wait ?? "Pick a chat model in the dropdown first.",
      ),
      action(
        "sync-cut",
        "Build full video prompt",
        !wait && input.chatReady,
        wait ?? "Pick a chat model in the dropdown first.",
      ),
      action("refresh", "Refresh rendered takes", !wait, wait),
      action("alternate", "New alternate angle", !wait, wait),
    ],
  });
}

export function buildRenderQueueModel(input: {
  scene: SceneRecord;
  busy: string | null;
  error: string | null;
  blocked: string[];
}): SurfacePresentation {
  const takes = input.scene.cuts.reduce((total, cut) => total + (cut.takes?.length ?? 0), 0);
  const blockedCuts = input.scene.cuts.filter((cut) => cut.package_needs_review).length;
  return chrome({
    status: input.scene.cuts.length === 0 ? "empty" : takes ? "ready" : "missing",
    tone: input.scene.cuts.length === 0 ? "empty" : takes ? "ready" : "missing",
    banner: takes
      ? `${takes} take(s) recorded. Render another take here or directly from Production Notes.`
      : "No takes recorded yet. Ready cuts can be rendered here or directly from Production Notes.",
    emptyTitle: input.scene.cuts.length === 0 ? "No cuts in the render queue." : null,
    emptyDetail: input.scene.cuts.length === 0 ? "Draft cut sketches before rendering." : null,
    busy: input.busy,
    error: input.error,
    blocked: blockedCuts ? [`${blockedCuts} shot package${blockedCuts === 1 ? "" : "s"} need review`, ...input.blocked] : input.blocked,
    actions: [
      action("render", "Render new take", !input.busy, input.busy),
      action("alternate", "New alternate angle", !input.busy, input.busy),
    ],
  });
}

export function buildTakeReviewModel(input: {
  scene: SceneRecord;
  busy: string | null;
  error: string | null;
  blocked: string[];
}): SurfacePresentation {
  const count = input.scene.cuts.reduce((total, cut) => total + (cut.takes?.length ?? 0), 0);
  return chrome({
    status: count ? "ready" : "empty",
    tone: count ? "ready" : "empty",
    banner:
      "A generated clip is a take until you select it. Changing a selected take unbinds dependent continuity frames. It does not approve a replacement frame.",
    emptyTitle: count ? null : "No takes to review.",
    emptyDetail: count
      ? null
      : "Generate clips under Scenes/SceneN/CutN, then refresh this stage. The importer registers video files as unreviewed takes.",
    busy: input.busy,
    error: input.error,
    blocked: input.blocked,
    actions: [
      action("alternate", "New alternate angle", !input.busy, input.busy),
      action("refresh", "Refresh takes from project", !input.busy, input.busy),
    ],
  });
}

export function buildAssembleModel(input: {
  scene: SceneRecord;
  busy: string | null;
  error: string | null;
  blocked: string[];
  reviewPath: string | null;
}): SurfacePresentation {
  const missing = input.scene.cuts.filter((cut) => {
    const take = (cut.takes ?? []).find(
      (candidate) => candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    return !take;
  });
  const selected = input.scene.cuts.length - missing.length;
  const wait = input.busy;
  const noCuts = input.scene.cuts.length === 0;
  return chrome({
    status: noCuts ? "empty" : missing.length ? "blocked" : "ready",
    tone: noCuts ? "empty" : missing.length ? "blocked" : "ready",
    banner: missing.length
      ? `Select one take for every cut before assembly. Missing: ${missing.map((cut) => `Cut ${cut.id.cut}`).join(", ")}.`
      : `${selected} selected takes are ready to assemble in cut order.`,
    emptyTitle: noCuts ? "This scene has no cuts." : null,
    emptyDetail: noCuts ? "Draft cut sketches before assembling." : null,
    busy: input.busy,
    error: input.error,
    blocked: missing.length ? missing.map((cut) => `Cut ${cut.id.cut}`) : input.blocked,
    actions: [
      action(
        "assemble",
        "Prepare assembly",
        !wait && !missing.length && !noCuts,
        wait ?? (noCuts ? "This scene has no cuts." : missing.length ? "Select a take for every cut first." : null),
      ),
      action("open-review", "Open review export", !wait && Boolean(input.reviewPath), wait ?? (input.reviewPath ? null : "No review export yet.")),
      action("mark-version", "Mark selects as cut version", !wait && !missing.length && !noCuts, wait ?? (missing.length ? "Select a take for every cut first." : null)),
    ],
  });
}

export function buildProfileModel(input: {
  projectName: string | null;
  recipeLabel: string | null;
  summary: string;
  requirementCount: number;
  busy: string | null;
  error: string | null;
}): SurfacePresentation {
  return chrome({
    status: input.requirementCount ? "ready" : "empty",
    tone: input.requirementCount ? "ready" : "empty",
    banner: input.summary || input.projectName || "Untitled",
    emptyTitle: input.requirementCount ? null : "No requirements yet.",
    emptyDetail: input.requirementCount ? null : "Add a location, prop, voice, or other requirement for this project.",
    busy: input.busy,
    error: input.error,
    actions: [
      action("save-profile", "Save profile", !input.busy, input.busy),
      action("add-requirement", "Add requirement", !input.busy, input.busy),
    ],
  });
}

export function buildCharactersModel(input: {
  characters: CharacterRecord[];
  creating: boolean;
  busy: string | null;
  error: string | null;
}): SurfacePresentation {
  const count = input.characters.length;
  const identity = input.characters.filter((character) => character.identity_ready).length;
  const voice = input.characters.filter((character) => character.voice_ready).length;
  return chrome({
    status: count ? "ready" : "empty",
    tone: count || input.creating ? "ready" : "empty",
    banner: count
      ? `${count} character${count === 1 ? "" : "s"} · ${identity} identity-ready · ${voice} voice-ready`
      : "No characters discovered.",
    emptyTitle: count || input.creating ? null : "No characters discovered.",
    emptyDetail:
      count || input.creating
        ? null
        : "Create a profile here. Identity model sheets and voice assets will remain in that character’s folder.",
    busy: input.busy,
    error: input.error,
    actions: [
      action("create", input.creating ? "Cancel new character" : "New character", !input.busy, input.busy),
      action("save", "Save profile", !input.busy, input.busy),
      action("create-character", "Create character", !input.busy, input.busy),
    ],
  });
}

export function buildScriptModel(input: { scene: SceneRecord }): SurfacePresentation {
  const beats =
    (input.scene.location ? 1 : 0) +
    (input.scene.action ?? []).length +
    (input.scene.sound ?? []).length +
    (input.scene.dialogue ?? []).length +
    (input.scene.transitions ?? []).length;
  const hasScript = beats > 0 || Boolean(input.scene.source_excerpt);
  return chrome({
    status: hasScript ? "present" : "empty",
    tone: hasScript ? "ready" : "empty",
    banner: null,
    emptyTitle: hasScript ? null : "No script beats found for this scene.",
    emptyDetail: null,
    busy: null,
    error: null,
  });
}
