//! Cost, budget, and provider-pricing foundations.
//!
//! Money is represented as integer minor units (cents, pence, …) so authoritative
//! totals never accumulate floating-point error. Unknown cost is an explicit
//! `None`/`Unknown`, never zero.
//!
//! The pricing registry is API-agnostic: it keys on the two data strings the app
//! already owns — `ApiEntry.kind` (a provider family) and a `capabilityDefaults`
//! key (a production operation). No enum of providers or capabilities lives in
//! this module; adding one is a data edit to `providers.json`, not a code change.

use crate::workflow::state::{JobStatus, ProjectState};
use serde::{Deserialize, Serialize};

/// A price the registry knows, or an explicit "we do not know" marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Price {
    Known { currency: String, minor: i64 },
    Unknown,
}

impl Price {
    pub fn minor(&self) -> Option<i64> {
        match self {
            Price::Known { minor, .. } => Some(*minor),
            Price::Unknown => None,
        }
    }

    pub fn currency(&self) -> Option<&str> {
        match self {
            Price::Known { currency, .. } => Some(currency),
            Price::Unknown => None,
        }
    }
}

/// Format integer minor units as a decimal string, e.g. `1234` → `"12.34 USD"`.
pub fn format_minor(minor: i64, currency: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let major = abs / 100;
    let frac = abs % 100;
    format!("{sign}{major}.{frac:02} {currency}")
}

//
// Provider pricing registry (data-driven and API-agnostic).
//
// Keys are data strings, not enums:
//   * `kind`       — an `ApiEntry.kind` ("atlas-cloud", "elevenlabs", …).
//   * `capability` — a `capabilityDefaults` key ("video.text_only", …).
//

#[derive(Debug, Clone, PartialEq)]
struct PriceEntry {
    kind: String,
    capability: String,
    currency: String,
    price_minor: i64,
    pricing_status: Option<String>,
    source_url: Option<String>,
    date_verified: Option<String>,
    billing_unit: Option<String>,
    notes: Option<String>,
}

fn registry() -> &'static [PriceEntry] {
    static REGISTRY: std::sync::OnceLock<Vec<PriceEntry>> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let raw: serde_json::Value = serde_json::from_str(include_str!("providers.json"))
            .expect("providers.json is invalid");
        raw["prices"]
            .as_array()
            .expect("providers.json needs a prices array")
            .iter()
            .map(|item| PriceEntry {
                kind: item["kind"].as_str().unwrap_or("").to_string(),
                capability: item["capability"].as_str().unwrap_or("").to_string(),
                currency: item["currency"].as_str().unwrap_or("USD").to_string(),
                price_minor: item["price_minor"].as_i64().unwrap_or(0),
                pricing_status: item["pricing_status"].as_str().map(|s| s.to_string()),
                source_url: item["source_url"].as_str().map(|s| s.to_string()),
                date_verified: item["date_verified"].as_str().map(|s| s.to_string()),
                billing_unit: item["billing_unit"].as_str().map(|s| s.to_string()),
                notes: item["notes"].as_str().map(|s| s.to_string()),
            })
            .collect()
    })
}

/// Resolve a known price for a provider `kind` + `capability`, case-insensitively.
/// Anything not listed resolves to `Price::Unknown`.
pub fn resolve_price(kind: &str, capability: &str) -> Price {
    let key = |s: &str| s.trim().to_ascii_lowercase();
    let kind_key = key(kind);
    let capability_key = key(capability);
    registry()
        .iter()
        .find(|entry| key(&entry.kind) == kind_key && key(&entry.capability) == capability_key)
        .and_then(|entry| {
            // Entries with unresolved or genuinely unknown pricing resolve to Unknown.
            match entry.pricing_status.as_deref() {
                Some("placeholder") | Some("unknown") => None,
                _ => Some(Price::Known {
                    currency: entry.currency.clone(),
                    minor: entry.price_minor,
                }),
            }
        })
        .unwrap_or(Price::Unknown)
}

/// A pricing issue surfaced by `validate_pricing`.
#[derive(Debug, Clone, PartialEq)]
pub struct PricingWarning {
    pub kind: String,
    pub capability: String,
    pub issue: String,
}

/// Check every registered price entry for unresolved placeholders, missing
/// required metadata, or stale version markers. Returns warnings for each
/// issue found — an empty vec means the registry is clean.
pub fn validate_pricing() -> Vec<PricingWarning> {
    let raw: serde_json::Value = serde_json::from_str(include_str!("providers.json"))
        .expect("providers.json is invalid");
    let mut warnings = Vec::new();

    // Stale version marker.
    if let Some(ver) = raw["pricing_version"].as_str() {
        if ver.contains("placeholder") {
            warnings.push(PricingWarning {
                kind: "".to_string(),
                capability: "".to_string(),
                issue: format!(
                    "pricing_version still contains 'placeholder': \"{}\"",
                    ver
                ),
            });
        }
    }

    // Check each entry.
    for entry in raw["prices"].as_array().unwrap_or(&vec![]) {
        let kind = entry["kind"].as_str().unwrap_or("").to_string();
        let capability = entry["capability"].as_str().unwrap_or("").to_string();
        let status = entry["pricing_status"].as_str();
        let has_source = entry["source_url"].as_str().filter(|s| !s.is_empty());
        let has_date = entry["date_verified"].as_str().filter(|s| !s.is_empty());

        match status {
            Some("placeholder") => {
                warnings.push(PricingWarning {
                    kind,
                    capability,
                    issue: "pricing_status is 'placeholder' — not yet researched".to_string(),
                });
            }
            None => {
                // No explicit status: legacy entry. Treat as "known" but check metadata.
                if has_source.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing source_url".to_string(),
                    });
                }
                if has_date.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing date_verified".to_string(),
                    });
                }
            }
            Some("known") => {
                if has_source.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing source_url".to_string(),
                    });
                }
                if has_date.is_none() {
                    warnings.push(PricingWarning {
                        kind: kind.clone(),
                        capability: capability.clone(),
                        issue: "known-pricing entry missing date_verified".to_string(),
                    });
                }
            }
            // "unknown" — intentionally unresolved; no warning for missing metadata.
            _ => {}
        }
    }

    warnings
}

/// Map a Director specialist name to the capability key its paid work uses.
/// Non-provider specialists (assembly, text writes, …) resolve to `None`, so
/// their cost is honestly reported as unknown rather than invented.
pub fn specialist_capability(specialist: &str) -> Option<&'static str> {
    match specialist.trim().to_ascii_lowercase().as_str() {
        "still_image" | "scene_look" | "model_sheet" | "wardrobe_plate" | "reference_assets" => {
            Some("image.text_to_image")
        }
        "video" | "render" | "scene_generation" => Some("video.text_only"),
        "voice_clone" => Some("audio.tts_clone"),
        "tts" | "tts_profile" => Some("audio.tts_profile"),
        "lip_sync" => Some("video.lip_sync_text_narration"),
        _ => None,
    }
}

/// Known price as `(currency, minor_units)` for a provider `kind` + specialist,
/// or `None` when unknown. Unknown is not zero.
pub fn known_price(kind: &str, specialist: &str) -> Option<(String, i64)> {
    let capability = specialist_capability(specialist)?;
    match resolve_price(kind, capability) {
        Price::Known { currency, minor } => Some((currency, minor)),
        Price::Unknown => None,
    }
}

/// Host-known price as a display label, e.g. `"0.05 USD"`.
pub fn known_price_label(kind: &str, specialist: &str) -> Option<String> {
    known_price(kind, specialist).map(|(currency, minor)| format_minor(minor, &currency))
}

/// Known spend in minor units, or `None` when unknown (not gated numerically).
pub fn known_spend_minor(kind: &str, specialist: &str) -> Option<i64> {
    known_price(kind, specialist).map(|(_, minor)| minor)
}

//
// Budget ledger: derive authoritative spend totals from project state.
//

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLedger {
    pub currency: String,
    pub limit_minor: Option<i64>,
    pub warning_at_minor: Option<i64>,
    pub hard_stop: bool,
    /// Sum of known estimates for pending/running (committed but unsettled) jobs.
    pub committed_minor: i64,
    /// Sum of actual cost for completed jobs.
    pub completed_minor: i64,
    /// Sum of billable cost for failed/cancelled jobs (retried spend is still spend).
    pub failed_minor: i64,
    /// `limit - (completed + failed)`. `None` when there is no budget limit.
    pub remaining_minor: Option<i64>,
    /// Count of jobs whose spend is real but whose cost is unknown.
    pub unknown_cost_jobs: usize,
    /// Budget gate derived from committed+completed+failed spend vs thresholds.
    pub gate: SpendGate,
}

pub fn compute_ledger(state: &ProjectState) -> BudgetLedger {
    let budget = state.budget.clone().unwrap_or_default();
    let mut committed = 0i64;
    let mut completed = 0i64;
    let mut failed = 0i64;
    let mut unknown = 0usize;
    for job in &state.jobs {
        match job.status {
            JobStatus::Pending | JobStatus::Running => {
                if let Some(estimate) = job.estimated_cost_minor {
                    committed = committed.saturating_add(estimate);
                }
            }
            JobStatus::Completed => match job.cost_minor {
                Some(cost) => completed = completed.saturating_add(cost),
                None => unknown += 1,
            },
            JobStatus::Failed | JobStatus::Cancelled => match job.cost_minor {
                Some(cost) => failed = failed.saturating_add(cost),
                None => unknown += 1,
            },
        }
    }
    let spent = completed.saturating_add(failed);
    let remaining = budget.limit_minor.map(|limit| limit.saturating_sub(spent));
    // Compute gate inline to avoid infinite recursion (gate_spend calls compute_ledger).
    let gate = match budget.limit_minor {
        None => SpendGate::NoLimit,
        Some(limit) => {
            let on_the_hook = committed.saturating_add(spent);
            if on_the_hook > limit {
                if budget.hard_stop {
                    SpendGate::Blocked
                } else {
                    SpendGate::Confirm
                }
            } else if let Some(warn_at) = budget.warning_at_minor {
                if on_the_hook > warn_at {
                    SpendGate::Warn
                } else {
                    SpendGate::Allow
                }
            } else {
                SpendGate::Allow
            }
        }
    };
    BudgetLedger {
        currency: budget.currency,
        limit_minor: budget.limit_minor,
        warning_at_minor: budget.warning_at_minor,
        hard_stop: budget.hard_stop,
        committed_minor: committed,
        completed_minor: completed,
        failed_minor: failed,
        remaining_minor: remaining,
        unknown_cost_jobs: unknown,
        gate,
    }
}

/// Whether a proposed spend can proceed under the project budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendGate {
    /// No budget limit configured — always allowed.
    NoLimit,
    /// Under both the warning threshold and the limit.
    Allow,
    /// Over the warning threshold but under the limit.
    Warn,
    /// Would exceed the limit, but the budget is not hard-stop: explicit
    /// confirmation is required.
    Confirm,
    /// Would exceed the limit and the budget is hard-stop: vetoed.
    Blocked,
}

/// Decide whether `proposed_minor` of new spend is allowed given committed,
/// completed, and failed spend already in the project. A hard-stop budget
/// blocks spending over its limit; this is the mechanism the Director cannot
/// bypass.
pub fn gate_spend(state: &ProjectState, proposed_minor: i64) -> SpendGate {
    let Some(budget) = &state.budget else {
        return SpendGate::NoLimit;
    };
    let Some(limit) = budget.limit_minor else {
        return SpendGate::NoLimit;
    };
    let ledger = compute_ledger(state);
    let on_the_hook = ledger
        .committed_minor
        .saturating_add(ledger.completed_minor)
        .saturating_add(ledger.failed_minor);
    let total = on_the_hook.saturating_add(proposed_minor.max(0));
    if total > limit {
        return if budget.hard_stop {
            SpendGate::Blocked
        } else {
            SpendGate::Confirm
        };
    }
    if let Some(warning) = budget.warning_at_minor {
        if total > warning {
            return SpendGate::Warn;
        }
    }
    SpendGate::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{JobRecord, JobStatus, ProjectBudget};

    fn job(
        kind: &str,
        status: JobStatus,
        cost_minor: Option<i64>,
        estimated_minor: Option<i64>,
    ) -> JobRecord {
        let mut job = JobRecord::open("job", kind);
        job.status = status;
        job.cost_minor = cost_minor;
        job.estimated_cost_minor = estimated_minor;
        job
    }

    #[test]
    fn registry_resolves_known_price_case_insensitively() {
        assert_eq!(
            resolve_price("ATLAS-CLOUD", " IMAGE.TEXT_TO_IMAGE "),
            Price::Known {
                currency: "USD".to_string(),
                minor: 7
            }
        );
        assert_eq!(
            resolve_price("atlas-cloud", "video.text_only"),
            Price::Known {
                currency: "USD".to_string(),
                minor: 37
            }
        );
    }

    #[test]
    fn registry_marks_unlisted_entries_unknown_not_zero() {
        assert_eq!(resolve_price("runway", "video.text_only"), Price::Unknown);
        assert_eq!(resolve_price("atlas-cloud", "audio.tts_custom"), Price::Unknown);
        assert_eq!(resolve_price("", ""), Price::Unknown);
        assert_eq!(
            resolve_price("elevenlabs", "video.text_only"),
            Price::Unknown
        );
    }

    #[test]
    fn minor_units_format_to_decimal_without_float_error() {
        assert_eq!(format_minor(1234, "USD"), "12.34 USD");
        assert_eq!(format_minor(400, "USD"), "4.00 USD");
        assert_eq!(format_minor(5, "USD"), "0.05 USD");
        assert_eq!(format_minor(-1234, "USD"), "-12.34 USD");
    }

    #[test]
    fn ledger_sums_spend_and_keeps_unknown_separate_from_zero() {
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".to_string(),
            limit_minor: Some(10_000),
            warning_at_minor: Some(7_000),
            hard_stop: true,
        });
        state.jobs = vec![
            job("render", JobStatus::Pending, None, Some(1_000)),
            job("render", JobStatus::Completed, Some(2_000), Some(1_500)),
            job("look", JobStatus::Failed, Some(500), Some(250)),
            job("look", JobStatus::Completed, None, None), // unknown-cost completed job
            job("look", JobStatus::Cancelled, Some(300), None),
        ];

        let ledger = compute_ledger(&state);
        assert_eq!(ledger.currency, "USD");
        assert_eq!(ledger.committed_minor, 1_000);
        assert_eq!(ledger.completed_minor, 2_000);
        assert_eq!(ledger.failed_minor, 800);
        assert_eq!(ledger.remaining_minor, Some(7_200));
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert!(ledger.hard_stop);
    }

    #[test]
    fn ledger_without_a_limit_has_no_remaining_but_still_tracks_unknown() {
        let mut state = ProjectState::new();
        state.jobs = vec![
            job("render", JobStatus::Completed, None, None),
            job("render", JobStatus::Completed, Some(123), None),
        ];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.remaining_minor, None);
        assert_eq!(ledger.completed_minor, 123);
        assert_eq!(ledger.unknown_cost_jobs, 1);
        assert_eq!(ledger.currency, "USD");
    }

    #[test]
    fn missing_budget_defaults_to_usd_and_no_limit() {
        let state = ProjectState::new();
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.currency, "USD");
        assert_eq!(ledger.limit_minor, None);
        assert!(!ledger.hard_stop);
    }

    #[test]
    fn specialist_names_map_to_capability_keys() {
        assert_eq!(
            specialist_capability("still_image"),
            Some("image.text_to_image")
        );
        assert_eq!(
            specialist_capability("scene_look"),
            Some("image.text_to_image")
        );
        assert_eq!(specialist_capability("VIDEO"), Some("video.text_only"));
        assert_eq!(
            specialist_capability("voice_clone"),
            Some("audio.tts_clone")
        );
        assert_eq!(specialist_capability("assemble"), None);
        assert_eq!(specialist_capability("text_write"), None);
        assert_eq!(specialist_capability(""), None);
    }

    #[test]
    fn known_price_label_resolves_known_and_unknown() {
        assert_eq!(
            known_price_label("atlas-cloud", "still_image"),
            Some("0.07 USD".to_string())
        );
        assert_eq!(
            known_price_label("atlas-cloud", "video"),
            Some("0.37 USD".to_string())
        );
        assert_eq!(
            known_price_label("elevenlabs", "voice_clone"),
            Some("0.00 USD".to_string())  // no per-clone API charge
        );
        // TTS pricing is per-character — genuinely unknown per operation.
        assert_eq!(known_price_label("elevenlabs", "tts"), None);
        // Unknown kind, unknown capability, non-provider specialist: unknown.
        assert_eq!(known_price_label("runway", "still_image"), None);
        assert_eq!(known_price_label("atlas-cloud", "assemble"), None);
        assert_eq!(known_price_label("atlas-cloud", ""), None);
    }

    #[test]
    fn known_spend_minor_returns_minor_units_or_unknown() {
        assert_eq!(known_spend_minor("atlas-cloud", "still_image"), Some(7));
        assert_eq!(known_spend_minor("atlas-cloud", "video"), Some(37));
        assert_eq!(known_spend_minor("elevenlabs", "voice_clone"), Some(0));
        // TTS is unknown per operation — None, never 0.
        assert_eq!(known_spend_minor("elevenlabs", "tts"), None);
        // Unknown spend is None, never 0.
        assert_eq!(known_spend_minor("runway", "still_image"), None);
        assert_eq!(known_spend_minor("atlas-cloud", "assemble"), None);
    }

    #[test]
    fn the_director_cannot_bypass_a_hard_stop_for_known_spend() {
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(30),
                warning_at_minor: None,
                hard_stop: true,
            }),
            ..ProjectState::new()
        };
        // Known video spend (37 minor) over a 30-minor hard-stop limit → vetoed.
        let minor = known_spend_minor("atlas-cloud", "video").unwrap();
        assert_eq!(gate_spend(&state, minor), SpendGate::Blocked);
        // Under the limit with image spend (7 minor).
        let minor2 = known_spend_minor("atlas-cloud", "still_image").unwrap();
        assert_eq!(
            gate_spend(&state, minor2),
            SpendGate::Allow
        );
        // Unknown spend has no minor value to compare, so it is not vetoed here;
        // the ledger flags it separately.
        assert_eq!(known_spend_minor("elevenlabs", "tts"), None);
        assert_eq!(known_spend_minor("runway", "still_image"), None);
    }

    #[test]
    fn a_completed_job_defaults_actual_cost_to_its_known_estimate() {
        let mut job = JobRecord::open("j", "scene_look");
        job.estimated_cost_minor = Some(40);
        job.currency = Some("USD".to_string());
        job.finish(Some("out.png"), None);
        assert_eq!(job.cost_minor, Some(40));

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.completed_minor, 40);
        assert_eq!(ledger.unknown_cost_jobs, 0);
    }

    #[test]
    fn failed_jobs_with_billable_cost_flow_into_ledger() {
        let mut job = JobRecord::open("j", "render");
        job.estimated_cost_minor = Some(500);
        job.currency = Some("USD".to_string());
        // Billable failure: provider charged despite the error.
        job.fail(Some(350), "GPU preempted");
        assert_eq!(job.cost_minor, Some(350));
        assert_eq!(job.error.as_deref(), Some("GPU preempted"));

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.failed_minor, 350);
        assert_eq!(ledger.completed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 0);
    }

    #[test]
    fn failed_jobs_without_billable_cost_are_unknown_cost() {
        let mut job = JobRecord::open("j", "render");
        job.estimated_cost_minor = Some(500);
        job.currency = Some("USD".to_string());
        // Non-billable failure: provider didn't charge for the failed attempt.
        job.fail(None, "timeout");
        assert!(job.cost_minor.is_none());

        let mut state = ProjectState::new();
        state.jobs = vec![job];
        let ledger = compute_ledger(&state);
        assert_eq!(ledger.failed_minor, 0);
        assert_eq!(ledger.unknown_cost_jobs, 1);
    }

    #[test]
    fn ledger_gate_reports_budget_status_at_all_thresholds() {
        // ---- No limit ----
        let state = ProjectState {
            budget: None,
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::NoLimit);

        // ---- Under warn threshold, under limit ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: Some(7_000),
                hard_stop: true,
            }),
            // committed 500 + completed 1_000 = on_the_hook = 1_500 → under warn (7_000)
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(1_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Allow);

        // ---- Over warn, under limit ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: Some(2_000),
                hard_stop: true,
            }),
            // committed 500 + completed 2_000 = 2_500 → over warn (2_000), under limit
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Warn);

        // ---- Over limit, soft stop → Confirm ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(2_000),
                warning_at_minor: None,
                hard_stop: false,
            }),
            // committed 500 + completed 2_000 = 2_500 → over limit (2_000), soft
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Confirm);

        // ---- Over limit, hard stop → Blocked ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(2_000),
                warning_at_minor: None,
                hard_stop: true,
            }),
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(2_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Blocked);

        // ---- No warning configured, under limit → Allow ----
        let state = ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(10_000),
                warning_at_minor: None,
                hard_stop: true,
            }),
            // committed 500 + completed 1_000 = 1_500 → under limit (10_000), no warn
            jobs: vec![
                job("r1", JobStatus::Pending, None, Some(500)),
                job("r2", JobStatus::Completed, Some(1_000), None),
            ],
            ..ProjectState::new()
        };
        assert_eq!(compute_ledger(&state).gate, SpendGate::Allow);
    }

    #[test]
    fn gate_spend_applies_warning_confirm_and_hard_stop() {
        let budgeted = |limit: i64, warning: i64, hard_stop: bool| ProjectState {
            budget: Some(ProjectBudget {
                currency: "USD".into(),
                limit_minor: Some(limit),
                warning_at_minor: Some(warning),
                hard_stop,
            }),
            ..ProjectState::new()
        };

        // No budget, and a budget with no limit: always allowed.
        assert_eq!(
            gate_spend(&ProjectState::new(), 100_000),
            SpendGate::NoLimit
        );
        assert_eq!(
            gate_spend(&ProjectState::default(), 100_000),
            SpendGate::NoLimit
        );

        let mut hard = budgeted(10_000, 7_000, true);
        assert_eq!(gate_spend(&hard, 1_000), SpendGate::Allow);
        assert_eq!(gate_spend(&hard, 8_000), SpendGate::Warn);
        assert_eq!(gate_spend(&hard, 11_000), SpendGate::Blocked);

        let soft = budgeted(10_000, 7_000, false);
        assert_eq!(gate_spend(&soft, 11_000), SpendGate::Confirm);

        // Already over the limit from completed spend: hard stop blocks even a
        // zero-cost proposal, and a negative proposal cannot buy headroom.
        hard.jobs = vec![job("render", JobStatus::Completed, Some(12_000), None)];
        assert_eq!(gate_spend(&hard, 0), SpendGate::Blocked);
        assert_eq!(gate_spend(&hard, -5_000), SpendGate::Blocked);
    }

    #[test]
    fn validate_pricing_reports_clean_on_current_registry() {
        let warnings = validate_pricing();
        assert!(
            warnings.is_empty(),
            "expected no pricing warnings but got: {:?}",
            warnings
        );
    }

    #[test]
    fn pricing_status_unknown_returns_unknown_price() {
        // ElevenLabs TTS: pricing_status = "unknown" → resolve_price returns Unknown
        // even though the entry exists in the registry.
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_profile"),
            Price::Unknown
        );
    }

    #[test]
    fn pricing_status_known_entry_returns_known_price() {
        // Voice clone is Known(0) — the API has no per-clone charge.
        assert_eq!(
            resolve_price("elevenlabs", "audio.tts_clone"),
            Price::Known {
                currency: "USD".to_string(),
                minor: 0
            }
        );
    }

    #[test]
    fn video_capabilities_all_resolve_to_known_prices() {
        for cap in &[
            "video.text_only",
            "video.start_frame",
            "video.edit",
            "video.character_reference",
            "video.location_prop_other_reference",
            "video.lip_sync_text_narration",
            "video.lip_sync_audio_included",
        ] {
            let price = resolve_price("atlas-cloud", cap);
            assert_eq!(
                price,
                Price::Known {
                    currency: "USD".to_string(),
                    minor: 37
                },
                "capability {cap} did not resolve to known price 37"
            );
        }
    }
}
