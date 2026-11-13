//! Filesystem-backed store for prediction and decision records.
//! This is the only driven adapter. S1–S5 use the filesystem; there is no
//! SQL and no HTTP. If sqlite is added later, it stays in this crate.

use std::fs;
use std::path::{Path, PathBuf};

use prediction_domain::{DecisionRecord, Error as DomainError, PredictionRecord, MAX_FILE_BYTES};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("io: {0}")]
    Io(String),
    #[error("parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error("file {path} is {got} bytes; max is {max}")]
    FileTooLarge {
        path: String,
        max: usize,
        got: usize,
    },
    #[error("claim {0} not found")]
    ClaimNotFound(String),
}

impl StoreError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Domain(e) => e.code(),
            Self::Io(_) => "StoreIo",
            Self::Parse { .. } => "StoreParse",
            Self::FileTooLarge { .. } => "InputTooLong",
            Self::ClaimNotFound(_) => "ClaimNotFound",
        }
    }
}

/// A claim together with the file it was loaded from, so it can be written back.
#[derive(Debug, Clone)]
pub struct LoadedClaim {
    pub path: PathBuf,
    pub record: PredictionRecord,
}

/// Filesystem store rooted at a claims directory. Decision files live in a
/// `decisions/` subdirectory of the same root, if present, or a sibling
/// `decisions/` directory.
pub struct Store {
    root: PathBuf,
}

impl Store {
    #[must_use]
    pub fn open(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load_claims(&self) -> Result<Vec<LoadedClaim>, StoreError> {
        let mut out = Vec::new();
        visit_json(&self.root, &mut |path| {
            if is_under(path, "decisions") {
                return Ok(());
            }
            let record = read_claim(path)?;
            out.push(LoadedClaim {
                path: path.to_path_buf(),
                record,
            });
            Ok(())
        })?;
        out.sort_by(|a, b| a.record.claim_id.cmp(&b.record.claim_id));
        Ok(out)
    }

    pub fn load_decisions(&self) -> Result<Vec<DecisionRecord>, StoreError> {
        let mut dirs = Vec::new();
        let nested = self.root.join("decisions");
        if nested.is_dir() {
            dirs.push(nested);
        }
        if let Some(parent) = self.root.parent() {
            let sibling = parent.join("decisions");
            if sibling.is_dir() && !dirs.contains(&sibling) {
                dirs.push(sibling);
            }
        }
        let mut out = Vec::new();
        for dir in dirs {
            visit_json(&dir, &mut |path| {
                out.push(read_decision(path)?);
                Ok(())
            })?;
        }
        out.sort_by(|a, b| a.decision_id.cmp(&b.decision_id));
        Ok(out)
    }

    pub fn find_claim(&self, claim_id: &str) -> Result<LoadedClaim, StoreError> {
        self.load_claims()?
            .into_iter()
            .find(|c| c.record.claim_id == claim_id)
            .ok_or_else(|| StoreError::ClaimNotFound(claim_id.to_string()))
    }

    pub fn save_claim(&self, loaded: &LoadedClaim) -> Result<(), StoreError> {
        write_json(&loaded.path, &loaded.record)
    }

    pub fn frozen_prediction_ids(&self) -> Result<Vec<String>, StoreError> {
        Ok(self
            .load_claims()?
            .into_iter()
            .filter(|c| c.record.prediction_digest.is_some())
            .map(|c| c.record.claim_id)
            .collect())
    }
}

fn is_under(path: &Path, folder: &str) -> bool {
    path.components().any(|c| c.as_os_str() == folder)
}

fn visit_json(
    dir: &Path,
    visit: &mut impl FnMut(&Path) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    if !dir.exists() {
        return Ok(());
    }
    let entries =
        fs::read_dir(dir).map_err(|e| StoreError::Io(format!("{}: {e}", dir.display())))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::Io(e.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            visit_json(&path, visit)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    for path in paths {
        visit(&path)?;
    }
    Ok(())
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, StoreError> {
    let bytes = fs::read(path).map_err(|e| StoreError::Io(format!("{}: {e}", path.display())))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(StoreError::FileTooLarge {
            path: path.display().to_string(),
            max: MAX_FILE_BYTES,
            got: bytes.len(),
        });
    }
    Ok(bytes)
}

fn read_claim(path: &Path) -> Result<PredictionRecord, StoreError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|e| StoreError::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn read_decision(path: &Path) -> Result<DecisionRecord, StoreError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|e| StoreError::Parse {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), StoreError> {
    let mut body =
        serde_json::to_string_pretty(value).map_err(|e| StoreError::Io(e.to_string()))?;
    body.push('\n');
    if body.len() > MAX_FILE_BYTES {
        return Err(StoreError::FileTooLarge {
            path: path.display().to_string(),
            max: MAX_FILE_BYTES,
            got: body.len(),
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
    }
    fs::write(path, body).map_err(|e| StoreError::Io(format!("{}: {e}", path.display())))
}
