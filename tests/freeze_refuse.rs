//! Freeze, digest mutation, and unfrozen refuse paths (S1).

use prediction_domain::{
    freeze, require_frozen, score_claim, DecisionRecord, MaybeId, PredictionRecord,
};
use serde_json::json;

fn example_claim(id: &str) -> PredictionRecord {
    serde_json::from_value(json!({
        "data_class": "example",
        "data_notice": "EXAMPLE DATA.",
        "claim_id": id,
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

#[test]
fn freeze_writes_digest_and_refuses_a_later_edit() {
    let mut r = example_claim("ex-freeze");
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    assert!(r.prediction_digest.as_ref().unwrap().len() == 64);
    require_frozen(&r).unwrap();
    r.exact_quote = "mutated wording".into();
    let err = require_frozen(&r).unwrap_err();
    assert_eq!(err.code(), "PredictionMutated");
}

#[test]
fn scoring_an_unfrozen_claim_exits_with_unfrozen_prediction() {
    let mut r = example_claim("ex-unfrozen");
    let decision = DecisionRecord {
        data_class: None,
        data_notice: None,
        decision_id: "d-1".into(),
        problem_id: MaybeId("p-1".into()),
        prediction_ids: vec!["ex-unfrozen".into()],
        title: None,
    };
    let err = score_claim(&mut r, Some(&decision), &[]).unwrap_err();
    assert_eq!(err.code(), "UnfrozenPrediction");
}

#[test]
fn decision_without_problem_id_is_refused() {
    let mut r = example_claim("ex-dec");
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    r.actual_value = Some(1.0);
    r.verdict = Some(prediction_domain::Verdict::True);
    r.verifier_id = Some("bob".into());
    let decision = DecisionRecord {
        data_class: None,
        data_notice: None,
        decision_id: "d-1".into(),
        problem_id: MaybeId("[U]".into()),
        prediction_ids: vec!["ex-dec".into()],
        title: None,
    };
    let frozen = vec!["ex-dec".into()];
    let err = score_claim(&mut r, Some(&decision), &frozen).unwrap_err();
    assert_eq!(err.code(), "DecisionMissingProblemId");
}

#[test]
fn decision_without_a_frozen_prediction_is_refused() {
    let mut r = example_claim("ex-dec2");
    freeze(&mut r, "2024-01-01T00:00:00Z").unwrap();
    r.actual_value = Some(1.0);
    r.verdict = Some(prediction_domain::Verdict::True);
    r.verifier_id = Some("bob".into());
    let decision = DecisionRecord {
        data_class: None,
        data_notice: None,
        decision_id: "d-1".into(),
        problem_id: MaybeId("p-1".into()),
        prediction_ids: vec!["some-other-claim".into()],
        title: None,
    };
    let frozen = vec!["ex-dec2".into()];
    let err = score_claim(&mut r, Some(&decision), &frozen).unwrap_err();
    assert_eq!(err.code(), "DecisionMissingFrozenPrediction");
}

#[test]
fn example_data_must_be_labelled() {
    let mut r = example_claim("ex-label");
    r.data_notice = Some("a fixture with no label".into());
    let err = freeze(&mut r, "2024-01-01T00:00:00Z").unwrap_err();
    assert_eq!(err.code(), "MissingRequiredField");
}
