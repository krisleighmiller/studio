export type ContinuityChoice = {
  showPreview: boolean;
  showTime: boolean;
  timeSeconds: number | null;
  bindEnabled: boolean;
  bindReason: string | null;
  confirmBeforeReplace: boolean;
  successorBlocked: boolean;
};

export function continuityChoice(input: {
  strategy: string | null | undefined;
  predecessorTakeSelected: boolean;
  durationSeconds: number | null;
  editedTimeSeconds: number | null;
  boundAssetId: string | null | undefined;
  dependentLabels: string[];
  busy: string | null;
}): ContinuityChoice {
  const continuity = input.strategy === "previous_selected_take_exit_frame";
  const nearEnd =
    input.durationSeconds != null && Number.isFinite(input.durationSeconds)
      ? Math.max(0, input.durationSeconds - 0.1)
      : null;
  const edited =
    input.editedTimeSeconds != null &&
    Number.isFinite(input.editedTimeSeconds) &&
    input.editedTimeSeconds >= 0
      ? input.editedTimeSeconds
      : null;
  const timeSeconds = continuity ? edited ?? nearEnd : null;
  let bindReason: string | null = null;
  if (!continuity) bindReason = "This cut does not use a predecessor take.";
  else if (input.busy) bindReason = input.busy;
  else if (!input.predecessorTakeSelected) {
    bindReason = "Select a take on the predecessor before binding a frame.";
  } else if (timeSeconds == null) {
    bindReason = "Preview the predecessor take so the frame time can default near the end.";
  }
  return {
    showPreview: continuity,
    showTime: continuity,
    timeSeconds,
    bindEnabled: bindReason == null,
    bindReason,
    confirmBeforeReplace: Boolean(input.boundAssetId) || input.dependentLabels.length > 0,
    successorBlocked: continuity && !input.boundAssetId,
  };
}
