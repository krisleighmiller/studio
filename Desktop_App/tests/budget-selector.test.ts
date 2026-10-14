import { describe, expect, it } from "vitest";
import { buildBudgetModel } from "../src/studio/selectors/budget";
import { renderBudgetSummary } from "../src/studio/components/budgetSummary";
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
    expect(text).toContain("Chat requests are blocked");
  });

  it("sums settled and committed spend, leaving available headroom", () => {
    const model = buildBudgetModel(
      stateWith({
        budget: { currency: "USD", limit_minor: 5000, hard_stop: true },
        jobs: [
          job({ id: "a", status: "completed", cost_minor: 2000 }),
          job({ id: "b", status: "failed", cost_minor: 500 }),
          job({ id: "c", status: "pending", estimated_cost_minor: 300 }),
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
        job({ id: "a", status: "completed", cost_minor: 900 }),
        job({ id: "b", status: "pending", estimated_cost_minor: 200 }),
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
          job({ id: "a", status: "completed", cost_minor: 900 }),
          job({ id: "b", status: "running", estimated_cost_minor: 200 }),
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
      jobs: [job({ status: "pending", estimated_cost_minor: amount })],
    }));
    expect(model.gate).toBe(gate);
    expect(model.overLimit).toBe(overLimit);
    expect(model.remaining).toBe(minorToMoney(1000 - amount));
  });

  it("counts pending and running jobs without estimates as unknown", () => {
    const model = buildBudgetModel(stateWith({
      jobs: [
        job({ id: "a", status: "pending", estimated_cost_minor: null }),
        job({ id: "b", status: "running" }),
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
    expect(model.committed).toBe("7.00");
    expect(model.remaining).toBe("3.00");
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
    expect(model.remaining).toBe("10.00");
    expect(model.unknownJobs).toBe(2);
  });

  it("counts settled jobs with unknown cost, never as zero", () => {
    const model = buildBudgetModel(
      stateWith({
        jobs: [
          job({ id: "a", status: "completed", cost_minor: null }),
          job({ id: "b", status: "cancelled", cost_minor: null }),
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