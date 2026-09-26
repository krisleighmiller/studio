import type { AssetRecord, JobRecord, SceneRecord, StageId } from "../workflowTypes";

export type JobFilter = "active" | "failed" | "awaiting" | "completed";

export type JobOutputKind = "image" | "video" | "audio" | "review" | "file";

export type JobRowModel = {
  id: string;
  title: string;
  bucket: JobFilter | "unknown";
  statusLabel: string;
  elapsed: string;
  created: string;
  started: string;
  completed: string;
  scope: string;
  progress: string;
  estimate: string;
  actual: string;
  currency: string;
  retry: string;
  detail: string;
  outputPath: string | null;
  outputKind: JobOutputKind | null;
  actionLabel: string | null;
  sceneId: number | null;
  cut: number | null;
  lookSlotId: string | null;
  stage: StageId | null;
  tooltip: string;
};

export type JobsModel = {
  rows: JobRowModel[];
  counts: Record<JobFilter, number>;
  total: number;
  empty: string;
};

const KIND_LABELS: Record<string, string> = {
  scene_look: "Look",
  wardrobe_plate: "Wardrobe",
  render: "Render",
  assemble: "Assembly",
  cut_start_frame: "Start frame",
  start_frame: "Start frame",
};

const IMAGE = new Set(["png", "jpg", "jpeg", "webp", "gif", "bmp"]);
const VIDEO = new Set(["mp4", "mov", "webm", "mkv", "m4v"]);
const AUDIO = new Set(["wav", "mp3", "m4a", "aac", "ogg", "flac"]);

const APPROVED_TAKES = new Set(["selected", "used_in_assembly"]);

export function buildJobsModel(input: {
  jobs: JobRecord[];
  scenes: SceneRecord[];
  assets: AssetRecord[];
  filter: JobFilter | "all";
}): JobsModel {
  const rows = input.jobs.map((job) => jobRow(job, input.scenes, input.assets));
  const counts: Record<JobFilter, number> = {
    active: 0,
    failed: 0,
    awaiting: 0,
    completed: 0,
  };
  for (const row of rows) {
    if (row.bucket !== "unknown") counts[row.bucket] += 1;
  }
  const visible = input.filter === "all" ? rows : rows.filter((row) => row.bucket === input.filter);
  return {
    rows: visible,
    counts,
    total: rows.length,
    empty: visible.length
      ? ""
      : input.jobs.length
        ? "No jobs in this filter."
        : "No provider jobs recorded yet.",
  };
}

export function elapsedLabel(job: {
  created_at?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  status?: string;
}): string {
  const start = job.started_at || job.created_at;
  if (!start) return "unknown";
  const running = job.status === "pending" || job.status === "running";
  const end = job.completed_at || (running ? new Date().toISOString() : null);
  if (!end) return "unknown";
  const seconds = (Date.parse(end) - Date.parse(start)) / 1000;
  if (!Number.isFinite(seconds) || seconds < 0) return "unknown";
  const whole = Math.floor(seconds);
  if (whole < 60) return `${whole}s`;
  if (whole < 3600) return `${Math.floor(whole / 60)}m`;
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  return minutes ? `${hours}h ${minutes}m` : `${hours}h`;
}

function jobRow(job: JobRecord, scenes: SceneRecord[], assets: AssetRecord[]): JobRowModel {
  const located = locateJob(job, scenes, assets);
  const bucket = jobBucket(job, located.approval);
  const outputPath = outputOf(job, assets);
  const outputKind = outputPath ? outputKindOf(job.kind, outputPath) : null;
  const elapsed = elapsedLabel(job);
  const detail = job.error?.trim() || located.label || job.progress?.trim() || "Scope unknown";
  const created = knownText(job.created_at);
  const started = knownText(job.started_at);
  const completed = knownText(job.completed_at);
  const scope = job.scope?.kind && job.scope.id ? `${job.scope.kind} ${job.scope.id}` : "unknown";
  const progress = knownText(job.progress);
  const estimate = money(job.estimated_cost);
  const actual = money(job.cost);
  const currency = knownText(job.currency);
  const retry = knownText(job.retry_of);
  const cost = `estimate ${estimate} · actual ${actual} · currency ${currency}`;
  return {
    id: job.id,
    title: KIND_LABELS[job.kind] ?? labelize(job.kind || "Job"),
    bucket,
    statusLabel: statusLabel(job.status, bucket),
    elapsed,
    created,
    started,
    completed,
    scope,
    progress,
    estimate,
    actual,
    currency,
    retry,
    detail,
    outputPath,
    outputKind,
    actionLabel: actionLabel(outputKind, Boolean(located.scene)),
    sceneId: located.scene?.id ?? null,
    cut: located.cut,
    lookSlotId: located.lookSlotId,
    stage: stageFor(job.kind),
    tooltip: `${job.kind} · ${job.status ?? "unknown"} · ${elapsed} · ${cost} · ${detail}`,
  };
}

function jobBucket(job: JobRecord, approval: "approved" | "pending" | "unknown"): JobFilter | "unknown" {
  const status = job.status ?? "unknown";
  if (status === "pending" || status === "running") return "active";
  if (status === "failed" || status === "cancelled") return "failed";
  if (status === "completed") return approval === "pending" ? "awaiting" : "completed";
  return "unknown";
}

function statusLabel(status: string | undefined, bucket: JobFilter | "unknown"): string {
  if (bucket === "awaiting") return "Awaiting approval";
  if (status === "pending") return "Pending";
  if (status === "running") return "Running";
  if (status === "completed") return "Complete";
  if (status === "failed") return "Failed";
  if (status === "cancelled") return "Cancelled";
  return "Unknown";
}

function actionLabel(kind: JobOutputKind | null, hasScene: boolean): string | null {
  if (kind === "review") return "Open review export";
  if (kind === "image") return "Open image";
  if (kind === "video") return "Open video";
  if (kind === "audio") return "Open audio";
  if (kind === "file") return "Open output";
  if (hasScene) return "Show in Studio";
  return null;
}

function stageFor(kind: string): StageId | null {
  if (kind === "scene_look" || kind === "wardrobe_plate") return "scene_look";
  if (kind === "cut_start_frame" || kind === "start_frame") return "reference_images";
  if (kind === "render") return "take_review";
  if (kind === "assemble") return "assemble";
  return null;
}

function outputOf(job: JobRecord, assets: AssetRecord[]): string | null {
  if (job.output_path?.trim()) return job.output_path;
  for (const id of job.output_asset_ids ?? []) {
    const path = assets.find((asset) => asset.id === id)?.path;
    if (path) return path;
  }
  return null;
}

function outputKindOf(kind: string, path: string): JobOutputKind {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  if (kind === "assemble" && VIDEO.has(extension)) return "review";
  if (IMAGE.has(extension)) return "image";
  if (VIDEO.has(extension)) return "video";
  if (AUDIO.has(extension)) return "audio";
  return "file";
}

type Located = {
  scene: SceneRecord | null;
  cut: number | null;
  lookSlotId: string | null;
  label: string;
  approval: "approved" | "pending" | "unknown";
};

function locateJob(job: JobRecord, scenes: SceneRecord[], assets: AssetRecord[]): Located {
  const scope = job.scope;
  let scene: SceneRecord | null = null;
  let cutNumber: number | null = null;
  let lookSlotId: string | null = null;
  if (scope?.kind === "scene" && scope.id) {
    scene = scenes.find((item) => item.stable_id === scope.id) ?? null;
  } else if (scope?.kind === "shot" && scope.id) {
    for (const item of scenes) {
      const cut = item.cuts.find((candidate) => candidate.stable_id === scope.id);
      if (cut) {
        scene = item;
        cutNumber = cut.id.cut;
        break;
      }
    }
  }
  const slotHit = findLookSlot(job, scenes);
  if (slotHit) {
    scene = scene ?? slotHit.scene;
    lookSlotId = slotHit.slotId;
  }
  if (scene && cutNumber == null) {
    const take = takeForOutput(scene, job, assets);
    if (take) cutNumber = take.cut;
  }
  if (!scene) {
    const asset = (job.output_asset_ids ?? [])
      .map((id) => assets.find((item) => item.id === id))
      .find((item) => item?.scene_id != null);
    if (asset?.scene_id != null) scene = scenes.find((item) => item.id === asset.scene_id) ?? null;
    if (asset?.cut_id) cutNumber = asset.cut_id.cut;
  }
  if (!scene) {
    const fromRaw = job.raw_provider_id?.match(/^scene_(\d+)$/);
    const fromPath = (job.output_path ?? "").match(/Scene(\d+)/i);
    const numeric = Number(fromRaw?.[1] ?? fromPath?.[1]);
    if (Number.isFinite(numeric)) scene = scenes.find((item) => item.id === numeric) ?? null;
  }
  const approval = approvalOf(job, scene, assets, lookSlotId);
  const label = scene
    ? cutNumber != null
      ? `Scene ${scene.id} · Cut ${cutNumber}`
      : `Scene ${scene.id}${scene.title.trim() ? ` ${scene.title.trim()}` : ""}`
    : "";
  return { scene, cut: cutNumber, lookSlotId, label, approval };
}

function findLookSlot(
  job: JobRecord,
  scenes: SceneRecord[],
): { scene: SceneRecord; slotId: string } | null {
  const raw = job.raw_provider_id?.trim();
  if (raw) {
    for (const scene of scenes) {
      const slot = scene.look_board?.slots?.find((item) => item.id === raw);
      if (slot) return { scene, slotId: slot.id };
    }
  }
  const outputs = new Set(job.output_asset_ids ?? []);
  if (!outputs.size) return null;
  for (const scene of scenes) {
    for (const slot of scene.look_board?.slots ?? []) {
      if ((slot.asset_ids ?? []).some((id) => outputs.has(id))) return { scene, slotId: slot.id };
      if (slot.approved_asset_id && outputs.has(slot.approved_asset_id)) return { scene, slotId: slot.id };
    }
  }
  return null;
}

function approvalOf(
  job: JobRecord,
  scene: SceneRecord | null,
  assets: AssetRecord[],
  lookSlotId: string | null,
): "approved" | "pending" | "unknown" {
  if (job.kind === "scene_look" || job.kind === "wardrobe_plate") {
    const slot = scene?.look_board?.slots?.find((item) => item.id === lookSlotId);
    if (!slot?.approved_asset_id) return slot ? "pending" : "unknown";
    const approved = assets.find((asset) => asset.id === slot.approved_asset_id);
    const outputs = job.output_asset_ids ?? [];
    if (outputs.includes(slot.approved_asset_id) || (approved?.path && approved.path === job.output_path)) {
      return "approved";
    }
    return "pending";
  }
  if (job.kind === "render" && scene) {
    const take = takeForOutput(scene, job, assets);
    if (!take) return "unknown";
    return APPROVED_TAKES.has(take.take.status) ? "approved" : "pending";
  }
  if (job.kind === "assemble" && scene) {
    const path = job.output_path;
    const version = (scene.post?.versions ?? []).find((item) => path && item.assembly_path === path);
    if (!version) return "unknown";
    return scene.post?.picture_locked_id === version.id ? "approved" : "pending";
  }
  return "unknown";
}

function takeForOutput(
  scene: SceneRecord,
  job: JobRecord,
  assets: AssetRecord[],
): { cut: number; take: { status: string } } | null {
  const paths = new Set<string>();
  if (job.output_path) paths.add(job.output_path);
  for (const id of job.output_asset_ids ?? []) {
    const path = assets.find((asset) => asset.id === id)?.path;
    if (path) paths.add(path);
  }
  for (const cut of scene.cuts) {
    for (const take of cut.takes ?? []) {
      if (paths.has(take.path)) return { cut: cut.id.cut, take };
    }
  }
  return null;
}

function knownText(value: string | null | undefined): string {
  const text = value?.trim();
  return text ? text : "unknown";
}

function money(value: number | null | undefined): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "unknown";
  return String(value);
}

function labelize(value: string): string {
  const text = value.replace(/_/g, " ").trim();
  if (!text) return "Job";
  return `${text.slice(0, 1).toUpperCase()}${text.slice(1)}`;
}
