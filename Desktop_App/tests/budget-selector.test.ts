import { describe, expect, it } from "vitest";
import { buildBudgetModel } from "../src/studio/selectors/budget";
import { renderBudgetSummary } from "../src/studio/components/budgetSummary";
import { buildJobsModel } from "../src/studio/jobsModel";
import { renderSpendReview } from "../src/studio/components/spendReview";
import { minorToMoney, moneyToMinor } from "../src/studio/money";
import type { JobRecord, ProjectBudget, ProjectState } from "../src/workflowTypes";

function job(partial: Record<string, unknown>): JobRecord {
  return { id: "j1", kind: "scene_look", ...partial } as unknown as JobRecord;
}

function stateWith(opts: { jobs?: JobRecord[]; budget?: ProjectBudget | null }): ProjectState {
  return {
    version: 3,
    project_name: "x",
    characters: [],
    scenes: [],
    assets: [],
    jobs: opts.jobs ?? [],
    budget: opts.budget ?? null,
    requirements: [],
    notes: [],
  };
}

describe("buildBudgetModel mirrors the host ledger", () => {
  it("reports no budget and zero spend for an empty project", () => {
    const model = buildBudgetModel(stateWith({}));
    expect(model.hasLimit).toBe(false);
    expect(model.hardStop).toBe(false);
    expect(model.spent).toBe("0.00");
    expect(model.committed).toBe("0.00");
    expect(model.remaining).toBeNull();
    expect(model.unknownJobs).toBe(0);
    expect(model.overLimit).toBe(false);
    expect(model.chatCoverage).toBe("not-accounted");
  });

  it("never advertises the media-job ledger as a project-wide chat guarantee", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, hard_stop: true },
      jobs: [job({ status: "completed", cost_minor: 100 })],
    }));
    expect(model.chatCoverage).toBe("not-accounted");
    const text = renderBudgetSummary(model).textContent ?? "";
    expect(text).toContain("Accounted jobs only");
    expect(text).toContain("LLM/chat costs are not included");
    expect(text).toContain("chat is not budget-enforced");
  });

  it("retains verified maximum exposure on a completed unsettled job, not its estimate as actual", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, policy: "strict_hard_stop" },
      jobs: [job({ status: "completed", provider: "p", reserved_max_minor: 700, estimated_cost_minor: 200, cost_minor: null })],
    }));
    expect(model.spent).toBe("0.00");
    expect(model.committed).toBe("7.00");
    expect(model.remaining).toBe("3.00");
    expect(model.unknownExposureJobs).toBe(0);
    expect(model.gate).toBe("none");
  });

  it("keeps unused reservation after a reported charge without treating it as verified", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, policy: "strict_hard_stop" },
      jobs: [job({ status: "completed", provider: "p", reserved_max_minor: 700, cost_minor: 200, actual_verified: false })],
    }));
    expect(model.spent).toBe("2.00");
    expect(model.committed).toBe("5.00");
    expect(model.unknownJobs).toBe(1);
    expect(model.gate).toBe("none");
    expect(renderBudgetSummary(model).textContent).not.toContain("verified actual");
  });

  it("releases the remainder of a reservation after provider-backed actual verification", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, policy: "strict_hard_stop" },
      jobs: [job({ status: "completed", provider: "p", reserved_max_minor: 700, cost_minor: 200, actual_verified: true })],
    }));
    expect(model.spent).toBe("2.00");
    expect(model.committed).toBe("0.00");
    expect(model.remaining).toBe("8.00");
    expect(model.unknownExposureJobs).toBe(0);
    expect(model.unknownJobs).toBe(0);
  });

  it("labels manually reported amounts as unverified without releasing a bounded reservation", () => {
    const reviewed = job({ status: "completed", provider: "p", reserved_max_minor: 700,
      cost_minor: 200, actual_verified: false, reconciliation: {
        reported_outcome: "charged", evidence_reference: "billing-ref", reviewed_at: "2026-01-01",
      } });
    const row = buildJobsModel({ jobs: [reviewed], scenes: [], assets: [], filter: "all" }).rows[0];
    expect(row?.tooltip).toContain("actual 2.00 (not provider-verified)");
    const reviewText = renderSpendReview({ ledgerRevision: "r", jobs: [reviewed] }, () => {}, () => {}).textContent ?? "";
    expect(reviewText).toContain("maximum exposure 7.00");
    expect(reviewText).not.toContain("dispatch and retry remain blocked");
  });

  it.each([
    { policy: "strict_hard_stop", gate: "blocked" },
    { policy: "approval_required", gate: "none" },
    { policy: "warn_only", gate: "none" },
  ] as const)("discloses unbounded exposure under $policy", ({ policy, gate }) => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, policy },
      jobs: [job({ status: "pending", provider: "p", estimated_cost_minor: 100, unknown_exposure: true })],
    }));
    expect(model.unknownExposureJobs).toBe(1);
    expect(model.remaining).toBeNull();
    expect(model.gate).toBe(gate);
    const text = renderBudgetSummary(model).textContent ?? "";
    expect(text).not.toContain("left");
    expect(text).toContain("unknown exposure");
    if (policy !== "strict_hard_stop") expect(text).not.toContain("hard-stop");
  });

  it("uses policy over legacy hard_stop and warns rather than confirms in warn-only mode", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, policy: "warn_only", hard_stop: true },
      jobs: [job({ status: "running", provider: "p", reserved_max_minor: 1001 })],
    }));
    expect(model.hardStop).toBe(false);
    expect(model.gate).toBe("warn");
    expect(renderBudgetSummary(model).textContent).not.toContain("hard-stop");
  });

  it("sums settled and committed spend, leaving available headroom", () => {
    const model = buildBudgetModel(
      stateWith({
        budget: { currency: "USD", limit_minor: 5000, hard_stop: true },
        jobs: [
          job({ id: "a", status: "completed", cost_minor: 2000, reserved_max_minor: 2000 }),
          job({ id: "b", status: "failed", cost_minor: 500, reserved_max_minor: 500 }),
          job({ id: "c", status: "pending", estimated_cost_minor: 300, reserved_max_minor: 300 }),
        ],
      }),
    );
    expect(model.hasLimit).toBe(true);
    expect(model.spent).toBe("25.00");
    expect(model.committed).toBe("3.00");
    expect(model.remaining).toBe("22.00");
    expect(model.overLimit).toBe(false);
  });

  it("reports host remaining after subtracting outstanding commitments", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, hard_stop: true },
      jobs: [
        job({ id: "a", status: "completed", cost_minor: 900, reserved_max_minor: 900 }),
        job({ id: "b", status: "pending", estimated_cost_minor: 200, reserved_max_minor: 200 }),
      ],
    }));
    expect(model.remaining).toBe("-1.00");
    expect(model.committed).toBe("2.00");
    expect(model.overLimit).toBe(true);
    expect(model.gate).toBe("blocked");
  });

  it("flags over-limit when committed plus settled exceed the ceiling", () => {
    const model = buildBudgetModel(
      stateWith({
        budget: { currency: "USD", limit_minor: 1000, hard_stop: true },
        jobs: [
          job({ id: "a", status: "completed", cost_minor: 900, reserved_max_minor: 900 }),
          job({ id: "b", status: "running", estimated_cost_minor: 200, reserved_max_minor: 200 }),
        ],
      }),
    );
    expect(model.overLimit).toBe(true);
    expect(model.remaining).toBe("-1.00");
  });

  it("requires confirmation rather than blocking beyond a soft limit with a warning", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, warning_at_minor: 800, hard_stop: false },
      jobs: [job({ status: "completed", cost_minor: 1001 })],
    }));
    expect(model.gate).toBe("confirm");
    expect(model.overLimit).toBe(true);
  });

  it.each([
    { amount: 800, hardStop: true, gate: "none", overLimit: false },
    { amount: 801, hardStop: true, gate: "warn", overLimit: false },
    { amount: 1000, hardStop: true, gate: "warn", overLimit: false },
    { amount: 1001, hardStop: true, gate: "blocked", overLimit: true },
    { amount: 1000, hardStop: false, gate: "warn", overLimit: false },
    { amount: 1001, hardStop: false, gate: "confirm", overLimit: true },
  ] as const)("gates $amount minor at strict warning/limit boundaries (hard stop: $hardStop)", ({ amount, hardStop, gate, overLimit }) => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, warning_at_minor: 800, hard_stop: hardStop },
      jobs: [job({ status: "pending", estimated_cost_minor: amount, reserved_max_minor: amount })],
    }));
    expect(model.gate).toBe(gate);
    expect(model.overLimit).toBe(overLimit);
    expect(model.remaining).toBe(minorToMoney(1000 - amount));
  });

  it("counts pending and running jobs without estimates as unknown", () => {
    const model = buildBudgetModel(stateWith({
      jobs: [
        job({ id: "a", status: "pending", provider: "p", estimated_cost_minor: null }),
        job({ id: "b", status: "running", provider: "p" }),
        job({ id: "c", status: "running", estimated_cost_minor: 0 }),
      ],
    }));
    expect(model.unknownJobs).toBe(2);
    expect(model.committed).toBe("0.00");
  });

  it("holds a completed estimate without representing it as actual spend", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, hard_stop: true },
      jobs: [job({ status: "completed", estimated_cost_minor: 700, cost_minor: null })],
    }));
    expect(model.spent).toBe("0.00");
    expect(model.committed).toBe("0.00");
    expect(model.remaining).toBeNull();
    expect(model.unknownExposureJobs).toBe(1);
    expect(model.gate).toBe("blocked");
    expect(model.unknownJobs).toBe(1);
  });

  it("does not combine foreign currency or negative costs into the budget", () => {
    const model = buildBudgetModel(stateWith({
      budget: { currency: "USD", limit_minor: 1000, hard_stop: true },
      jobs: [
        job({ id: "a", status: "completed", currency: "EUR", cost_minor: 500 }),
        job({ id: "b", status: "failed", currency: "USD", cost_minor: -200 }),
      ],
    }));
    expect(model.spent).toBe("0.00");
    expect(model.remaining).toBeNull();
    expect(model.unknownJobs).toBe(2);
  });

  it("counts settled jobs with unknown cost, never as zero", () => {
    const model = buildBudgetModel(
      stateWith({
        jobs: [
          job({ id: "a", status: "completed", provider: "p", cost_minor: null }),
          job({ id: "b", status: "cancelled", provider: "p", cost_minor: null }),
          job({ id: "c", status: "completed", cost_minor: 100 }),
        ],
      }),
    );
    expect(model.spent).toBe("1.00");
    expect(model.unknownJobs).toBe(2);
  });
});

describe("moneyToMinor parses dollars into minor units", () => {
  it("converts whole and fractional dollars", () => {
    expect(moneyToMinor("50")).toBe(5000);
    expect(moneyToMinor("50.00")).toBe(5000);
    expect(moneyToMinor("0.05")).toBe(5);
    expect(moneyToMinor("50.5")).toBe(5050);
  });

  it("returns null for empty and invalid input", () => {
    expect(moneyToMinor("")).toBeNull();
    expect(moneyToMinor("abc")).toBeNull();
    expect(moneyToMinor("1.234")).toBeNull();
    expect(moneyToMinor("-5")).toBeNull();
  });
});