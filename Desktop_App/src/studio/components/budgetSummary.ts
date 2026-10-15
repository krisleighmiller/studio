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
    parts.push(`${model.remaining} ${model.currency} left`);
  } else {
    parts.push(`No budget · ${model.jobCount === 0 ? "no spend yet" : `${model.spent} ${model.currency} spent`}`);
  }
  if (model.hasLimit) {
    parts.push(`${model.spent} ${model.currency} spent`);
    parts.push(`${model.committed} ${model.currency} committed`);
  } else if (model.jobCount > 0) {
    parts.push(`${model.committed} ${model.currency} committed`);
  }
  if (model.unknownJobs > 0) {
    parts.push(model.unknownJobs === 1 ? "1 unverified/unknown-cost job · paid dispatch blocked" : `${model.unknownJobs} unverified/unknown-cost jobs · paid dispatch blocked`);
  }
  if (model.overLimit) {
    parts.push(model.hardStop ? "over hard-stop limit" : "over budget");
  }
  parts.push("Accounted jobs only; LLM/chat costs are not included. Chat requests are blocked until budget enforcement is wired.");
  line.textContent = parts.join(" · ");
  if (model.overLimit) line.classList.add("studio-budget-over");
  return line;
}