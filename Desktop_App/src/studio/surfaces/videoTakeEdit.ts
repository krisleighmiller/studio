import type { AnimaticShot, TakeRecord } from "../../workflowTypes";
import type { AssembleIntent } from "./surfaceIntents";
import { getSurfaceHost } from "./session";
import { actionButton, block } from "./shared";

/** A source-bound editorial window. It does not create or alter a paid MP4. */
export function renderVideoTakeEdit(shot: AnimaticShot, take: TakeRecord, emit: (intent: AssembleIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const section = document.createElement("section");
  section.setAttribute("aria-label", `Edit selected video take ${take.id}`);
  section.append(block("workflow-field-hint", `Local video edit · source take ${take.id} · ${take.path}. The original stays untouched. The saved window belongs to this exact take; changing takes requires a fresh review.`));
  if (shot.trim_source_take_id && shot.trim_source_take_id !== take.id) {
    section.append(block("workflow-missing", `Saved edit belongs to ${shot.trim_source_take_id}, not selected take ${take.id}. Preview this source and save a new window before assembly.`));
  }
  const before = document.createElement("video");
  before.controls = true;
  before.preload = "metadata";
  before.setAttribute("aria-label", `Original take ${take.id}`);
  const after = document.createElement("video");
  after.controls = true;
  after.preload = "metadata";
  after.setAttribute("aria-label", `Edited window of ${take.id}`);
  const head = document.createElement("input");
  head.type = "number"; head.min = "0"; head.step = "0.01";
  head.value = String(shot.trim_in_seconds ?? 0);
  head.setAttribute("aria-label", `Source in seconds for ${take.id}`);
  const content = document.createElement("input");
  content.type = "number"; content.min = "0.01"; content.step = "0.01";
  content.value = String(Math.max(0, Math.round((shot.duration_seconds - (shot.trim_in_seconds ?? 0) - (shot.trim_out_seconds ?? 0)) * 100) / 100));
  content.setAttribute("aria-label", `Content duration seconds for ${take.id}`);
  const tail = document.createElement("input");
  tail.type = "number"; tail.min = "0"; tail.step = "0.01";
  tail.value = "0";
  tail.setAttribute("aria-label", `Extra video tail seconds for ${take.id}`);
  const field = (label: string, input: HTMLElement) => {
    const wrapper = document.createElement("label"); wrapper.textContent = label; wrapper.append(input); return wrapper;
  };
  let sourceDuration: number | null = null;
  const status = block("workflow-field-hint", "Reading selected source duration…");
  const save = actionButton("Save selected-take edit", () => {
    if (sourceDuration == null || !valid()) return;
    emit({ type: "set-shot-trim", shotId: shot.id, trimInSeconds: Number(head.value), trimOutSeconds: 0, sourceEndSeconds: end(), expectedTakeId: take.id });
  });
  const end = () => Math.round((Number(head.value) + Number(content.value) + Number(tail.value)) * 100) / 100;
  const valid = () => {
    const values = [head, content, tail].map(input => Number(input.value));
    const [start, length, extra] = values;
    return [head, content, tail].every(input => input.value.trim() !== "")
      && values.every(value => Number.isFinite(value) && Math.abs(value * 100 - Math.round(value * 100)) < 0.000001)
      && start >= 0 && length > 0 && extra >= 0 && end() >= 0.4 && end() <= 120
      && sourceDuration != null && end() <= sourceDuration + 0.001;
  };
  const refresh = () => {
    save.disabled = Boolean(host.busy) || !valid();
    status.textContent = sourceDuration == null ? "Source duration unavailable; no edit can be saved." : valid()
      ? `Source ${sourceDuration.toFixed(2)}s · edited window ${Number(head.value).toFixed(2)}s to ${end().toFixed(2)}s (${(end() - Number(head.value)).toFixed(2)}s). Save, then prepare a new master to review.`
      : `Window must fit the measured ${sourceDuration.toFixed(2)}s source. Use a positive content duration and non-negative source in and extra tail (hundredths of a second).`;
  };
  const preview = () => {
    if (!valid()) return;
    after.currentTime = Number(head.value);
    void after.play().catch(() => undefined);
  };
  const enforceWindow = () => {
    if (!valid()) { after.pause(); return; }
    if (after.currentTime < Number(head.value)) after.currentTime = Number(head.value);
    if (after.currentTime >= end()) {
      after.pause();
      if (after.currentTime > end()) after.currentTime = end();
    }
  };
  after.addEventListener("timeupdate", enforceWindow);
  after.addEventListener("seeked", enforceWindow);
  after.addEventListener("play", enforceWindow);
  head.addEventListener("input", refresh); content.addEventListener("input", refresh); tail.addEventListener("input", refresh);
  before.addEventListener("loadedmetadata", () => {
    sourceDuration = Number.isFinite(before.duration) && before.duration > 0 ? before.duration : null;
    refresh();
  });
  before.addEventListener("error", refresh);
  section.append(field("Original selected take", before), field("Edited window preview", after), field("Source in (seconds)", head), field("Content duration (seconds)", content), field("Extra video tail (seconds)", tail), actionButton("Play edited window", preview), save, status);
  save.disabled = true;
  void host.mediaUrl(host.resolvePath(take.path) ?? take.path).then(url => {
    if (!section.isConnected || getSurfaceHost().state !== host.state) return;
    before.src = url; after.src = url;
  }).catch(() => { if (section.isConnected) status.textContent = "Could not open selected take for preview."; });
  return section;
}
