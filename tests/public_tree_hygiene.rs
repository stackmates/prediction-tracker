//! Fail the build if the public tree contains secrets, internal paths, a
//! not-for-redistribution licence line, or the two personal names that must
//! not appear. Every forbidden literal is assembled at runtime so this file
//! itself does not contain the patterns.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str();
        s == "target" || s == ".git" || s == ".idea" || s == ".cargo"
    })
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if skip(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

fn word_contains(hay_lower: &str, needle_lower: &str) -> bool {
    let mut start = 0;
    while let Some(i) = hay_lower[start..].find(needle_lower) {
        let at = start + i;
        let before = at
            .checked_sub(1)
            .and_then(|j| hay_lower[j..].chars().next());
        let after = hay_lower[at + needle_lower.len()..].chars().next();
        let b_ok = before.map(|c| !c.is_ascii_alphanumeric()).unwrap_or(true);
        let a_ok = after.map(|c| !c.is_ascii_alphanumeric()).unwrap_or(true);
        if b_ok && a_ok {
            return true;
        }
        start = at + 1;
        if start >= hay_lower.len() {
            break;
        }
    }
    false
}

fn has_akia(text: &str) -> bool {
    let bytes = text.as_bytes();
    let prefix = [b'A', b'K', b'I', b'A'];
    let mut i = 0;
    while i + 20 <= bytes.len() {
        if bytes[i..i + 4] == prefix
            && bytes[i + 4..i + 20]
                .iter()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return true;
        }
        i += 1;
    }
    false
}

fn has_pem_private_key(text: &str) -> bool {
    let begin = format!("{}{}", "BEGIN ", "PRIVATE KEY");
    let rsa = format!("{}{}{}", "BEGIN ", "RSA ", "PRIVATE KEY");
    let ssh = format!("{}{}{}", "BEGIN ", "OPENSSH ", "PRIVATE KEY");
    let ec = format!("{}{}{}", "BEGIN ", "EC ", "PRIVATE KEY");
    text.contains(&begin) || text.contains(&rsa) || text.contains(&ssh) || text.contains(&ec)
}

fn has_assigned_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let keys = [
        format!("{}{}", "api", "_key"),
        format!("{}{}", "api", "-key"),
        format!("{}{}", "api", "key"),
        format!("{}{}", "secret", "_key"),
        format!("{}{}", "private", "_key"),
    ];
    for key in keys {
        let mut start = 0;
        while let Some(i) = lower[start..].find(&key) {
            let at = start + i + key.len();
            let rest = text.get(at..).unwrap_or("").trim_start();
            if let Some(rest) = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':')) {
                let v = rest.trim_start();
                if let Some(quote) = v.chars().next().filter(|c| *c == '\'' || *c == '"') {
                    if let Some(end) = v[1..].find(quote) {
                        if end >= 8 {
                            return true;
                        }
                    }
                }
            }
            start += i + 1;
        }
    }
    false
}

struct Needle {
    label: String,
    kind: Kind,
}

enum Kind {
    Substring(String),
    Word(String),
    Akia,
    Pem,
    AssignedSecret,
}

fn needles() -> Vec<Needle> {
    vec![
        Needle {
            label: format!("{}{}", "Mil", "la"),
            kind: Kind::Word(format!("{}{}", "Mil", "la")),
        },
        Needle {
            label: format!("{}{}", "Ol", "lie"),
            kind: Kind::Word(format!("{}{}", "Ol", "lie")),
        },
        Needle {
            label: "home-matt".into(),
            kind: Kind::Substring(format!("/{}/{}", "home", "matt")),
        },
        Needle {
            label: "workspace-path".into(),
            kind: Kind::Substring(format!("/{}", "workspace")),
        },
        Needle {
            label: "kb-planning".into(),
            kind: Kind::Substring(format!("{}/{}/{}", "kb", "50-planning", "")),
        },
        Needle {
            label: "kb-product".into(),
            kind: Kind::Substring(format!("{}/{}/{}", "kb", "40-product", "")),
        },
        Needle {
            label: "evolve-priorities".into(),
            kind: Kind::Substring(format!("{}/{}", "priorities", "_evolve")),
        },
        Needle {
            label: "personal-licence".into(),
            kind: Kind::Substring(format!("{} {}", "Not for", "redistribution")),
        },
        Needle {
            label: "wealth-toml".into(),
            kind: Kind::Substring(format!("{}.{}", "wealth", "toml")),
        },
        Needle {
            label: "wealth-crate".into(),
            kind: Kind::Substring(format!("{}-{}", "drmg", "wealth")),
        },
        Needle {
            label: "pem-header".into(),
            kind: Kind::Pem,
        },
        Needle {
            label: "aws-key".into(),
            kind: Kind::Akia,
        },
        Needle {
            label: "generic-secret".into(),
            kind: Kind::AssignedSecret,
        },
    ]
}

fn hit(kind: &Kind, text: &str) -> bool {
    match kind {
        Kind::Substring(s) => {
            // The planning/product needles were built with a trailing slash via empty third part.
            let needle = s.trim_end_matches('/');
            let candidate = if s.ends_with('/') {
                format!("{needle}/")
            } else {
                needle.to_string()
            };
            text.contains(&candidate)
        }
        Kind::Word(s) => word_contains(&text.to_ascii_lowercase(), &s.to_ascii_lowercase()),
        Kind::Akia => has_akia(text),
        Kind::Pem => has_pem_private_key(text),
        Kind::AssignedSecret => has_assigned_secret(text),
    }
}

#[test]
fn public_tree_has_no_forbidden_patterns() {
    let root = repo_root().canonicalize().expect("repo root");
    let mut files = Vec::new();
    collect_files(&root, &mut files);
    assert!(!files.is_empty(), "expected to find source files");

    let patterns = needles();
    let mut hits = Vec::new();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let rel = path.strip_prefix(&root).unwrap_or(path);
        for n in &patterns {
            if hit(&n.kind, &text) {
                hits.push(format!("{} in {}", n.label, rel.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "hygiene failed; forbidden patterns in the public tree: {hits:?}"
    );
}

#[test]
fn domain_crate_has_no_io() {
    let src = repo_root().join("crates/prediction-domain/src");
    let mut files = Vec::new();
    collect_files(&src, &mut files);
    let banned = [
        format!("{}::{}", "std", "fs"),
        format!("{}::{}", "std", "net"),
        format!("{}::{}", "std", "env"),
        format!("{}::{}", "std", "process"),
        "reqwest".to_string(),
        "sqlx".to_string(),
        "rusqlite".to_string(),
        format!("{}::{}", "tokio", "fs"),
    ];
    let mut hits = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).unwrap();
        for b in &banned {
            if text.contains(b) {
                hits.push(format!("{b} in {}", path.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "prediction-domain must have no I/O: {hits:?}"
    );
}
