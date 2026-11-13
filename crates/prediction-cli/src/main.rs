//! Driving adapter for the file-based prediction tracker.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use prediction_domain::{
    calibration_deciles, freeze, record_actual, require_frozen, score_by_source, score_claim,
    ActualInput, Verdict,
};
use prediction_store::{Store, StoreError};

#[derive(Parser, Debug)]
#[command(
    name = "prediction-tracker",
    about = "Freeze a public prediction, record a measured actual, compute a true/false/void verdict, and score with binary Brier and a Wilson 95% interval.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Write prediction_digest and frozen_at. Refuses mutated frozen fields.
    Freeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        claim: Option<String>,
        /// ISO 8601 timestamp written as frozen_at. Required so inserts are reproducible.
        #[arg(long)]
        frozen_at: String,
    },
    /// Record a measured actual. The program computes the verdict. Verifier ≠ forecaster.
    RecordActual {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        claim: String,
        #[arg(long)]
        verifier_id: String,
        #[arg(long)]
        actual_at: String,
        #[arg(long)]
        actual_evidence_uri: String,
        #[arg(long)]
        actual_value: Option<f64>,
        #[arg(long)]
        resolved_outcome: Option<String>,
        /// Evidence that the named source is gone or the frozen rule cannot be applied.
        /// The program then sets verdict void. This is not a home for "almost."
        #[arg(long)]
        void_reason: Option<String>,
    },
    /// Score a frozen, resolved claim. Exits non-zero with PredictionMutated or
    /// UnfrozenPrediction when the digest is missing or disagrees.
    Score {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        claim: String,
    },
    /// List frozen claims, per-source Brier, Wilson intervals, and voids counted separately.
    Report {
        #[arg(long)]
        input: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Freeze {
            input,
            claim,
            frozen_at,
        } => cmd_freeze(input, claim, &frozen_at)?,
        Command::RecordActual {
            input,
            claim,
            verifier_id,
            actual_at,
            actual_evidence_uri,
            actual_value,
            resolved_outcome,
            void_reason,
        } => cmd_record_actual(
            input,
            &claim,
            ActualInput {
                verifier_id,
                actual_at,
                evidence_uri: Some(actual_evidence_uri),
                actual_value,
                resolved_outcome,
                void_reason,
                caller_verdict: None,
            },
        )?,
        Command::Score { input, claim } => cmd_score(input, &claim)?,
        Command::Report { input } => cmd_report(input)?,
    }
    Ok(())
}

fn cmd_freeze(
    input: PathBuf,
    claim: Option<String>,
    frozen_at: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(input);
    let mut loaded = store.load_claims()?;
    let mut n = 0;
    for item in &mut loaded {
        if let Some(id) = claim.as_deref() {
            if item.record.claim_id != id {
                continue;
            }
        }
        freeze(&mut item.record, frozen_at).map_err(store_domain)?;
        store.save_claim(item)?;
        n += 1;
        println!(
            "froze {} digest {}",
            item.record.claim_id,
            item.record.prediction_digest.as_deref().unwrap_or("")
        );
    }
    if let Some(id) = claim {
        if n == 0 {
            return Err(StoreError::ClaimNotFound(id).into());
        }
    }
    Ok(())
}

fn cmd_record_actual(
    input: PathBuf,
    claim: &str,
    actual: ActualInput,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(input);
    let mut item = store.find_claim(claim)?;
    let verdict = record_actual(&mut item.record, actual).map_err(store_domain)?;
    store.save_claim(&item)?;
    let verdict_word = match verdict {
        Verdict::True => "true",
        Verdict::False => "false",
        Verdict::Void => "void",
    };
    print!("claim {claim} verdict {verdict_word}");
    if let Some(b) = item.record.brier {
        print!(" brier {b}");
    }
    println!();
    Ok(())
}

fn cmd_score(input: PathBuf, claim: &str) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(input);
    let mut item = store.find_claim(claim)?;
    require_frozen(&item.record).map_err(store_domain)?;
    let decisions = store.load_decisions()?;
    let frozen_ids = store.frozen_prediction_ids()?;
    let decision = item
        .record
        .decision_id
        .as_ref()
        .and_then(|m| m.known())
        .and_then(|id| decisions.iter().find(|d| d.decision_id == id));
    let brier = score_claim(&mut item.record, decision, &frozen_ids).map_err(store_domain)?;
    store.save_claim(&item)?;
    match item.record.verdict {
        Some(Verdict::True) => print!("claim {claim} verdict true"),
        Some(Verdict::False) => print!("claim {claim} verdict false"),
        Some(Verdict::Void) => {
            print!("claim {claim} verdict void (counted separately, not in Brier)")
        }
        None => print!("claim {claim} unresolved"),
    }
    match brier {
        Some(b) => println!(" brier {b}"),
        None => println!(),
    }
    Ok(())
}

fn cmd_report(input: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(input);
    let claims = store.load_claims()?;
    let records: Vec<_> = claims.iter().map(|c| &c.record).collect();
    let sources = score_by_source(&records);
    let calib = calibration_deciles(&records);
    let decisions = store.load_decisions()?;

    let mut report = serde_json::Map::new();
    report.insert(
        "provenance".into(),
        serde_json::json!("throwaway / local fixtures"),
    );
    report.insert("n_claims".into(), serde_json::json!(records.len()));
    report.insert(
        "n_frozen".into(),
        serde_json::json!(records
            .iter()
            .filter(|r| r.prediction_digest.is_some())
            .count()),
    );
    report.insert(
        "n_scored".into(),
        serde_json::json!(records
            .iter()
            .filter(|r| r.verdict.is_some()
                && r.verdict != Some(Verdict::Void)
                && r.verifier_id.as_deref() != Some(r.forecaster_id.as_str()))
            .count()),
    );
    report.insert(
        "n_void".into(),
        serde_json::json!(records
            .iter()
            .filter(|r| r.verdict == Some(Verdict::Void))
            .count()),
    );
    report.insert(
        "n_example".into(),
        serde_json::json!(records.iter().filter(|r| r.is_example()).count()),
    );
    report.insert(
        "n_public_sourced".into(),
        serde_json::json!(records.iter().filter(|r| !r.is_example()).count()),
    );

    let listed: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "claim_id": r.claim_id,
                "data_class": r.data_class,
                "forecaster_id": r.forecaster_id,
                "forecaster_name": r.forecaster_name,
                "question_id": r.question_id.as_ref().map(|m| m.as_str()),
                "problem_id": r.problem_id.as_ref().map(|m| m.as_str()),
                "decision_id": r.decision_id.as_ref().map(|m| m.as_str()),
                "prediction_digest": r.prediction_digest,
                "frozen_at": r.frozen_at,
                "probability": r.probability,
                "verdict": r.verdict,
                "verifier_id": r.verifier_id,
                "brier": r.brier,
                "resolution_at": r.resolution_at,
                "data_notice": r.data_notice,
            })
        })
        .collect();
    report.insert("claims".into(), serde_json::Value::Array(listed));
    report.insert("sources".into(), serde_json::to_value(&sources)?);
    report.insert("calibration_deciles".into(), serde_json::to_value(&calib)?);
    report.insert(
        "decisions".into(),
        serde_json::to_value(
            decisions
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "decision_id": d.decision_id,
                        "problem_id": d.problem_id.as_str(),
                        "prediction_ids": d.prediction_ids,
                    })
                })
                .collect::<Vec<_>>(),
        )?,
    );
    report.insert(
        "scoring_rule".into(),
        serde_json::json!({
            "brier": "(p - y)^2 on resolved, non-void, Brier-set claims; lower is better; 0 is perfect, 0.25 is a coin",
            "aggregate": "mean of per-prediction Brier, reported with n",
            "wilson": "95% two-sided Wilson interval on the hit rate, z = 1.959963984540054",
            "void": "counted separately, never dropped, excluded from Brier and calibration",
            "ranking": "a source with scored n below 10 is not ranked",
        }),
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::Value::Object(report))?
    );
    Ok(())
}

fn store_domain(e: prediction_domain::Error) -> StoreError {
    StoreError::Domain(e)
}
