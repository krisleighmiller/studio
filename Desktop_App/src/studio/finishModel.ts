import type { DeliverableRecord, JobRecord, SceneRecord } from "../workflowTypes";

export type FinishState = "complete" | "pending" | "blocked" | "stale" | "awaiting";

export type FinishItemModel = {
  id: string;
  label: string;
  state: FinishState;
  stateLabel: string;
  detail: string;
};

export type FinishModel = {
  sceneLabel: string;
  headline: string;
  items: FinishItemModel[];
  reviewExportPath: string | null;
  canAssemble: boolean;
  assembleReason: string | null;
  deliverables: { id: string; name: string }[];
  selectedDeliverableId: string | null;
  busyReason: string | null;
};

const POST_FIELDS: Array<{ id: string; label: string }> = [
  { id: "audio_mix", label: "Audio mix" },
  { id: "vfx", label: "VFX" },
  { id: "compositing", label: "Compositing" },
  { id: "captions", label: "Captions" },
  { id: "mastering", label: "Mastering" },
  { id: "qc", label: "QC" },
];

const POST_LABELS: Record<string, string> = {
  not_started: "not started",
  in_progress: "in progress",
  needs_review: "needs review",
  approved: "approved",
};

export function buildFinishModel(input: {
  scene: SceneRecord | null;
  jobs: JobRecord[];
  deliverables: DeliverableRecord[];
  busyReason: string | null;
}): FinishModel {
  if (!input.scene) {
    return {
      sceneLabel: "Finish",
      headline: "Nothing selected",
      items: [],
      reviewExportPath: null,
      canAssemble: false,
      assembleReason: "Select a scene first.",
      deliverables: [],
      selectedDeliverableId: null,
      busyReason: input.busyReason,
    };
  }
  const scene = input.scene;
  const missing = missingCuts(scene);
  const takesMissing = missing.length > 0 || scene.cuts.length === 0;
  const reviewExportPath = latestReviewExport(scene, input.jobs);
  const versions = scene.post?.versions ?? [];
  const latest = versions[versions.length - 1];
  const locked = versions.find((version) => version.id === scene.post?.picture_locked_id) ?? null;
  const items: FinishItemModel[] = [
    takesItem(scene, missing),
    ...staleItems(scene),
    ...pickupItems(scene),
    assemblyItem(scene, reviewExportPath, latest?.label ?? null, latest?.stage ?? null),
    pictureItem(locked?.label ?? null, Boolean(locked)),
    postItem(scene),
    qcItem(scene, input.deliverables),
  ];
  const gated = takesMissing
    ? items.map((item) =>
        item.state === "complete"
          ? { ...item, state: "pending" as const, stateLabel: "Pending" }
          : item,
      )
    : items;
  return {
    sceneLabel: `Scene ${scene.id}`,
    headline: headline(gated),
    items: gated,
    reviewExportPath,
    canAssemble: !input.busyReason && !takesMissing,
    assembleReason: input.busyReason
      ? input.busyReason
      : scene.cuts.length === 0
        ? "This scene has no cuts."
        : missing.length
          ? `Select a take for ${missing.map((cut) => `Cut ${cut}`).join(", ")}.`
          : null,
    deliverables: input.deliverables.map((item) => ({ id: item.id, name: item.name })),
    selectedDeliverableId:
      scene.post?.last_check?.deliverable_id ?? input.deliverables[0]?.id ?? null,
    busyReason: input.busyReason,
  };
}

export function latestReviewExport(scene: SceneRecord, jobs: JobRecord[]): string | null {
  const versions = scene.post?.versions ?? [];
  for (let index = versions.length - 1; index >= 0; index -= 1) {
    const path = versions[index]?.assembly_path;
    if (path) return path;
  }
  for (let index = jobs.length - 1; index >= 0; index -= 1) {
    const job = jobs[index];
    if (job?.kind !== "assemble" || job.status !== "completed" || !job.output_path) continue;
    const matches =
      job.scope?.id === scene.stable_id || job.raw_provider_id === `scene_${scene.id}`;
    if (matches) return job.output_path;
  }
  return null;
}

function missingCuts(scene: SceneRecord): number[] {
  const missing: number[] = [];
  for (const cut of scene.cuts) {
    const take = (cut.takes ?? []).find(
      (candidate) => candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    if (!take) missing.push(cut.id.cut);
  }
  return missing;
}

function takesItem(scene: SceneRecord, missing: number[]): FinishItemModel {
  if (!scene.cuts.length) {
    return item("takes", "Takes", "pending", "No cuts");
  }
  if (missing.length) {
    return item(
      "takes",
      "Takes",
      "blocked",
      `Missing ${missing.map((cut) => `Cut ${cut}`).join(", ")}`,
    );
  }
  const rerender = scene.cuts.filter((cut) => {
    const take = (cut.takes ?? []).find(
      (candidate) => candidate.id === cut.selected_take || candidate.path === cut.selected_take,
    );
    return take?.status === "needs_rerender" || take?.status === "needs_edit";
  });
  if (rerender.length) {
    return item(
      "takes",
      "Takes",
      "pending",
      rerender
        .map((cut) => {
          const take = (cut.takes ?? []).find(
            (candidate) => candidate.id === cut.selected_take || candidate.path === cut.selected_take,
          );
          return `Cut ${cut.id.cut} ${take?.status === "needs_edit" ? "needs edit" : "needs rerender"}`;
        })
        .join(", "),
    );
  }
  return item("takes", "Takes", "complete", `${scene.cuts.length}/${scene.cuts.length} selected`);
}

function staleItems(scene: SceneRecord): FinishItemModel[] {
  const lines = scene.cuts
    .filter((cut) => cut.dependency_state === "stale" || cut.dependency_state === "invalid" || cut.dependency_state === "needs_review")
    .map((cut) => `Cut ${cut.id.cut} ${cut.dependency_state?.replace(/_/g, " ")}`);
  if (!lines.length) return [];
  return [item("stale", "Dependencies", "stale", lines.join(", "))];
}

function pickupItems(scene: SceneRecord): FinishItemModel[] {
  const lines: string[] = [];
  for (const cut of scene.cuts) {
    if (cut.package_needs_review) lines.push(`Cut ${cut.id.cut} package needs review`);
    for (const take of cut.takes ?? []) {
      if (take.id === cut.selected_take || take.path === cut.selected_take) continue;
      if (take.status === "needs_rerender") lines.push(`Cut ${cut.id.cut} has a take that needs rerender`);
      if (take.status === "needs_edit") lines.push(`Cut ${cut.id.cut} has a take that needs edit`);
    }
  }
  if (!lines.length) return [];
  return [item("pickup", "Pickup", "awaiting", lines.join(", "))];
}

function assemblyItem(
  scene: SceneRecord,
  path: string | null,
  versionLabel: string | null,
  stage: string | null,
): FinishItemModel {
  if (!path) {
    return item(
      "assembly",
      "Review export",
      scene.cuts.length ? "pending" : "blocked",
      "No review export",
    );
  }
  const version = versionLabel ? `${versionLabel}${stage ? ` · ${stage.replace(/_/g, " ")}` : ""}` : "Review export ready";
  return item("assembly", "Review export", "complete", version);
}

function pictureItem(label: string | null, locked: boolean): FinishItemModel {
  if (!locked || !label) return item("picture", "Picture lock", "pending", "Not locked");
  return item("picture", "Picture lock", "complete", label);
}

function postItem(scene: SceneRecord): FinishItemModel {
  const post = scene.post;
  const unfinished = POST_FIELDS.filter((field) => postStatus(post, field.id) !== "approved");
  if (!unfinished.length) return item("post", "Post", "complete", "Approved");
  const awaiting = unfinished.some((field) => postStatus(post, field.id) === "needs_review");
  const detail = unfinished
    .map((field) => `${field.label} ${POST_LABELS[postStatus(post, field.id)] ?? "not started"}`)
    .join(" · ");
  return item("post", "Post", awaiting ? "awaiting" : "pending", detail);
}

function qcItem(scene: SceneRecord, deliverables: DeliverableRecord[]): FinishItemModel {
  if (!deliverables.length) return item("qc", "QC", "pending", "No deliverable to check");
  const check = scene.post?.last_check;
  if (!check) return item("qc", "QC", "pending", "Not checked");
  const name = deliverables.find((item) => item.id === check.deliverable_id)?.name ?? "Deliverable";
  const notes = (check.messages ?? []).filter(Boolean).join(" ");
  const mediaNotes = (check.media_findings ?? [])
    .filter((finding) => finding.severity !== "info")
    .map((finding) => finding.message)
    .join(" · ");
  const detail = [notes, mediaNotes].filter(Boolean).join(" · ");
  if (!check.passed) {
    return item("qc", "QC", "blocked", detail ? `${name} failed. ${detail}` : `${name} failed`);
  }
  const lockedId = scene.post?.picture_locked_id;
  if (lockedId && check.version_id && check.version_id !== lockedId) {
    return item("qc", "QC", "stale", `${name} passed an older cut version`);
  }
  return item("qc", "QC", "complete", detail ? `${name} passed. ${detail}` : `${name} passed`);
}

function postStatus(post: SceneRecord["post"], field: string): string {
  if (!post) return "not_started";
  switch (field) {
    case "audio_mix":
      return post.audio_mix ?? "not_started";
    case "vfx":
      return post.vfx ?? "not_started";
    case "compositing":
      return post.compositing ?? "not_started";
    case "captions":
      return post.captions ?? "not_started";
    case "mastering":
      return post.mastering ?? "not_started";
    case "qc":
      return post.qc ?? "not_started";
    default:
      return "not_started";
  }
}

function headline(items: FinishItemModel[]): string {
  if (!items.length) return "Pending";
  if (items.some((item) => item.state === "blocked")) return "Blocked";
  if (items.some((item) => item.state === "stale")) return "Stale";
  if (items.some((item) => item.state === "awaiting")) return "Awaiting approval";
  if (items.some((item) => item.state === "pending")) return "Pending";
  return "Complete";
}

function item(id: string, label: string, state: FinishState, detail: string): FinishItemModel {
  const stateLabel =
    state === "complete"
      ? "Complete"
      : state === "blocked"
        ? "Blocked"
        : state === "stale"
          ? "Stale"
          : state === "awaiting"
            ? "Awaiting"
            : "Pending";
  return { id, label, state, stateLabel, detail };
}
