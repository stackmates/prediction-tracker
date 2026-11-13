//! Prediction, decision and outcome record types. File-shaped for S1–S5.

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::limits::{
    check_finite, check_https_uri, check_len, check_probability, MAX_ID_LEN, MAX_QUOTE_LEN,
    MAX_TEXT_LEN,
};

/// How a fixture entered the tree. Public-sourced claims come from a fetched URL;
/// example data is invented and must stay labelled as such.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataClass {
    #[serde(rename = "public-sourced")]
    PublicSourced,
    Example,
}

/// Settlement vocabulary, matching `core_prediction_settlement.outcome`: true, false, void.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    True,
    False,
    Void,
}

/// Numeric threshold headline carried only in `verdict_detail`, never as the claim verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdVerdict {
    Pass,
    Inconclusive,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
}

/// A string id that may honestly be unknown (`[U]`). Unknown is not invented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaybeId(pub String);

impl MaybeId {
    #[must_use]
    pub fn known(&self) -> Option<&str> {
        known_str(&self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Signal ids: a list, or a single unknown marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SignalIds {
    List(Vec<String>),
    One(String),
}

impl SignalIds {
    #[must_use]
    pub fn known(&self) -> Vec<&str> {
        match self {
            Self::List(v) => v.iter().filter_map(|s| known_str(s)).collect(),
            Self::One(s) => known_str(s).into_iter().collect(),
        }
    }
}

#[must_use]
pub fn known_str(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() || t == "[U]" || t.starts_with("[U]") {
        None
    } else {
        Some(s)
    }
}

/// Numeric 11 Nov arithmetic, plus the threshold headline that mapped onto the settlement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictDetail {
    pub predicted_change: f64,
    pub actual_change: f64,
    pub error: f64,
    pub accuracy: Option<f64>,
    pub threshold_verdict: ThresholdVerdict,
}

/// A team forecast recorded against a public-sourced claim. This is not the
/// original source's probability and does not put the source's row in the Brier set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamForecast {
    pub forecaster_id: String,
    pub probability: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One prediction record: frozen fields, optional probability, optional outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictionRecord {
    pub data_class: DataClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_notice: Option<String>,
    pub claim_id: String,
    pub forecaster_id: String,
    pub forecaster_name: String,
    pub exact_quote: String,
    pub source_url: String,
    pub stated_at: String,
    #[serde(default)]
    pub question_id: Option<MaybeId>,
    #[serde(default)]
    pub problem_id: Option<MaybeId>,
    #[serde(default)]
    pub decision_id: Option<MaybeId>,
    #[serde(default)]
    pub signal_ids: Option<SignalIds>,
    pub question: String,
    pub population: String,
    pub denominator: String,
    pub threshold: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
    pub resolution_at: String,
    pub resolution_rule: String,
    pub resolution_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_rate_or_reference_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made_at: Option<String>,
    #[serde(default = "default_version")]
    pub forecast_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_assumptions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_triggers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supporting_evidence_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradicting_evidence_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_forecasts: Option<Vec<TeamForecast>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prediction_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_evidence_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_detail: Option<VerdictDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_outcome: Option<String>,
    /// When true, the program will set verdict `void`. The verifier supplies
    /// the fact that the named source is gone or the frozen rule cannot be applied;
    /// the program, not the caller, still sets the verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub void_requested: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub void_reason: Option<String>,
}

fn default_version() -> u32 {
    1
}

impl PredictionRecord {
    /// A claim is in the Brier set only when it has a frozen probability and a
    /// non-void settlement with `o` in {0, 1}.
    #[must_use]
    pub fn in_brier_set(&self) -> bool {
        self.probability.is_some()
            && self.verdict.is_some()
            && self.verdict != Some(Verdict::Void)
            && self.prediction_digest.is_some()
    }

    #[must_use]
    pub fn is_example(&self) -> bool {
        self.data_class == DataClass::Example
    }

    #[must_use]
    pub fn is_binary_event(&self) -> bool {
        self.metric_id.is_none() && self.target_value.is_none() && self.baseline_value.is_none()
    }

    /// Binary outcome `o` used for Brier: 1 if true, 0 if false, None if void or unresolved.
    #[must_use]
    pub fn brier_outcome(&self) -> Option<f64> {
        match self.verdict {
            Some(Verdict::True) => Some(1.0),
            Some(Verdict::False) => Some(0.0),
            Some(Verdict::Void) | None => None,
        }
    }

    pub fn validate_frozen_fields(&self) -> Result<(), Error> {
        let id = self.claim_id.as_str();
        for (field, value) in [
            ("claim_id", self.claim_id.as_str()),
            ("forecaster_id", self.forecaster_id.as_str()),
            ("forecaster_name", self.forecaster_name.as_str()),
            ("stated_at", self.stated_at.as_str()),
            ("resolution_at", self.resolution_at.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(Error::MissingRequiredField {
                    claim_id: id.into(),
                    field,
                });
            }
            check_len(field, value, MAX_ID_LEN.max(64))?;
        }
        check_len("claim_id", &self.claim_id, MAX_ID_LEN)?;
        check_len("forecaster_id", &self.forecaster_id, MAX_ID_LEN)?;
        check_len("forecaster_name", &self.forecaster_name, MAX_ID_LEN)?;
        if self.exact_quote.trim().is_empty() {
            return Err(Error::MissingRequiredField {
                claim_id: id.into(),
                field: "exact_quote",
            });
        }
        check_len("exact_quote", &self.exact_quote, MAX_QUOTE_LEN)?;
        check_https_uri("source_url", &self.source_url)?;
        for (field, value) in [
            ("question", self.question.as_str()),
            ("population", self.population.as_str()),
            ("denominator", self.denominator.as_str()),
            ("threshold", self.threshold.as_str()),
            ("resolution_rule", self.resolution_rule.as_str()),
            ("resolution_source", self.resolution_source.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(Error::MissingRequiredField {
                    claim_id: id.into(),
                    field,
                });
            }
            check_len(field, value, MAX_TEXT_LEN)?;
        }
        if let Some(p) = self.probability {
            check_probability(p)?;
        }
        if let Some(b) = self.baseline_value {
            check_finite("baseline_value", b)?;
        }
        if let Some(t) = self.target_value {
            check_finite("target_value", t)?;
        }
        if let Some(t) = self.tolerance {
            check_finite("tolerance", t)?;
            if t < 0.0 {
                return Err(Error::InvalidNumeric {
                    field: "tolerance",
                    reason: "must be non-negative".into(),
                });
            }
        }
        if let (Some(b), Some(t)) = (self.baseline_value, self.target_value) {
            if (b - t).abs() == 0.0 {
                return Err(Error::TargetEqualsBaseline);
            }
        }
        if let Some(notice) = &self.data_notice {
            check_len("data_notice", notice, MAX_TEXT_LEN)?;
        }
        if self.data_class == DataClass::Example {
            let notice = self.data_notice.as_deref().unwrap_or("");
            if !notice.to_ascii_uppercase().contains("EXAMPLE") {
                return Err(Error::MissingRequiredField {
                    claim_id: id.into(),
                    field: "data_notice (example data must be labelled EXAMPLE)",
                });
            }
        }
        Ok(())
    }
}

/// A decision record: refused without a problem_id and at least one frozen prediction id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_class: Option<DataClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_notice: Option<String>,
    pub decision_id: String,
    pub problem_id: MaybeId,
    pub prediction_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl DecisionRecord {
    pub fn validate_for_score(&self, frozen_prediction_ids: &[String]) -> Result<(), Error> {
        if self.problem_id.known().is_none() {
            return Err(Error::DecisionMissingProblemId {
                decision_id: self.decision_id.clone(),
            });
        }
        let has_frozen = self
            .prediction_ids
            .iter()
            .any(|id| frozen_prediction_ids.iter().any(|f| f == id));
        if !has_frozen {
            return Err(Error::DecisionMissingFrozenPrediction {
                decision_id: self.decision_id.clone(),
            });
        }
        Ok(())
    }
}
