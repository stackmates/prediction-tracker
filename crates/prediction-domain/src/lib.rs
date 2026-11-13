//! Pure prediction domain: records, freeze, digest, verdict, binary Brier,
//! and the Wilson 95% interval. This crate has no I/O — no filesystem, no
//! network, no SQL.

pub mod digest;
pub mod error;
pub mod freeze;
pub mod limits;
pub mod record;
pub mod score;
pub mod verdict;

pub use digest::{check_digest, prediction_digest};
pub use error::Error;
pub use freeze::{freeze, frozen_before_resolution, require_frozen};
pub use limits::{MAX_FILE_BYTES, MAX_QUOTE_LEN, MAX_TEXT_LEN, MAX_URL_LEN};
pub use record::{
    known_str, DataClass, DecisionRecord, Direction, MaybeId, PredictionRecord, SignalIds,
    TeamForecast, ThresholdVerdict, Verdict, VerdictDetail,
};
pub use score::{
    binary_brier, calibration_deciles, constant_forecaster_brier, mean_brier, score_by_source,
    score_claim, skill, wilson_95, CalibrationBucket, Interval, SourceScore, COIN_BRIER, WILSON_Z,
};
pub use verdict::{compute_numeric, record_actual, ActualInput};
