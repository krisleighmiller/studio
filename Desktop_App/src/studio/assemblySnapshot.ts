import type { CutVersion, SceneRecord } from "../workflowTypes";

/** A historical pass certifies only the exact ordered cut/take/window snapshot. */
export function assemblySnapshotMatches(scene: SceneRecord, version: CutVersion): boolean {
  const saved = version.assembly_snapshot;
  if (!version.assembly_path?.trim() || !saved?.length || !scene.cuts.length || saved.length !== scene.cuts.length || version.shots.length !== scene.cuts.length) return false;
  const keys = scene.cuts.map(c => `${c.id.source_scene}:${c.id.cut}`);
  if (new Set(keys).size !== keys.length) return false;
  const shots = scene.animatic?.shots;
  if (scene.animatic && (!shots?.length || shots.length !== scene.cuts.length)) return false;
  const ordered = shots ? shots.map(a => scene.cuts.find(c => c.id.source_scene === a.cut_id.source_scene && c.id.cut === a.cut_id.cut)) : [...scene.cuts].sort((a,b) => a.id.cut - b.id.cut);
  if (new Set(ordered).size !== scene.cuts.length || ordered.some(c => !c)) return false;
  return ordered.every((cut, index) => {
    if (!cut) return false;
    const take = (cut.takes ?? []).find(t => t.id === cut.selected_take || t.path === cut.selected_take);
    const record = saved[index];
    const claims = version.shots.filter(s => cut.stable_id ? s.shot_id === cut.stable_id : s.cut_label === cut.id.cut);
    if (!take || claims.length !== 1 || !record || record.shot_id !== cut.stable_id || record.cut_label !== cut.id.cut || record.take_id !== take.id || record.path !== take.path || claims[0].take_id !== take.id || claims[0].path !== take.path) return false;
    if (!Number.isFinite(record.start_seconds) || record.start_seconds < 0 || !Number.isFinite(record.duration_seconds) || record.duration_seconds <= 0 || !["cut","dissolve","fade_through_black"].includes(record.transition)) return false;
    const anim = shots?.[index];
    if (!anim) return record.start_seconds === 0 && record.transition === "cut";
    const head = anim.trim_in_seconds ?? 0, tail = anim.trim_out_seconds ?? 0;
    return Number.isFinite(head) && Number.isFinite(tail) && head >= 0 && tail >= 0 && Number.isFinite(anim.duration_seconds) && anim.duration_seconds > head + tail && close(record.start_seconds, head) && close(record.duration_seconds, anim.duration_seconds - head - tail) && record.transition === (anim.transition ?? "cut");
  });
}
function close(a: number, b: number): boolean { return Math.abs(a-b) < 0.000001; }
