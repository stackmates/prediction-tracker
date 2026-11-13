# prediction-tracker

A file-based scored-prediction tracker. Freeze a claim with its exact words, source, criteria and deadline; record a measured actual from a named public source; let the program compute a `true`, `false` or `void` verdict; and score resolved forecasts with the binary Brier score and a Wilson 95% interval.

This is a local CLI and a public dataset. It does not talk to a production database.

## Crates

Hexagonal layout:

| Crate | Role |
| --- | --- |
| `prediction-domain` | Pure: record, digest, freeze rules, verdict, Brier, Wilson. **No I/O.** |
| `prediction-store` | The only driven adapter. Owns persistence. Today that is the filesystem; there is no SQL and no HTTP. If sqlite is added later, it stays in this crate. |
| `prediction-cli` | Driving adapter: `freeze`, `record-actual`, `score`, `report`. |

## Scoring rule

- Settlement vocabulary is `true`, `false`, or `void`, computed by the program. The forecaster cannot self-score. Void means the named source is gone or the frozen rule cannot be applied without rewriting it. Void is not a home for "almost." Void is counted separately, never dropped, and is excluded from Brier and calibration.
- Per-prediction binary Brier is `(p − y)²` on the 0–1 scale, lower is better (0 is perfect, 0.25 is a coin). Only resolved, non-void claims with a frozen probability are in the Brier set.
- Per-source aggregate is the mean of those scores, reported with `n`.
- Calibration uses the Wilson score interval at 95% (`z = 1.959963984540054`) on the hit rate in each probability decile. Every bucket shows `n` and the interval.
- A source whose scored `n` is below 10 is not ranked.
- A claim with no frozen `p` is verdict-only and is not in the Brier set.

Numeric claims also run a reach-the-target arithmetic (pass / inconclusive / fail). That maps onto the headline as pass → `true`; fail or inconclusive → `false`, with inconclusive kept in `verdict_detail`.

## Build and test

Requires Rust 1.85.

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## CLI

```bash
# Write prediction_digest and frozen_at (idempotent if the digest already matches).
prediction-tracker freeze --input fixtures/public-claims --frozen-at 2026-11-14T08:22:00+13:00

# Record a measured actual. The program computes the verdict. Verifier ≠ forecaster.
prediction-tracker record-actual \
  --input fixtures/public-claims \
  --claim example-rain-city-alpha \
  --verifier-id example-verifier \
  --actual-at 2024-01-03 \
  --actual-evidence-uri https://example.com/weather/example-city/2024-01-02 \
  --actual-value 1

# Score a frozen resolved claim. Exits non-zero with PredictionMutated or
# UnfrozenPrediction if the digest is missing or the frozen fields changed.
prediction-tracker score --input fixtures/public-claims --claim example-rain-city-alpha

# Report: every claim with its digest and links, per-source Brier, Wilson intervals.
prediction-tracker report --input fixtures/public-claims
```

The report provenance field is `"throwaway / local fixtures"`.

Refuse paths (non-zero exit, message starts with the code): `PredictionMutated`, `UnfrozenPrediction`, `VerifierIsForecaster`, `ActualEvidenceMissing`, `VerdictIsComputed`, `DecisionMissingProblemId`, `DecisionMissingFrozenPrediction`.

Input limits, set in S1: source and evidence URLs at most 2,048 bytes and must be `https://`; exact quotes at most 8,192 bytes; long text fields at most 16,384 bytes; a claim file at most 65,536 bytes.

## Fixtures

`fixtures/public-claims/` holds 20 claims:

- **9 public-sourced** (`data_class: public-sourced`) from already-published URLs (METR, Epoch AI, AI Futures Project, Daron Acemoglu, Erik Brynjolfsson). These are real quotes and criteria. Where the source did not state a probability word for word, `probability` is omitted and the row is not in the Brier set. Link fields marked `[U]` are unknown and were not invented; those rows are reading-list items and are not scored. A tenth source slot is empty under the selection rule and is not filled with a substitute.
- **11 example** (`data_class: example`), clearly labelled `EXAMPLE DATA`. Invented for CLI demonstration. Not real forecasts, not from a real source, not measured public outcomes. These carry complete question, problem and decision links. At least three are scored by a verifier other than the forecaster, including one void, so the report and refuse paths can be exercised.

`fixtures/brier-lab.json` is the published binary-Brier case whose mean is 0.065. `fixtures/schema-shape.json` lists the record fields.

Private venture bets do not belong in this tree. Seed data is either public-sourced or labelled example data.

## Licence

MIT. See [LICENSE](LICENSE).
