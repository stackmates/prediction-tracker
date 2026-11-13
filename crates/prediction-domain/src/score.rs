//! Binary Brier `(p − y)²` and the Wilson 95% interval.
//!
//! Reimplemented from the published formulas (house rule: single-term 0–1 form,
//! lower is better; Wilson two-sided `z = 1.959963984540054`). Not imported
//! from any other crate.

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::record::{DecisionRecord, PredictionRecord, Verdict};

/// Two-sided 95% z used by the Wilson interval.
pub const WILSON_Z: f64 = 1.959_963_984_540_054;

/// A closed interval on [0, 1].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

/// Per-prediction binary Brier: `(p − y)²` on the 0–1 scale, lower is better.
/// `y` is 0 or 1.
#[must_use]
pub fn binary_brier(p: f64, y: f64) -> f64 {
    (p - y).powi(2)
}

/// Mean of per-prediction binary Brier scores. `None` if the set is empty.
#[must_use]
pub fn mean_brier(scores: &[f64]) -> Option<f64> {
    if scores.is_empty() {
        None
    } else {
        Some(scores.iter().sum::<f64>() / scores.len() as f64)
    }
}

/// Two-sided Wilson score interval at 95% (`z = 1.959963984540054`).
/// `None` when `total` is 0. Bounds are clipped to [0, 1].
#[must_use]
pub fn wilson_95(successes: usize, total: usize) -> Option<Interval> {
    if total == 0 {
        return None;
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z = WILSON_Z;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let margin = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt() / denominator;
    Some(Interval {
        low: (center - margin).max(0.0),
        high: (center + margin).min(1.0),
    })
}

/// Skill against a reference Brier: `1 − Brier / Brier_ref`. `None` when the
/// reference is 0.
#[must_use]
pub fn skill(brier: f64, brier_ref: f64) -> Option<f64> {
    if brier_ref == 0.0 {
        None
    } else {
        Some(1.0 - brier / brier_ref)
    }
}

/// Coin-flip reference Brier (always `p = 0.5`).
pub const COIN_BRIER: f64 = 0.25;

/// Brier of a forecaster who always predicts `pi`.
#[must_use]
pub fn constant_forecaster_brier(pi: f64, outcomes: &[f64]) -> Option<f64> {
    if outcomes.is_empty() {
        return None;
    }
    let scores: Vec<f64> = outcomes.iter().map(|y| binary_brier(pi, *y)).collect();
    mean_brier(&scores)
}

/// A probability decile bucket for the calibration table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationBucket {
    pub bucket: String,
    pub n: usize,
    pub hit_rate: Option<f64>,
    pub wilson_95: Option<Interval>,
}

/// Per-source aggregate used in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceScore {
    pub source: String,
    pub n: usize,
    pub mean_brier: Option<f64>,
    pub successes: usize,
    pub hit_rate: Option<f64>,
    pub wilson_95: Option<Interval>,
    pub void_count: usize,
    pub ranked: bool,
    pub skill_vs_coin: Option<f64>,
    pub skill_vs_base_rate: Option<f64>,
    pub base_rate_pi: Option<f64>,
}

/// Build calibration deciles on Brier-set claims (stated `p`, resolved, non-void).
#[must_use]
pub fn calibration_deciles(records: &[&PredictionRecord]) -> Vec<CalibrationBucket> {
    let mut buckets: Vec<(usize, usize)> = vec![(0, 0); 10]; // (successes, n)
    for r in records {
        let Some(p) = r.probability else { continue };
        let Some(y) = r.brier_outcome() else { continue };
        if !r.in_brier_set() {
            continue;
        }
        let mut idx = (p * 10.0).floor() as usize;
        if idx > 9 {
            idx = 9;
        }
        buckets[idx].1 += 1;
        if y == 1.0 {
            buckets[idx].0 += 1;
        }
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(i, (successes, n))| {
            let lo = i as f64 / 10.0;
            let hi = (i + 1) as f64 / 10.0;
            CalibrationBucket {
                bucket: format!("[{lo:.1}, {hi:.1})"),
                n,
                hit_rate: if n == 0 {
                    None
                } else {
                    Some(successes as f64 / n as f64)
                },
                wilson_95: wilson_95(successes, n),
            }
        })
        .collect()
}

/// Per-source means. A source is not ranked when scored `n` is below 10.
#[must_use]
pub fn score_by_source(records: &[&PredictionRecord]) -> Vec<SourceScore> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<&PredictionRecord>> = BTreeMap::new();
    for r in records {
        groups.entry(r.forecaster_id.clone()).or_default().push(*r);
    }
    groups
        .into_iter()
        .map(|(source, recs)| {
            let void_count = recs
                .iter()
                .filter(|r| r.verdict == Some(Verdict::Void))
                .count();
            let scored: Vec<(&PredictionRecord, f64, f64)> = recs
                .iter()
                .filter_map(|r| {
                    let p = r.probability?;
                    let y = r.brier_outcome()?;
                    if r.in_brier_set() {
                        Some((*r, p, y))
                    } else {
                        None
                    }
                })
                .collect();
            let scores: Vec<f64> = scored
                .iter()
                .map(|(_, p, y)| binary_brier(*p, *y))
                .collect();
            let outcomes: Vec<f64> = scored.iter().map(|(_, _, y)| *y).collect();
            let n = scores.len();
            let mean = mean_brier(&scores);
            let successes = outcomes.iter().filter(|y| **y == 1.0).count();
            let pi = empirical_or_stated_pi(&recs, &outcomes);
            let base_rate_brier = pi.and_then(|pi| constant_forecaster_brier(pi, &outcomes));
            SourceScore {
                source,
                n,
                mean_brier: mean,
                successes,
                hit_rate: if n == 0 {
                    None
                } else {
                    Some(successes as f64 / n as f64)
                },
                wilson_95: wilson_95(successes, n),
                void_count,
                ranked: n >= 10,
                skill_vs_coin: mean.and_then(|b| skill(b, COIN_BRIER)),
                skill_vs_base_rate: match (mean, base_rate_brier) {
                    (Some(b), Some(r)) => skill(b, r),
                    _ => None,
                },
                base_rate_pi: pi,
            }
        })
        .collect()
}

fn empirical_or_stated_pi(recs: &[&PredictionRecord], outcomes: &[f64]) -> Option<f64> {
    // Prefer a numeric frozen base rate if every scored row states the same one.
    // The field is often prose, so fall back to the empirical rate of o = 1.
    let stated: Vec<f64> = recs
        .iter()
        .filter_map(|r| {
            r.base_rate_or_reference_class
                .as_deref()
                .and_then(parse_probability_token)
        })
        .collect();
    if stated.len() == recs.len() && !stated.is_empty() {
        let first = stated[0];
        if stated.iter().all(|x| (*x - first).abs() < 1e-12) {
            return Some(first);
        }
    }
    if outcomes.is_empty() {
        None
    } else {
        Some(outcomes.iter().sum::<f64>() / outcomes.len() as f64)
    }
}

fn parse_probability_token(s: &str) -> Option<f64> {
    let t = s.trim();
    let v: f64 = t.parse().ok()?;
    if (0.0..=1.0).contains(&v) {
        Some(v)
    } else {
        None
    }
}

/// Attach a Brier to a already-settled, frozen, Brier-set claim. Refuses
/// unfrozen / mutated records. The decision, when present, must carry a
/// `problem_id` and at least one frozen prediction id.
pub fn score_claim(
    record: &mut PredictionRecord,
    decision: Option<&DecisionRecord>,
    frozen_prediction_ids: &[String],
) -> Result<Option<f64>, Error> {
    crate::freeze::require_frozen(record)?;
    if let Some(d) = decision {
        d.validate_for_score(frozen_prediction_ids)?;
    } else if record
        .decision_id
        .as_ref()
        .and_then(|m| m.known())
        .is_some()
    {
        // A named decision was required to score; the caller must supply it.
        return Err(Error::DecisionMissingFrozenPrediction {
            decision_id: record
                .decision_id
                .as_ref()
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        });
    } else {
        return Err(Error::DecisionMissingFrozenPrediction {
            decision_id: record.claim_id.clone(),
        });
    }
    if record.verdict == Some(Verdict::Void) {
        record.brier = None;
        return Ok(None);
    }
    let Some(p) = record.probability else {
        record.brier = None;
        return Ok(None);
    };
    let Some(y) = record.brier_outcome() else {
        return Err(Error::NotInBrierSet {
            claim_id: record.claim_id.clone(),
            reason: "no resolved non-void outcome to score".into(),
        });
    };
    let b = binary_brier(p, y);
    record.brier = Some(b);
    Ok(Some(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_case_mean_brier_is_0_065() {
        // Port of the lab test: {p=0.8, o=1} and {p=0.3, o=0} → mean 0.065.
        // A pending (unresolved) forecast is not scored.
        let scores = [binary_brier(0.8, 1.0), binary_brier(0.3, 0.0)];
        let mean = mean_brier(&scores).unwrap();
        assert!((mean - 0.065).abs() < 1e-12);
        assert!((binary_brier(0.8, 1.0) - 0.04).abs() < 1e-12);
        assert!((binary_brier(0.3, 0.0) - 0.09).abs() < 1e-12);
        assert_eq!(mean_brier(&[]), None);
    }

    #[test]
    fn wilson_does_not_turn_one_success_into_certainty() {
        let interval = wilson_95(1, 1).unwrap();
        assert!((interval.low - 0.206_549_314).abs() < 1e-8);
        assert!((interval.high - 1.0).abs() < 1e-12);
        assert!(wilson_95(0, 0).is_none());
        let interval = wilson_95(0, 10).unwrap();
        assert!(interval.low.abs() < 1e-12);
        assert!((interval.high - 0.277_532_8).abs() < 1e-6);
    }

    #[test]
    fn skill_against_coin_and_zero_ref() {
        assert!((skill(0.0, 0.25).unwrap() - 1.0).abs() < 1e-12);
        assert!((skill(0.25, 0.25).unwrap() - 0.0).abs() < 1e-12);
        assert!(skill(0.1, 0.0).is_none());
    }
}
