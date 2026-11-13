//! Freeze a record: write `prediction_digest` and `frozen_at`. Refuse mutations.

use crate::digest::{check_digest, prediction_digest};
use crate::error::Error;
use crate::record::PredictionRecord;

/// Freeze `record` at `frozen_at` (ISO 8601). Idempotent when the digest already
/// matches. Refuses if a stored digest does not match the current frozen fields.
pub fn freeze(record: &mut PredictionRecord, frozen_at: &str) -> Result<(), Error> {
    record.validate_frozen_fields()?;
    let digest = prediction_digest(record);
    match record.prediction_digest.as_deref() {
        Some(stored) if stored == digest => {
            if record.frozen_at.is_none() {
                record.frozen_at = Some(frozen_at.to_string());
            }
            Ok(())
        }
        Some(stored) => Err(Error::PredictionMutated {
            claim_id: record.claim_id.clone(),
            expected_digest: stored.to_string(),
            actual_digest: digest,
        }),
        None => {
            record.prediction_digest = Some(digest);
            record.frozen_at = Some(frozen_at.to_string());
            Ok(())
        }
    }
}

/// Confirm the record is frozen and its frozen fields have not changed.
pub fn require_frozen(record: &PredictionRecord) -> Result<String, Error> {
    check_digest(record)
}

/// A claim is scorable only if frozen before its resolution window starts:
/// `frozen_at` precedes `resolution_at` (string compare on ISO 8601 dates).
pub fn frozen_before_resolution(record: &PredictionRecord) -> bool {
    match record.frozen_at.as_deref() {
        Some(frozen) => {
            frozen <= record.resolution_at.as_str()
                || frozen.starts_with(record.resolution_at.as_str())
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::DataClass;

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
    fn freeze_writes_digest_and_is_idempotent() {
        let mut r = sample();
        freeze(&mut r, "2024-01-01T12:00:00Z").unwrap();
        let d = r.prediction_digest.clone().unwrap();
        assert_eq!(d.len(), 64);
        assert_eq!(r.frozen_at.as_deref(), Some("2024-01-01T12:00:00Z"));
        freeze(&mut r, "2024-01-02T00:00:00Z").unwrap();
        assert_eq!(r.prediction_digest.as_deref(), Some(d.as_str()));
        // frozen_at stays the original insert time
        assert_eq!(r.frozen_at.as_deref(), Some("2024-01-01T12:00:00Z"));
    }

    #[test]
    fn mutated_frozen_fields_are_refused() {
        let mut r = sample();
        freeze(&mut r, "2024-01-01T12:00:00Z").unwrap();
        r.exact_quote = "changed".into();
        let err = freeze(&mut r, "2024-01-01T12:00:00Z").unwrap_err();
        assert_eq!(err.code(), "PredictionMutated");
        let err = require_frozen(&r).unwrap_err();
        assert_eq!(err.code(), "PredictionMutated");
    }

    #[test]
    fn unfrozen_is_refused() {
        let r = sample();
        let err = require_frozen(&r).unwrap_err();
        assert_eq!(err.code(), "UnfrozenPrediction");
        assert_eq!(r.data_class, DataClass::Example);
    }
}
