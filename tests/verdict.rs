//! Program-computed true/false/void, verifier rule, and numeric mapping (S2).

use prediction_domain::{
    compute_numeric, freeze, record_actual, ActualInput, Direction, PredictionRecord,
    ThresholdVerdict, Verdict,
};
use serde_json::json;

fn binary() -> PredictionRecord {
    serde_json::from_value(json!({
        "data_class": "example",
        "data_notice": "EXAMPLE DATA.",
        "claim_id": "ex-v",
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

fn input(
    verifier: &str,
    value: Option<f64>,
    void: Option<&str>,
    caller: Option<Verdict>,
) -> ActualInput {
    ActualInput {
        verifier_id: verifier.into(),
        actual_at: "2024-01-03".into(),
        evidence_uri: Some("https://example.com/obs".into()),
        actual_value: value,
        resolved_outcome: Some("recorded".into()),
        void_reason: void.map(str::to_string),
        caller_verdict: caller,
    }
}

#[test]
fn program_sets_true_false_and_void() {
    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert_eq!(
        record_actual(&mut r, input("bob", Some(1.0), None, None)).unwrap(),
        Verdict::True
    );

    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert_eq!(
        record_actual(&mut r, input("bob", Some(0.0), None, None)).unwrap(),
        Verdict::False
    );

    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert_eq!(
        record_actual(
            &mut r,
            input("bob", None, Some("named source is gone"), None)
        )
        .unwrap(),
        Verdict::Void
    );
    assert!(r.brier.is_none());
}

#[test]
fn verifier_must_differ_and_evidence_is_required() {
    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert_eq!(
        record_actual(&mut r, input("alice", Some(1.0), None, None))
            .unwrap_err()
            .code(),
        "VerifierIsForecaster"
    );

    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    let mut missing = input("bob", Some(1.0), None, None);
    missing.evidence_uri = None;
    assert_eq!(
        record_actual(&mut r, missing).unwrap_err().code(),
        "ActualEvidenceMissing"
    );
}

#[test]
fn caller_supplied_disagreeing_verdict_is_refused() {
    let mut r = binary();
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert_eq!(
        record_actual(&mut r, input("bob", Some(1.0), None, Some(Verdict::False)))
            .unwrap_err()
            .code(),
        "VerdictIsComputed"
    );
}

#[test]
fn numeric_mapping_pass_inconclusive_fail() {
    let mut r = binary();
    r.metric_id = Some("n".into());
    r.direction = Some(Direction::Up);
    r.baseline_value = Some(0.0);
    r.target_value = Some(2.0);
    r.tolerance = Some(0.0);
    let pass = compute_numeric(&r, 2.0).unwrap();
    assert_eq!(pass.threshold_verdict, ThresholdVerdict::Pass);
    let mixed = compute_numeric(&r, 1.0).unwrap();
    assert_eq!(mixed.threshold_verdict, ThresholdVerdict::Inconclusive);
    let fail = compute_numeric(&r, 0.0).unwrap();
    assert_eq!(fail.threshold_verdict, ThresholdVerdict::Fail);
}
