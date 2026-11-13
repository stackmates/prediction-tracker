//! Error codes named by the tracker spec. Display prints the code first.

use std::fmt;

/// Domain failure. The CLI exits non-zero and prints [`Error::code`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    PredictionMutated {
        claim_id: String,
        expected_digest: String,
        actual_digest: String,
    },
    UnfrozenPrediction {
        claim_id: String,
    },
    VerifierIsForecaster {
        id: String,
    },
    ActualEvidenceMissing {
        claim_id: String,
    },
    VerdictIsComputed {
        claim_id: String,
    },
    DecisionMissingProblemId {
        decision_id: String,
    },
    DecisionMissingFrozenPrediction {
        decision_id: String,
    },
    MissingRequiredField {
        claim_id: String,
        field: &'static str,
    },
    InvalidProbability {
        reason: String,
    },
    NonHttpsEvidenceUri {
        uri: String,
    },
    InputTooLong {
        field: &'static str,
        max: usize,
        got: usize,
    },
    InvalidNumeric {
        field: &'static str,
        reason: String,
    },
    TargetEqualsBaseline,
    InvalidDirection {
        value: String,
    },
    ActualAlreadyRecorded {
        claim_id: String,
    },
    NotInBrierSet {
        claim_id: String,
        reason: String,
    },
}

impl Error {
    /// Stable machine-readable code, used as the CLI exit message.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::PredictionMutated { .. } => "PredictionMutated",
            Self::UnfrozenPrediction { .. } => "UnfrozenPrediction",
            Self::VerifierIsForecaster { .. } => "VerifierIsForecaster",
            Self::ActualEvidenceMissing { .. } => "ActualEvidenceMissing",
            Self::VerdictIsComputed { .. } => "VerdictIsComputed",
            Self::DecisionMissingProblemId { .. } => "DecisionMissingProblemId",
            Self::DecisionMissingFrozenPrediction { .. } => "DecisionMissingFrozenPrediction",
            Self::MissingRequiredField { .. } => "MissingRequiredField",
            Self::InvalidProbability { .. } => "InvalidProbability",
            Self::NonHttpsEvidenceUri { .. } => "NonHttpsEvidenceUri",
            Self::InputTooLong { .. } => "InputTooLong",
            Self::InvalidNumeric { .. } => "InvalidNumeric",
            Self::TargetEqualsBaseline => "TargetEqualsBaseline",
            Self::InvalidDirection { .. } => "InvalidDirection",
            Self::ActualAlreadyRecorded { .. } => "ActualAlreadyRecorded",
            Self::NotInBrierSet { .. } => "NotInBrierSet",
        }
    }

    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::PredictionMutated {
                claim_id,
                expected_digest,
                actual_digest,
            } => format!(
                "claim {claim_id}: frozen digest {expected_digest} does not match current frozen fields {actual_digest}"
            ),
            Self::UnfrozenPrediction { claim_id } => {
                format!("claim {claim_id} has no prediction_digest; freeze it first")
            }
            Self::VerifierIsForecaster { id } => {
                format!("verifier_id {id} must differ from forecaster_id")
            }
            Self::ActualEvidenceMissing { claim_id } => {
                format!("claim {claim_id}: a measured actual needs a public evidence URI")
            }
            Self::VerdictIsComputed { claim_id } => {
                format!("claim {claim_id}: the program computes the verdict; a caller-supplied value that disagrees is refused")
            }
            Self::DecisionMissingProblemId { decision_id } => {
                format!("decision {decision_id} has no problem_id")
            }
            Self::DecisionMissingFrozenPrediction { decision_id } => {
                format!("decision {decision_id} has no frozen prediction behind it")
            }
            Self::MissingRequiredField { claim_id, field } => {
                format!("claim {claim_id}: missing required field {field}")
            }
            Self::InvalidProbability { reason } => reason.clone(),
            Self::NonHttpsEvidenceUri { uri } => {
                format!("public evidence and source URIs must be https: {uri}")
            }
            Self::InputTooLong { field, max, got } => {
                format!("{field} is {got} bytes; max is {max}")
            }
            Self::InvalidNumeric { field, reason } => format!("{field}: {reason}"),
            Self::TargetEqualsBaseline => {
                "target_value must differ from baseline_value when both are set".into()
            }
            Self::InvalidDirection { value } => {
                format!("direction must be up or down, got {value}")
            }
            Self::ActualAlreadyRecorded { claim_id } => {
                format!("claim {claim_id} already has a recorded actual")
            }
            Self::NotInBrierSet { claim_id, reason } => format!("claim {claim_id}: {reason}"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.detail())
    }
}

impl std::error::Error for Error {}
