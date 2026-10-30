import type { BudgetModel } from "../selectors/budget";

/** Renders the compact budget line shown above the jobs ledger. It does not call the host. */
export function renderBudgetSummary(model: BudgetModel): HTMLElement {
  const line = document.createElement("p");
  line.className = "studio-budget";
  const parts: string[] = [];

  // Gate status: highlighted visual flag for the most restrictive gate.
  if (model.gate === "blocked") {
    parts.push("🚫 BLOCKED - over hard-stop limit");
    line.classList.add("studio-budget-over");
  } else if (model.gate === "confirm") {
    parts.push("⚠️  CONFIRM - over limit");
    line.classList.add("studio-budget-over");
  } else if (model.gate === "warn") {
    parts.push("💡 nearing limit");
  }

  if (model.hasLimit) {
    parts.push(model.remaining === null
      ? "Remaining cannot be guaranteed (unknown exposure)"
      : `${model.remaining} ${model.currency} left against known exposure`);
  } else {
    parts.push("No project budget limit · testing without budget enforcement");
    if (model.jobCount === 0) parts.push("No accounted generation jobs yet");
  }
  const hasUnknownCosts = model.unknownJobs > 0 || model.unknownExposureJobs > 0;
  if (model.hasLimit || model.jobCount > 0) {
    // Zero in the numeric ledger is not evidence that an unknown provider charged nothing.
    if (!hasUnknownCosts || model.spent !== "0.00") {
      parts.push(`${model.spent} ${model.currency} recorded spend`);
    }
    if (!hasUnknownCosts || model.committed !== "0.00") {
      parts.push(`${model.committed} ${model.currency} recorded commitments`);
    }
    if (hasUnknownCosts) parts.push("Provider costs unknown/unverified. Recorded amounts exclude unknown costs.");
  }
  if (model.unknownJobs > 0) {
    parts.push(`${model.unknownJobs} unverified/unknown-cost ${model.unknownJobs === 1 ? "job" : "jobs"}`);
  }
  if (model.unknownExposureJobs > 0) {
    parts.push(`${model.unknownExposureJobs} unknown exposure ${model.unknownExposureJobs === 1 ? "job" : "jobs"}${model.hardStop && model.hasLimit ? " · strict paid dispatch blocked" : " · exposure not bounded"}`);
  }
  if (model.overLimit) {
    parts.push(model.hardStop ? "over hard-stop limit" : "over budget");
  }
  parts.push("Accounted jobs only; LLM/chat costs are not included; chat is not budget-enforced.");
  parts.push("Unknown provider costs remain unknown, not zero. Provider charges may still apply. Generation still requires approval.");
  line.textContent = parts.join(" · ");
  if (model.overLimit) line.classList.add("studio-budget-over");
  return line;
}