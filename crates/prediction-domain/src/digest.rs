//! `prediction_digest`: SHA-256 of a canonical JSON of the frozen fields.
//! `frozen_at` and the digest itself are written together at insert and are
//! not inputs to the hash (so the hash is not circular).

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::record::PredictionRecord;

/// Frozen-field view used as the digest input. Outcome, notices, team forecasts,
/// `frozen_at` and `prediction_digest` are excluded.
#[derive(Serialize)]
struct FrozenView<'a> {
    claim_id: &'a str,
    forecaster_id: &'a str,
    forecaster_name: &'a str,
    exact_quote: &'a str,
    source_url: &'a str,
    stated_at: &'a str,
    question_id: Option<&'a str>,
    problem_id: Option<&'a str>,
    decision_id: Option<&'a str>,
    signal_ids: Option<Value>,
    question: &'a str,
    population: &'a str,
    denominator: &'a str,
    threshold: &'a str,
    metric_id: Option<&'a str>,
    metric_unit: Option<&'a str>,
    direction: Option<Value>,
    baseline_value: Option<f64>,
    baseline_at: Option<&'a str>,
    target_value: Option<f64>,
    tolerance: Option<f64>,
    resolution_at: &'a str,
    resolution_rule: &'a str,
    resolution_source: &'a str,
    probability: Option<f64>,
    basis: Option<&'a str>,
    base_rate_or_reference_class: Option<&'a str>,
    made_at: Option<&'a str>,
    forecast_version: u32,
    key_assumptions: Option<&'a [String]>,
    update_triggers: Option<&'a [String]>,
    supporting_evidence_ids: Option<&'a [String]>,
    contradicting_evidence_ids: Option<&'a [String]>,
}

impl<'a> FrozenView<'a> {
    fn from_record(r: &'a PredictionRecord) -> Self {
        Self {
            claim_id: &r.claim_id,
            forecaster_id: &r.forecaster_id,
            forecaster_name: &r.forecaster_name,
            exact_quote: &r.exact_quote,
            source_url: &r.source_url,
            stated_at: &r.stated_at,
            question_id: r.question_id.as_ref().map(|m| m.as_str()),
            problem_id: r.problem_id.as_ref().map(|m| m.as_str()),
            decision_id: r.decision_id.as_ref().map(|m| m.as_str()),
            signal_ids: r
                .signal_ids
                .as_ref()
                .and_then(|s| serde_json::to_value(s).ok()),
            question: &r.question,
            population: &r.population,
            denominator: &r.denominator,
            threshold: &r.threshold,
            metric_id: r.metric_id.as_deref(),
            metric_unit: r.metric_unit.as_deref(),
            direction: r
                .direction
                .as_ref()
                .and_then(|d| serde_json::to_value(d).ok()),
            baseline_value: r.baseline_value,
            baseline_at: r.baseline_at.as_deref(),
            target_value: r.target_value,
            tolerance: r.tolerance,
            resolution_at: &r.resolution_at,
            resolution_rule: &r.resolution_rule,
            resolution_source: &r.resolution_source,
            probability: r.probability,
            basis: r.basis.as_deref(),
            base_rate_or_reference_class: r.base_rate_or_reference_class.as_deref(),
            made_at: r.made_at.as_deref(),
            forecast_version: r.forecast_version,
            key_assumptions: r.key_assumptions.as_deref(),
            update_triggers: r.update_triggers.as_deref(),
            supporting_evidence_ids: r.supporting_evidence_ids.as_deref(),
            contradicting_evidence_ids: r.contradicting_evidence_ids.as_deref(),
        }
    }
}

/// SHA-256 hex digest of the canonical JSON of frozen fields.
#[must_use]
pub fn prediction_digest(record: &PredictionRecord) -> String {
    let view = FrozenView::from_record(record);
    let value = serde_json::to_value(view).expect("frozen view is always serializable");
    let canonical = canonical_json(&value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Deterministic JSON: object keys sorted, no insignificant whitespace,
/// the same number formatting serde_json uses for finite f64.
fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(&mut out, value);
    out
}

fn write_canonical(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("string")),
        Value::Array(arr) => {
            out.push('[');
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(out, v);
            }
            out.push(']');
        }
        Value::Object(map) => write_object(out, map),
    }
}

fn write_object(out: &mut String, map: &Map<String, Value>) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    out.push('{');
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(k).expect("key"));
        out.push(':');
        write_canonical(out, &map[*k]);
    }
    out.push('}');
}

/// Returns the current digest, or `PredictionMutated` if a stored digest disagrees.
pub fn check_digest(record: &PredictionRecord) -> Result<String, crate::error::Error> {
    let current = prediction_digest(record);
    match record.prediction_digest.as_deref() {
        None => Err(crate::error::Error::UnfrozenPrediction {
            claim_id: record.claim_id.clone(),
        }),
        Some(stored) if stored == current => Ok(current),
        Some(stored) => Err(crate::error::Error::PredictionMutated {
            claim_id: record.claim_id.clone(),
            expected_digest: stored.to_string(),
            actual_digest: current,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::PredictionRecord;

    fn sample() -> PredictionRecord {
        serde_json::from_value(serde_json::json!({
            "data_class": "example",
            "data_notice": "EXAMPLE DATA.",
            "claim_id": "ex-1",
            "forecaster_id": "alice",
            "forecaster_name": "Alice Example",
            "exact_quote": "It will rain.",
            "source_url": "https://example.com/rain",
            "stated_at": "2024-01-01",
            "question_id": "q-1",
            "problem_id": "p-1",
            "decision_id": "d-1",
            "signal_ids": ["s-1"],
            "question": "Will it rain on 2 Jan 2024 in Example City?",
            "population": "Example City official weather station",
            "denominator": "calendar day 2024-01-02",
            "threshold": "at least 0.2 mm recorded precipitation",
            "resolution_at": "2024-01-03",
            "resolution_rule": "True if the official station records >= 0.2 mm.",
            "resolution_source": "https://example.com/weather",
            "probability": 0.8,
            "forecast_version": 1
        }))
        .unwrap()
    }

    #[test]
    fn digest_is_stable_and_hex() {
        let a = prediction_digest(&sample());
        let b = prediction_digest(&sample());
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn digest_changes_when_a_frozen_field_changes() {
        let mut r = sample();
        let first = prediction_digest(&r);
        r.exact_quote = "It will not rain.".into();
        assert_ne!(first, prediction_digest(&r));
    }

    #[test]
    fn digest_ignores_outcome_and_notice() {
        let mut r = sample();
        let first = prediction_digest(&r);
        r.data_notice = Some("EXAMPLE DATA. Different wording.".into());
        r.actual_value = Some(1.0);
        r.frozen_at = Some("2024-01-01T00:00:00Z".into());
        r.prediction_digest = Some("deadbeef".into());
        assert_eq!(first, prediction_digest(&r));
    }
}
