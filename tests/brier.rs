//! Binary Brier and Wilson 95% interval, including the lab 0.065 case (S3).

use std::fs;
use std::path::PathBuf;

use prediction_domain::{binary_brier, mean_brier, wilson_95};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[test]
fn lab_case_mean_brier_is_0_065() {
    let scores = [binary_brier(0.8, 1.0), binary_brier(0.3, 0.0)];
    let mean = mean_brier(&scores).unwrap();
    assert!((mean - 0.065).abs() < 1e-12);
}

#[test]
fn lab_fixture_agrees_with_the_same_arithmetic() {
    let path = fixtures_dir().join("brier-lab.json");
    let body = fs::read_to_string(path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut scores = Vec::new();
    for f in v["forecasts"].as_array().unwrap() {
        let p = f["p"].as_f64().unwrap();
        let y = f["y"].as_f64().unwrap();
        scores.push(binary_brier(p, y));
    }
    let expected = v["expected_mean_brier"].as_f64().unwrap();
    assert!((mean_brier(&scores).unwrap() - expected).abs() < 1e-12);

    let w = &v["wilson_one_success_in_one"];
    let interval = wilson_95(1, 1).unwrap();
    assert!((interval.low - w["low"].as_f64().unwrap()).abs() < 1e-8);
    assert!((interval.high - w["high"].as_f64().unwrap()).abs() < 1e-12);

    let w = &v["wilson_zero_in_ten"];
    let interval = wilson_95(0, 10).unwrap();
    assert!((interval.low - w["low"].as_f64().unwrap()).abs() < 1e-12);
    assert!((interval.high - w["high"].as_f64().unwrap()).abs() < 1e-6);
}

#[test]
fn wilson_does_not_turn_one_success_into_certainty() {
    let interval = wilson_95(1, 1).unwrap();
    assert!((interval.low - 0.206_549_314).abs() < 1e-8);
    assert!((interval.high - 1.0).abs() < 1e-12);
    assert!(wilson_95(0, 0).is_none());
}
