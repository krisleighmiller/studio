import {
  LOOK_ROLES,
  type AssetRecord,
  type JobRecord,
  type LookSlot,
  type ProjectState,
  type SceneRecord,
  type SceneReport,
} from "../workflowTypes";
import { buildAudioStatus, describeAudioStatus } from "./selectors/audio";

export type SceneListItemModel = {
  key: string;
  title: string;
  numberLabel: string;
  thumbnailPath: string | null;
  readiness: string;
  warning: boolean;
  active: boolean;
  audio: string | null;
};

export type LookVersionModel = {
  assetId: string;
  path: string | null;
  statusLabel: string;
  approved: boolean;
  rejected: boolean;
};

export type LookSlotModel = {
  id: string;
  role: string;
  title: string;
  prompt: string;
  negativePrompt: string;
  approvalLabel: string;
};

export type PlateModel = {
  assetId: string;
  path: string | null;
  label: string;
};

export type SceneLookModel = {
  title: string;
  counts: string;
  warnings: string[];
  blockers: string[];
  narrativePurpose: string;
  notes: string[];
  slots: LookSlotModel[];
  selectedSlotId: string | null;
  versions: LookVersionModel[];
  selectedAssetId: string | null;
  primary: LookVersionModel | null;
  identityHint: string | null;
  failedJobs: { id: string; message: string }[];
  showPlates: boolean;
  plateNote: string;
  plates: PlateModel[];
  canBuildPlates: boolean;
  directorAttached: boolean;
  busyReason: string | null;
  emptySlots: boolean;
};

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "webp", "gif", "bmp"]);

export function sceneSelectionKey(scene: Pick<SceneRecord, "id" | "stable_id">): string {
  const stable = scene.stable_id?.trim();
  return stable || `scene:${scene.id}`;
}

export function buildSceneList(input: {
  scenes: SceneRecord[];
  reports: SceneReport[];
  assets: AssetRecord[];
  selectedKey: string | null;
  blockerCounts: Map<number, number>;
  resolvePath: (path: string | null | undefined) => string | null;
  state: ProjectState | null;
}): SceneListItemModel[] {
  return input.scenes.map((scene) => {
    const sceneReport = input.reports.find((item) => item.id === scene.id);
    const blockers = input.blockerCounts.get(scene.id) ?? 0;
    const warningCount = (sceneReport?.warnings.length ?? 0) + blockers;
    return {
      key: sceneSelectionKey(scene),
      title: scene.title,
      numberLabel: `Scene ${scene.id}`,
      thumbnailPath: sceneThumbnail(scene, input.assets, input.resolvePath),
      readiness: readinessLabel(scene, sceneReport, blockers),
      warning: warningCount > 0 || Boolean(sceneReport?.effectively_performance_heavy),
      active: sceneSelectionKey(scene) === input.selectedKey,
      audio: input.state
        ? describeAudioStatus(buildAudioStatus(input.state, scene.id))
        : null,
    };
  });
}

export function buildSceneLook(input: {
  scene: SceneRecord;
  report: SceneReport | undefined;
  assets: AssetRecord[];
  jobs: JobRecord[];
  characters: { id: string; name: string }[];
  blockers: string[];
  focusedSlotId: string | null;
  focusedAssetId: string | null;
  directorSlotId: string | null;
  identityHint: string | null;
  busy: string | null;
  resolvePath: (path: string | null | undefined) => string | null;
}): SceneLookModel {
  const slots = input.scene.look_board?.slots ?? [];
  const selected = pickSlot(slots, input.focusedSlotId, input.directorSlotId);
  const versions = selected ? lookVersions(selected, input.assets, input.resolvePath) : [];
  const primary = pickPrimary(versions, input.focusedAssetId);
  const plates = wardrobePlates(input.scene, input.assets, input.characters, input.resolvePath);
  return {
    title: `Scene ${input.scene.id} — ${input.scene.title}`,
    counts: sceneCounts(input.scene, input.report, input.blockers.length),
    warnings: input.report?.warnings ?? [],
    blockers: input.blockers,
    narrativePurpose: input.scene.intent?.narrative_purpose?.trim() || "No narrative purpose yet.",
    notes: input.scene.look_board?.notes ?? [],
    slots: slots.map((slot) => ({
      id: slot.id,
      role: knownRole(slot.role),
      title: slot.title ?? "",
      prompt: slot.prompt ?? "",
      negativePrompt: slot.negative_prompt ?? "",
      approvalLabel: approvalLabel(slot),
    })),
    selectedSlotId: selected?.id ?? null,
    versions,
    selectedAssetId: primary?.assetId ?? null,
    primary,
    identityHint: input.identityHint,
    failedJobs: failedJobs(input.jobs, selected?.id ?? null, input.scene.stable_id ?? null),
    showPlates: selected?.role === "wardrobe_reference",
    plateNote: plates.length
      ? "These plates replace bare model sheets for later stills. Build wardrobe plates sends every missing close-up and standing plate in one approval."
      : "One approval builds a close-up and a standing plate for each named character.",
    plates,
    canBuildPlates: Boolean(selected && selected.role === "wardrobe_reference" && approvedAsset(selected, input.assets)),
    directorAttached: Boolean(selected && input.directorSlotId === selected.id),
    busyReason: input.busy ? "Wait for the current action to finish." : null,
    emptySlots: slots.length === 0,
  };
}

function sceneThumbnail(
  scene: SceneRecord,
  assets: AssetRecord[],
  resolvePath: (path: string | null | undefined) => string | null,
): string | null {
  for (const slot of scene.look_board?.slots ?? []) {
    const approved = approvedAsset(slot, assets);
    const path = resolvePath(approved?.path);
    if (path) return path;
  }
  for (const cut of scene.cuts) {
    const start = resolvePath(cut.approved_start_frame);
    if (start) return start;
    const sourceId = cut.visual_source?.asset_id;
    if (!sourceId) continue;
    const asset = assets.find((item) => item.id === sourceId);
    if (asset?.status === "approved" && isImagePath(asset.path)) {
      const path = resolvePath(asset.path);
      if (path) return path;
    }
  }
  return null;
}

function readinessLabel(scene: SceneRecord, report: SceneReport | undefined, blockers: number): string {
  if (blockers > 0) return "Blocked";
  const warnings = report?.warnings.length ?? 0;
  if (warnings > 0) return `${warnings} warning${warnings === 1 ? "" : "s"}`;
  const slots = scene.look_board?.slots ?? [];
  const approved = slots.filter((slot) => slot.approved_asset_id).length;
  if (slots.length > 0 && approved === slots.length) return "Look ready";
  if (approved > 0) return `${approved}/${slots.length} look`;
  if (scene.intent_approved) return "Intent approved";
  return "Needs look";
}

function sceneCounts(scene: SceneRecord, report: SceneReport | undefined, blockers: number): string {
  const intent = scene.intent_approved ? "Intent approved" : scene.intent ? "Intent draft" : "Intent missing";
  const slots = scene.look_board?.slots ?? [];
  const approved = slots.filter((slot) => slot.approved_asset_id).length;
  const look = slots.length ? `Look ${approved}/${slots.length}` : "Look missing";
  const parts = [intent, look, `${scene.cuts.length} cut${scene.cuts.length === 1 ? "" : "s"}`];
  if (report && report.planned_visual_sources > 0) {
    parts.push(`${report.render_ready_sources}/${report.planned_visual_sources} references`);
  }
  const warnings = (report?.warnings.length ?? 0) + blockers;
  if (warnings > 0) parts.push(`${warnings} warning${warnings === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

function pickSlot(slots: LookSlot[], focusedId: string | null, directorId: string | null): LookSlot | undefined {
  return (
    slots.find((slot) => slot.id === focusedId) ??
    slots.find((slot) => slot.id === directorId) ??
    slots[0]
  );
}

function lookVersions(
  slot: LookSlot,
  assets: AssetRecord[],
  resolvePath: (path: string | null | undefined) => string | null,
): LookVersionModel[] {
  const versions: LookVersionModel[] = [];
  for (const assetId of slot.asset_ids ?? []) {
    const asset = findLookAsset(slot, assetId, assets);
    if (!asset) continue;
    const approved = slot.approved_asset_id === assetId;
    const rejected = asset.status === "rejected";
    versions.push({
      assetId,
      path: resolvePath(asset.path),
      statusLabel: `${asset.status ?? "draft"}${approved ? " · current" : ""}`,
      approved,
      rejected,
    });
  }
  return versions;
}

function pickPrimary(versions: LookVersionModel[], focusedAssetId: string | null): LookVersionModel | null {
  return (
    versions.find((version) => version.assetId === focusedAssetId) ??
    versions.find((version) => version.approved) ??
    [...versions].reverse().find((version) => !version.rejected) ??
    versions[versions.length - 1] ??
    null
  );
}

function failedJobs(jobs: JobRecord[], slotId: string | null, sceneStableId: string | null): { id: string; message: string }[] {
  return jobs
    .filter((job) => {
      if (job.kind !== "scene_look" && job.kind !== "wardrobe_plate") return false;
      const status = (job.status ?? "").toLowerCase();
      if (status !== "failed" && status !== "cancelled") return false;
      if (job.kind === "scene_look" && job.raw_provider_id && slotId && job.raw_provider_id !== slotId) return false;
      if (job.scope?.id && sceneStableId && job.scope.id !== sceneStableId && job.scope.id !== slotId) return false;
      return true;
    })
    .map((job) => ({
      id: job.id,
      message: `${job.kind.replace(/_/g, " ")} ${job.status ?? "status unknown"}: ${job.error?.trim() || "no error recorded"}`,
    }));
}

function wardrobePlates(
  scene: SceneRecord,
  assets: AssetRecord[],
  characters: { id: string; name: string }[],
  resolvePath: (path: string | null | undefined) => string | null,
): PlateModel[] {
  return assets
    .filter(
      (asset) =>
        asset.role === "wardrobe_plate" &&
        asset.scene_id === scene.id &&
        asset.status !== "rejected",
    )
    .map((asset) => {
      const character = characters.find((item) => item.id === asset.character_id);
      return {
        assetId: asset.id,
        path: resolvePath(asset.path),
        label: `${character?.name ?? "Character"} · ${(asset.notes ?? "plate").replace(/_/g, " ")}`,
      };
    });
}

function approvalLabel(slot: LookSlot): string {
  if (slot.approved_asset_id) return "Approved";
  if ((slot.asset_ids?.length ?? 0) > 0) return "Draft still";
  return "Prompt only";
}

function knownRole(role: string): string {
  return LOOK_ROLES.some((item) => item.id === role) ? role : "location_reference";
}

function approvedAsset(slot: LookSlot, assets: AssetRecord[]): AssetRecord | undefined {
  if (slot.approved_asset_id) {
    const chosen = findLookAsset(slot, slot.approved_asset_id, assets);
    if (chosen?.status === "approved" && chosen.role === slot.role) return chosen;
  }
  for (const id of [...(slot.asset_ids ?? [])].reverse()) {
    const asset = findLookAsset(slot, id, assets);
    if (asset?.status === "approved" && asset.role === slot.role) return asset;
  }
  return undefined;
}

function findLookAsset(slot: LookSlot, id: string, assets: AssetRecord[]): AssetRecord | undefined {
  const matches = assets.filter((asset) => asset.id === id);
  return matches.find((asset) => asset.role === slot.role) ?? matches[0];
}

function isImagePath(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.has(ext);
}
