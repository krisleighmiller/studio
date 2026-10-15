import type { JobRecord } from "../../workflowTypes";
import { moneyToMinor, minorToMoney } from "../money";

export const REVIEW_ACK = "I reviewed the provider evidence and accept this manual accounting entry";
export type SpendReviewList = { ledgerRevision: string; jobs: JobRecord[] };
export type SpendReviewIntent = {
  jobId: string; ledgerRevision: string; currency: string; actualMinor: number;
  providerOutcome: "charged" | "not_charged"; evidenceReference: string;
  reviewNote: string; acknowledgment: string;
};

/** No automated provider verification exists. This is explicitly a manual evidence review. */
export function renderSpendReview(
  list: SpendReviewList | null,
  fetch: () => void,
  submit: (intent: SpendReviewIntent) => void,
): HTMLElement {
  const panel = document.createElement("section");
  panel.className = "studio-spend-review";
  const heading = document.createElement("h3");
  heading.textContent = "Paid media · unknown or manually reviewed spend";
  const disclosure = document.createElement("p");
  disclosure.textContent = "Provider verification is unavailable here. This is user-entered accounting only: a reference or zero amount cannot prove a provider outcome or enable retry. Unbounded exposure blocks dispatch under strict hard-stop; approval-required and warn-only policies do not guarantee a hard stop. Check provider billing/history outside Studio.";
  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.textContent = "Load / refresh unsettled jobs";
  refresh.addEventListener("click", fetch);
  panel.append(heading, disclosure, refresh);
  if (!list) return panel;
  if (!list.jobs.length) {
    const empty = document.createElement("p");
    empty.textContent = "No unknown or manually reviewed paid jobs.";
    panel.append(empty);
  }
  for (const job of list.jobs) {
    const group = document.createElement("div");
    const label = document.createElement("p");
    label.textContent = `${job.id} · ${job.provider ?? "unknown provider"} · ${job.kind} · ${job.status ?? "unknown"} · estimate ${minorToMoney(job.estimated_cost_minor)} ${job.currency ?? "?"}. ${job.error ?? ""}`;
    group.append(label);
    if (job.reconciliation) {
      const accounting = document.createElement("p");
      accounting.textContent = `Manual accounting ${minorToMoney(job.cost_minor)} ${job.currency ?? "?"} · reviewer-reported ${job.reconciliation.reported_outcome ?? job.reconciliation.provider_outcome ?? "unknown"} · reference ${job.reconciliation.evidence_reference}. Not provider-verified; ${job.reserved_max_minor != null ? `maximum exposure ${minorToMoney(job.reserved_max_minor)} ${job.currency ?? "?"}` : "exposure unbounded"}; retry remains blocked.`;
      group.append(accounting);
      panel.append(group);
      continue;
    }
    // A running or pending job has not yet reached a reviewable outcome.
    if (job.status !== "failed" && job.status !== "completed") {
      panel.append(group);
      continue;
    }
    const form = document.createElement("form");
    const amount = document.createElement("input");
    amount.placeholder = "Actual amount (e.g. 0.00)";
    amount.setAttribute("aria-label", `Actual charge for ${job.id}`);
    amount.required = true;
    const outcome = document.createElement("select");
    outcome.setAttribute("aria-label", `Reviewer-reported outcome for ${job.id}`);
    for (const [value, text] of [["", "Report outcome (not verified)"], ["charged", "Reported charged"], ["not_charged", "Reported not charged"]]) {
      const option = document.createElement("option"); option.value = value; option.textContent = text; outcome.append(option);
    }
    outcome.required = true;
    const evidence = document.createElement("input");
    evidence.placeholder = "Provider request/history ID or billing record reference";
    evidence.required = true;
    const note = document.createElement("input");
    note.placeholder = "What evidence did you review?";
    note.required = true;
    const acknowledge = document.createElement("input");
    acknowledge.type = "checkbox";
    const acknowledgeLabel = document.createElement("label");
    acknowledgeLabel.append(acknowledge, document.createTextNode(REVIEW_ACK));
    const submitButton = document.createElement("button");
    submitButton.type = "submit";
    submitButton.textContent = "Record manual review (not provider-verified)";
    form.append(amount, outcome, evidence, note, acknowledgeLabel, submitButton);
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      const actualMinor = moneyToMinor(amount.value);
      if (actualMinor === null || !Number.isSafeInteger(actualMinor) || !acknowledge.checked || !outcome.value ||
          (actualMinor === 0 && outcome.value !== "not_charged") || (actualMinor > 0 && outcome.value !== "charged")) {
        amount.setCustomValidity("Enter a valid amount matching the confirmed outcome, and acknowledge the manual review.");
        amount.reportValidity();
        return;
      }
      amount.setCustomValidity("");
      submit({ jobId: job.id, ledgerRevision: list.ledgerRevision, currency: job.currency ?? "",
        actualMinor, providerOutcome: outcome.value as "charged" | "not_charged",
        evidenceReference: evidence.value, reviewNote: note.value, acknowledgment: REVIEW_ACK });
    });
    group.append(form);
    panel.append(group);
  }
  return panel;
}
