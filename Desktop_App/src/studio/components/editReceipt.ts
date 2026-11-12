import type { CutRecord, ProjectState } from "../../workflowTypes";
import { effectiveVisualSourceStrategy } from "../surfaces/surfaceSelectors";

/** Mirror the host source resolution: an explicit binding wins over retained history. */
function currentImage(cut: CutRecord, state: ProjectState): string {
  const source = cut.visual_source;
  const strategy = effectiveVisualSourceStrategy(cut);
  if (!strategy) return "not selected";
  if (strategy === "no_start_frame") return "no start frame";
  const label = strategy === "shared_reference" ? "shared reference"
    : strategy === "previous_selected_take_exit_frame"
      ? `predecessor ${source?.predecessor ? `Scene ${source.predecessor.source_scene} Cut ${source.predecessor.cut}` : "not set"}`
      : strategy === "deferred_manual" ? "manual source" : "start frame";
  if (source?.asset_id) {
    const asset = state.assets?.find(candidate => candidate.id === source.asset_id);
    if (!asset) return `${label} missing asset ${source.asset_id}`;
    return `${label}${strategy === "previous_selected_take_exit_frame" ? " ·" : ""} ${asset.status === "approved" ? "" : `stale `}${asset.id} · ${asset.path}`;
  }
  if (strategy === "preproduction_start_frame" && cut.approved_start_frame) {
    return `${label} ${cut.approved_start_frame}`;
  }
  return `${label} · ${strategy === "previous_selected_take_exit_frame" ? "awaiting selected exit frame" : "not bound"}`;
}

/** Project-owned edit receipt, derived from persisted host state on every render/reopen. */
export function renderEditReceipt(state: ProjectState | null): HTMLElement | null {
  if (!state) return null;
  const impacts = state.change_impacts ?? [];
  const latest = impacts.length ? Math.max(...impacts.map(edge => edge.revision)) : null;
  const edges = impacts.filter(edge => edge.revision === latest);
  const pending = state.scenes.flatMap(scene => scene.cuts
    .filter(cut => cut.package_needs_review || cut.dependency_state === "needs_review")
    .map(cut => ({ scene, cut })));
  // An edit to a script without cuts still has a persisted revision. Show the
  // current source on reopen rather than relying on a transient save toast.
  if (latest === null && !pending.length && !state.scripts?.length) return null;

  const receipt = document.createElement("section");
  receipt.className = "workflow-edit-receipt";
  receipt.setAttribute("aria-label", "Current project edit receipt");
  const heading = document.createElement("h2");
  heading.textContent = `Current project state · revision ${state.state_revision ?? "unknown"}`;
  receipt.append(heading);
  const notice = document.createElement("p");
  notice.textContent = pending.length
    ? `${pending.length} cut${pending.length === 1 ? "" : "s"} need review. Next: review affected cut sketches, production notes and selected media, then prepare a new render and reassemble. Old prepared jobs or masters are not current approvals.`
    : edges.length
      ? "No cuts currently marked for review. Check the latest edit and its invalidated approvals before preparing new work."
      : "No dependent cuts are marked for review. Continue in Script or plan the next scene.";
  receipt.append(notice);

  const scripts = document.createElement("p");
  scripts.textContent = `Current registered script${(state.scripts?.length ?? 0) === 1 ? "" : "s"}: ${state.scripts?.map(script => script.path).join(", ") || "none"}. Workflow revision ${state.state_revision ?? "unknown"}; scripts have no separate version ledger.`;
  receipt.append(scripts);

  const current = document.createElement("ul");
  const affected = new Set(edges.filter(edge => edge.dependent.startsWith("shot:")).map(edge => edge.dependent.slice(5)));
  const shown = state.scenes.flatMap(scene => scene.cuts
    .filter(cut => pending.some(item => item.cut === cut) || (cut.stable_id && affected.has(cut.stable_id)))
    .map(cut => ({scene, cut})));
  for (const {scene, cut} of shown) {
    const line = document.createElement("li");
    const selectedTake = cut.takes?.find(take => take.id === cut.selected_take || take.path === cut.selected_take);
    const selectedAudio = state.assets?.find(asset => asset.id === cut.selected_performance_asset_id);
    const needsReview = cut.package_needs_review || cut.dependency_state === "needs_review";
    line.textContent = `Scene ${scene.id} Cut ${cut.id.cut}: script ${scene.source_path ?? "not linked"}; plan ${cut.production_note_path ?? "not saved"}; image ${currentImage(cut, state)}; audio ${selectedAudio?.path ?? cut.selected_performance_asset_id ?? "not selected"}; video ${selectedTake?.path ?? cut.selected_take ?? "not selected"}. ${needsReview ? "Review needed." : "No current cut review flag."}`;
    current.append(line);
  }
  if (shown.length) receipt.append(current);

  if (edges.length) {
    const detail = document.createElement("details");
    const summary = document.createElement("summary");
    summary.textContent = `Latest edit revision ${latest}: ${edges.filter(edge => edge.disposition === "needs_review").length} review edges, ${edges.filter(edge => edge.disposition === "invalidated").length} invalidated approvals/jobs`;
    detail.append(summary);
    const list = document.createElement("ul");
    for (const edge of edges) {
      const line = document.createElement("li");
      line.textContent = `${edge.source} → ${edge.dependent}: ${edge.disposition === "needs_review" ? "review needed" : "invalidated (historical only)"}`;
      list.append(line);
    }
    detail.append(list);
    receipt.append(detail);
  }
  const previous = impacts.filter(edge => edge.revision !== latest).length;
  const oldJobs = (state.jobs ?? []).filter(job => job.status === "completed" || job.status === "cancelled").length;
  const history = document.createElement("p");
  history.className = "muted";
  history.textContent = `History retained: ${previous} earlier edit edges, ${oldJobs} completed/cancelled jobs, ${(state.versions ?? []).length} saved media versions. Historical results are not evidence of current approval; unaffected choices remain intact.`;
  receipt.append(history);
  if (state.versions?.length) {
    const saved = document.createElement("details");
    const label = document.createElement("summary");
    label.textContent = "Retained media versions (inspect by asset ID, not current selection)";
    saved.append(label);
    const list = document.createElement("ul");
    for (const version of state.versions) {
      const line = document.createElement("li");
      line.textContent = `${version.asset_id} · v${version.version} · ${version.path} · recorded ${version.status}`;
      list.append(line);
    }
    saved.append(list);
    receipt.append(saved);
  }
  return receipt;
}
