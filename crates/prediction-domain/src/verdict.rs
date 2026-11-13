//! Program-computed settlement: `true`, `false`, or `void`.

use crate::error::Error;
use crate::freeze::{frozen_before_resolution, require_frozen};
use crate::limits::{check_finite, check_https_uri, check_len, MAX_ID_LEN, MAX_TEXT_LEN};
use crate::record::{Direction, PredictionRecord, ThresholdVerdict, Verdict, VerdictDetail};

/// Facts the verifier records. The program, not the verifier, sets the verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct ActualInput {
    pub verifier_id: String,
    pub actual_at: String,
    /// Required for a measured actual; also required when requesting void, so
    /// "the source is gone" is itself evidenced. Absence is an error, not void.
    pub evidence_uri: Option<String>,
    pub actual_value: Option<f64>,
    pub resolved_outcome: Option<String>,
    /// Verifier states that the named source is gone or the frozen rule cannot
    /// be applied. The program then sets `void`.
    pub void_reason: Option<String>,
    /// If the caller tries to supply a verdict, it is checked: disagreement is
    /// `VerdictIsComputed`. The CLI does not offer a --verdict flag.
    pub caller_verdict: Option<Verdict>,
}

/// Record a measured actual (or a evidenced void request) and compute the verdict.
pub fn record_actual(record: &mut PredictionRecord, input: ActualInput) -> Result<Verdict, Error> {
    require_frozen(record)?;
    if record.verdict.is_some() || record.actual_at.is_some() {
        return Err(Error::ActualAlreadyRecorded {
            claim_id: record.claim_id.clone(),
        });
    }
    if input.verifier_id.trim().is_empty() {
        return Err(Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "verifier_id",
        });
    }
    check_len("verifier_id", &input.verifier_id, MAX_ID_LEN)?;
    if input.verifier_id == record.forecaster_id {
        return Err(Error::VerifierIsForecaster {
            id: input.verifier_id,
        });
    }
    let evidence = input
        .evidence_uri
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let evidence = match evidence {
        None => {
            return Err(Error::ActualEvidenceMissing {
                claim_id: record.claim_id.clone(),
            })
        }
        Some(uri) => {
            check_https_uri("actual_evidence_uri", uri)?;
            uri.to_string()
        }
    };
    if input.actual_at.trim().is_empty() {
        return Err(Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "actual_at",
        });
    }
    if let Some(text) = input.resolved_outcome.as_deref() {
        check_len("resolved_outcome", text, MAX_TEXT_LEN)?;
    }
    if let Some(text) = input.void_reason.as_deref() {
        check_len("void_reason", text, MAX_TEXT_LEN)?;
    }

    let computed = compute_settlement(record, &input)?;

    if let Some(supplied) = input.caller_verdict {
        if supplied != computed.verdict {
            return Err(Error::VerdictIsComputed {
                claim_id: record.claim_id.clone(),
            });
        }
    }

    record.verifier_id = Some(input.verifier_id);
    record.actual_at = Some(input.actual_at);
    record.actual_evidence_uri = Some(evidence);
    record.actual_value = computed.actual_value;
    record.verdict = Some(computed.verdict);
    record.verdict_detail = computed.detail;
    record.resolved_outcome = input.resolved_outcome;
    record.void_reason = input.void_reason;
    record.void_requested = if computed.verdict == Verdict::Void {
        Some(true)
    } else {
        None
    };
    record.brier = computed.brier;
    let _ = frozen_before_resolution(record);
    Ok(computed.verdict)
}

struct Computed {
    verdict: Verdict,
    actual_value: Option<f64>,
    detail: Option<VerdictDetail>,
    brier: Option<f64>,
}

fn compute_settlement(record: &PredictionRecord, input: &ActualInput) -> Result<Computed, Error> {
    if input
        .void_reason
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Ok(Computed {
            verdict: Verdict::Void,
            actual_value: input.actual_value,
            detail: None,
            brier: None,
        });
    }

    if record.is_binary_event() {
        let value = input
            .actual_value
            .ok_or_else(|| Error::MissingRequiredField {
                claim_id: record.claim_id.clone(),
                field: "actual_value",
            })?;
        check_finite("actual_value", value)?;
        if (value - 1.0).abs() > f64::EPSILON && (value - 0.0).abs() > f64::EPSILON {
            return Err(Error::InvalidNumeric {
                field: "actual_value",
                reason: "for a binary event, actual_value must be 1 or 0".into(),
            });
        }
        let o = if (value - 1.0).abs() <= f64::EPSILON {
            1.0
        } else {
            0.0
        };
        let verdict = if o == 1.0 {
            Verdict::True
        } else {
            Verdict::False
        };
        let brier = record.probability.map(|p| crate::score::binary_brier(p, o));
        return Ok(Computed {
            verdict,
            actual_value: Some(o),
            detail: None,
            brier,
        });
    }

    let actual = input
        .actual_value
        .ok_or_else(|| Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "actual_value",
        })?;
    let detail = compute_numeric(record, actual)?;
    let (verdict, o) = match detail.threshold_verdict {
        ThresholdVerdict::Pass => (Verdict::True, 1.0),
        ThresholdVerdict::Fail | ThresholdVerdict::Inconclusive => (Verdict::False, 0.0),
    };
    let brier = record.probability.map(|p| crate::score::binary_brier(p, o));
    Ok(Computed {
        verdict,
        actual_value: Some(actual),
        detail: Some(detail),
        brier,
    })
}

/// Numeric 11 Nov arithmetic: reached the target (within tolerance, good
/// direction) is pass; moved toward it is inconclusive; otherwise fail.
/// Mapping onto the headline: pass → true, o=1; fail → false, o=0;
/// inconclusive → false, o=0, with `threshold_verdict` carrying inconclusive.
pub fn compute_numeric(
    record: &PredictionRecord,
    actual_value: f64,
) -> Result<VerdictDetail, Error> {
    check_finite("actual_value", actual_value)?;
    let baseline = record
        .baseline_value
        .ok_or_else(|| Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "baseline_value",
        })?;
    let target = record
        .target_value
        .ok_or_else(|| Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "target_value",
        })?;
    let direction = record
        .direction
        .ok_or_else(|| Error::MissingRequiredField {
            claim_id: record.claim_id.clone(),
            field: "direction",
        })?;
    let tolerance = record.tolerance.unwrap_or(0.0);
    let predicted_change = target - baseline;
    let actual_change = actual_value - baseline;
    let reached = match direction {
        Direction::Down => actual_value <= target + tolerance,
        Direction::Up => actual_value >= target - tolerance,
    };
    let toward = sign(actual_change) == sign(predicted_change) && actual_change != 0.0;
    let threshold_verdict = if reached {
        ThresholdVerdict::Pass
    } else if toward {
        ThresholdVerdict::Inconclusive
    } else {
        ThresholdVerdict::Fail
    };
    let accuracy = if predicted_change == 0.0 {
        None
    } else {
        Some(((actual_change / predicted_change) * 100.0).round() / 100.0)
    };
    Ok(VerdictDetail {
        predicted_change,
        actual_change,
        error: actual_value - target,
        accuracy,
        threshold_verdict,
    })
}

fn sign(x: f64) -> i8 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::freeze::freeze;

    fn binary() -> PredictionRecord {
        serde_json::from_value(serde_json::json!({
            "data_class": "example",
            "data_notice": "EXAMPLE DATA.",
            "claim_id": "ex-bin",
            "forecaster_id": "alice",
            "forecaster_name": "Alice Example",
            "exact_quote": "It will rain.",
            "source_url": "https://example.com/rain",
            "stated_at": "2024-01-01",
            "question_id": "q-1",
            "problem_id": "p-1",
            "decision_id": "d-1",
            "signal_ids": ["s-1"],
            "question": "Will it rain?",
            "population": "Example City",
            "denominator": "one day",
            "threshold": "0.2 mm",
            "resolution_at": "2024-01-03",
            "resolution_rule": "True if rain.",
            "resolution_source": "https://example.com/weather",
            "probability": 0.8,
            "forecast_version": 1
        }))
        .unwrap()
    }

    fn numeric_up() -> PredictionRecord {
        let mut r = binary();
        r.claim_id = "ex-num".into();
        r.metric_id = Some("paid-engagements".into());
        r.metric_unit = Some("count".into());
        r.direction = Some(Direction::Up);
        r.baseline_value = Some(0.0);
        r.baseline_at = Some("2024-01-01".into());
        r.target_value = Some(2.0);
        r.tolerance = Some(0.0);
        r.probability = Some(0.7);
        r
    }

    #[test]
    fn binary_true_and_false() {
        let mut r = binary();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let v = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/obs".into()),
                actual_value: Some(1.0),
                resolved_outcome: Some("rain was recorded".into()),
                void_reason: None,
                caller_verdict: None,
            },
        )
        .unwrap();
        assert_eq!(v, Verdict::True);
        assert!((r.brier.unwrap() - 0.04).abs() < 1e-12);
    }

    #[test]
    fn forecaster_cannot_self_score() {
        let mut r = binary();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let err = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "alice".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/obs".into()),
                actual_value: Some(1.0),
                resolved_outcome: None,
                void_reason: None,
                caller_verdict: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "VerifierIsForecaster");
    }

    #[test]
    fn disagreeing_caller_verdict_is_refused() {
        let mut r = binary();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let err = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/obs".into()),
                actual_value: Some(1.0),
                resolved_outcome: None,
                void_reason: None,
                caller_verdict: Some(Verdict::False),
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "VerdictIsComputed");
        assert!(r.verdict.is_none());
    }

    #[test]
    fn missing_evidence_is_an_error_not_void() {
        let mut r = binary();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let err = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: None,
                actual_value: Some(1.0),
                resolved_outcome: None,
                void_reason: None,
                caller_verdict: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "ActualEvidenceMissing");
    }

    #[test]
    fn void_is_computed_when_the_rule_cannot_be_applied() {
        let mut r = binary();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let v = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/gone".into()),
                actual_value: None,
                resolved_outcome: Some("station withdrawn".into()),
                void_reason: Some("the named source is gone".into()),
                caller_verdict: None,
            },
        )
        .unwrap();
        assert_eq!(v, Verdict::Void);
        assert!(r.brier.is_none());
    }

    #[test]
    fn numeric_pass_mixed_fail_and_tolerance() {
        let up = numeric_up();
        let pass = compute_numeric(&up, 2.0).unwrap();
        assert_eq!(pass.threshold_verdict, ThresholdVerdict::Pass);
        assert_eq!(pass.predicted_change, 2.0);
        assert_eq!(pass.actual_change, 2.0);
        assert_eq!(pass.error, 0.0);
        assert_eq!(pass.accuracy, Some(1.0));

        let mixed = compute_numeric(&up, 1.0).unwrap();
        assert_eq!(mixed.threshold_verdict, ThresholdVerdict::Inconclusive);
        assert_eq!(mixed.error, -1.0);
        assert_eq!(mixed.accuracy, Some(0.5));

        let fail = compute_numeric(&up, 0.0).unwrap();
        assert_eq!(fail.threshold_verdict, ThresholdVerdict::Fail);

        let mut tolerant = up.clone();
        tolerant.tolerance = Some(1.0);
        assert_eq!(
            compute_numeric(&tolerant, 1.0).unwrap().threshold_verdict,
            ThresholdVerdict::Pass
        );

        let mut down = up;
        down.direction = Some(Direction::Down);
        down.baseline_value = Some(60.0);
        down.target_value = Some(30.0);
        assert_eq!(
            compute_numeric(&down, 25.0).unwrap().threshold_verdict,
            ThresholdVerdict::Pass
        );
        assert_eq!(
            compute_numeric(&down, 45.0).unwrap().threshold_verdict,
            ThresholdVerdict::Inconclusive
        );
        assert_eq!(
            compute_numeric(&down, 70.0).unwrap().threshold_verdict,
            ThresholdVerdict::Fail
        );
    }

    #[test]
    fn inconclusive_maps_to_false_with_detail() {
        let mut r = numeric_up();
        freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
        let v = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/obs".into()),
                actual_value: Some(1.0),
                resolved_outcome: None,
                void_reason: None,
                caller_verdict: None,
            },
        )
        .unwrap();
        assert_eq!(v, Verdict::False);
        assert_eq!(
            r.verdict_detail.as_ref().unwrap().threshold_verdict,
            ThresholdVerdict::Inconclusive
        );
        // o = 0, p = 0.7 → (0.7 - 0)^2 = 0.49
        assert!((r.brier.unwrap() - 0.49).abs() < 1e-12);
    }

    #[test]
    fn unfrozen_cannot_be_scored() {
        let mut r = binary();
        let err = record_actual(
            &mut r,
            ActualInput {
                verifier_id: "bob".into(),
                actual_at: "2024-01-03".into(),
                evidence_uri: Some("https://example.com/obs".into()),
                actual_value: Some(1.0),
                resolved_outcome: None,
                void_reason: None,
                caller_verdict: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code(), "UnfrozenPrediction");
    }
}
