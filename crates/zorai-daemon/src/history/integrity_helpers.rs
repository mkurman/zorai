use super::*;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};

const PROVENANCE_SIGNATURE_SCHEME_ED25519: &str = "ed25519";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProvenanceSigningMaterial {
    pub version: u32,
    pub scheme: String,
    pub public_key_base64: String,
    pub pkcs8_base64: String,
}

pub(super) fn append_line(path: &PathBuf, line: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub(super) fn compute_memory_provenance_hash(
    target: &str,
    mode: &str,
    source_kind: &str,
    content: &str,
    fact_keys: &[String],
    thread_id: Option<&str>,
    task_id: Option<&str>,
    goal_run_id: Option<&str>,
    created_at: u64,
) -> String {
    compute_provenance_hash(
        0,
        created_at,
        "memory_write",
        &format!("{target}:{mode}"),
        &serde_json::json!({
            "source_kind": source_kind,
            "content": content,
            "fact_keys": fact_keys,
            "thread_id": thread_id,
            "task_id": task_id,
            "goal_run_id": goal_run_id,
        }),
        "memory-provenance",
        "memory",
        goal_run_id,
        task_id,
        thread_id,
        None,
        None,
        "memory_provenance",
    )
}

pub(super) fn memory_provenance_entry_from_row_db(
    row: &db::Row,
    now_ms: u64,
) -> MemoryProvenanceReportEntry {
    let created_at = row.get::<i64>(9).unwrap_or_default().max(0) as u64;
    let target: String = row.get(1).unwrap_or_default();
    let mode: String = row.get(2).unwrap_or_default();
    let source_kind: String = row.get(3).unwrap_or_default();
    let content: String = row.get(4).unwrap_or_default();
    let confirmed_at = row
        .get::<Option<i64>>(13)
        .ok()
        .flatten()
        .map(|value| value.max(0) as u64);
    let retracted_at = row
        .get::<Option<i64>>(14)
        .ok()
        .flatten()
        .map(|value| value.max(0) as u64);
    let fact_keys_json: String = row.get(5).unwrap_or_else(|_| "[]".to_string());
    let fact_keys = serde_json::from_str::<Vec<String>>(&fact_keys_json).unwrap_or_default();
    let entry_hash: String = row.get(10).unwrap_or_default();
    let signature: Option<String> = row.get::<Option<String>>(11).ok().flatten();
    let signature_scheme: Option<String> = row.get::<Option<String>>(12).ok().flatten();
    let thread_id: Option<String> = row.get::<String>(6).ok();
    let task_id: Option<String> = row.get::<String>(7).ok();
    let goal_run_id: Option<String> = row.get::<String>(8).ok();
    let age_days = now_ms.saturating_sub(created_at) as f64 / 86_400_000.0;
    let mut confidence = memory_provenance_confidence(age_days);
    let hash_valid = if entry_hash.is_empty() {
        false
    } else {
        let expected_hash = compute_memory_provenance_hash(
            &target,
            &mode,
            &source_kind,
            &content,
            &fact_keys,
            thread_id.as_deref(),
            task_id.as_deref(),
            goal_run_id.as_deref(),
            created_at,
        );
        expected_hash == entry_hash
    };
    let signature_valid = match (&signature, signature_scheme.as_deref()) {
        (Some(_), Some(scheme)) if scheme == provenance_signature_scheme_ed25519() => hash_valid,
        (Some(_), Some(_)) => false,
        (Some(_), None) => hash_valid,
        (None, _) => entry_hash.is_empty(),
    };
    let status = if retracted_at.is_some() {
        confidence = confidence.min(0.2);
        "retracted"
    } else if confirmed_at.is_some() {
        confidence = confidence.max(0.95);
        "confirmed"
    } else if !entry_hash.is_empty() && (!hash_valid || !signature_valid) {
        confidence = confidence.min(0.1);
        "invalid"
    } else if mode == "conflict" {
        confidence = confidence.min(0.35);
        "contradicted"
    } else if mode == "remove" {
        "retracted"
    } else if confidence < 0.55 {
        "uncertain"
    } else {
        "active"
    };

    MemoryProvenanceReportEntry {
        id: row.get(0).unwrap_or_default(),
        target,
        mode,
        source_kind,
        content,
        fact_keys,
        thread_id,
        task_id,
        goal_run_id,
        created_at,
        age_days,
        confidence,
        status: status.to_string(),
        entry_hash: (!entry_hash.is_empty()).then_some(entry_hash),
        signature,
        signature_scheme,
        hash_valid,
        signature_valid,
        relationships: Vec::new(),
    }
}

pub(super) fn memory_provenance_confidence(age_days: f64) -> f64 {
    let raw = 1.0 - (age_days * 0.02);
    raw.clamp(0.15, 1.0)
}

pub(super) fn compute_provenance_hash(
    sequence: u64,
    timestamp: u64,
    event_type: &str,
    summary: &str,
    details: &serde_json::Value,
    prev_hash: &str,
    agent_id: &str,
    goal_run_id: Option<&str>,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    approval_id: Option<&str>,
    causal_trace_id: Option<&str>,
    compliance_mode: &str,
) -> String {
    hex_hash(
        &serde_json::json!({
            "sequence": sequence,
            "timestamp": timestamp,
            "event_type": event_type,
            "summary": summary,
            "details": details,
            "prev_hash": prev_hash,
            "agent_id": agent_id,
            "goal_run_id": goal_run_id,
            "task_id": task_id,
            "thread_id": thread_id,
            "approval_id": approval_id,
            "causal_trace_id": causal_trace_id,
            "compliance_mode": compliance_mode,
        })
        .to_string(),
    )
}

pub(super) fn sign_provenance_hash(key: &str, entry_hash: &str) -> String {
    hex_hash(&format!("{key}:{entry_hash}"))
}

pub(super) fn provenance_signature_scheme_ed25519() -> &'static str {
    PROVENANCE_SIGNATURE_SCHEME_ED25519
}

pub(super) fn provenance_signing_material_path(root: &Path) -> PathBuf {
    root.join("provenance-signing.json")
}

pub(super) fn legacy_provenance_signing_key_path(root: &Path) -> PathBuf {
    root.join("provenance-signing.key")
}

pub(super) fn load_provenance_signing_material(path: &Path) -> Result<ProvenanceSigningMaterial> {
    let raw = std::fs::read_to_string(path)?;
    let parsed = serde_json::from_str::<ProvenanceSigningMaterial>(&raw)?;
    if parsed.scheme != PROVENANCE_SIGNATURE_SCHEME_ED25519 {
        anyhow::bail!("unsupported provenance signing scheme: {}", parsed.scheme);
    }
    Ok(parsed)
}

pub(super) fn create_provenance_signing_material() -> Result<ProvenanceSigningMaterial> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow::anyhow!("failed to generate ed25519 provenance keypair"))?;
    let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| anyhow::anyhow!("failed to parse generated ed25519 keypair"))?;
    Ok(ProvenanceSigningMaterial {
        version: 1,
        scheme: PROVENANCE_SIGNATURE_SCHEME_ED25519.to_string(),
        public_key_base64: BASE64_STANDARD.encode(key_pair.public_key().as_ref()),
        pkcs8_base64: BASE64_STANDARD.encode(pkcs8.as_ref()),
    })
}

pub(super) fn persist_provenance_signing_material(
    path: &Path,
    material: &ProvenanceSigningMaterial,
) -> Result<()> {
    let serialized = serde_json::to_string_pretty(material)?;
    std::fs::write(path, serialized)?;
    Ok(())
}

pub(super) fn sign_provenance_hash_ed25519(
    material: &ProvenanceSigningMaterial,
    entry_hash: &str,
) -> Result<String> {
    let pkcs8 = BASE64_STANDARD.decode(&material.pkcs8_base64)?;
    let key_pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| anyhow::anyhow!("failed to parse stored ed25519 provenance keypair"))?;
    Ok(BASE64_STANDARD.encode(key_pair.sign(entry_hash.as_bytes()).as_ref()))
}

pub(super) fn verify_provenance_signature_ed25519(
    material: &ProvenanceSigningMaterial,
    entry_hash: &str,
    signature: &str,
) -> bool {
    let public_key = match BASE64_STANDARD.decode(&material.public_key_base64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = match BASE64_STANDARD.decode(signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(entry_hash.as_bytes(), &signature)
        .is_ok()
}

pub(super) fn hex_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Read the last line of a WORM ledger file and extract (prev_hash, next_seq).
/// Returns ("genesis", 0) if the file does not exist or is empty.
pub(super) fn read_last_worm_entry(path: &PathBuf) -> (String, usize) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return ("genesis".to_string(), 0),
    };

    let reader = std::io::BufReader::new(file);
    let mut last_line: Option<String> = None;
    for line in reader.lines() {
        if let Ok(l) = line {
            if !l.trim().is_empty() {
                last_line = Some(l);
            }
        }
    }

    match last_line {
        None => ("genesis".to_string(), 0),
        Some(line) => {
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
                let hash = entry
                    .get("hash")
                    .and_then(|v| v.as_str())
                    .unwrap_or("genesis")
                    .to_string();
                let seq = entry
                    .get("seq")
                    .and_then(|v| v.as_u64())
                    .map(|s| s as usize + 1)
                    .unwrap_or(0);
                (hash, seq)
            } else {
                ("genesis".to_string(), 0)
            }
        }
    }
}

pub(super) fn read_last_provenance_entry(path: &PathBuf) -> Option<ProvenanceLogEntry> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut window = 64 * 1024u64;
    loop {
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut tail = Vec::with_capacity((len - start) as usize);
        file.read_to_end(&mut tail).ok()?;
        let text = String::from_utf8_lossy(&tail);
        let mut lines = text.split('\n').collect::<Vec<_>>();
        if start > 0 {
            lines.remove(0);
        }
        let found = lines.iter().rev().find_map(|line| {
            let trimmed = line.trim();
            (!trimmed.is_empty())
                .then(|| serde_json::from_str::<ProvenanceLogEntry>(trimmed).ok())
                .flatten()
        });
        if found.is_some() || start == 0 {
            return found;
        }
        window = window.saturating_mul(4);
    }
}

pub(super) fn read_provenance_entries(path: &PathBuf) -> Result<Vec<ProvenanceLogEntry>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let reader = std::io::BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<ProvenanceLogEntry>(trimmed) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Verify an individual WORM ledger file's hash-chain integrity.
pub(super) fn verify_ledger_file(kind: &str, path: &PathBuf) -> WormIntegrityResult {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return WormIntegrityResult {
                kind: kind.to_string(),
                total_entries: 0,
                valid: true,
                first_invalid_seq: None,
                message: "Ledger file not found; no entries to verify.".to_string(),
            };
        }
    };

    let reader = std::io::BufReader::new(file);
    let mut prev_hash = "genesis".to_string();
    let mut total: usize = 0;
    let mut expected_seq: usize = 0;
    let mut first_invalid_seq: Option<usize> = None;
    let mut failure_message: Option<String> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message = Some(format!(
                        "IO error reading line at seq {}: {}",
                        expected_seq, e
                    ));
                }
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        total += 1;

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message =
                        Some(format!("JSON parse error at seq {}: {}", expected_seq, e));
                }
                break;
            }
        };

        let has_seq = entry.get("seq").is_some();
        let has_prev_hash = entry.get("prev_hash").is_some();

        if !has_seq || !has_prev_hash {
            let payload = &entry["payload"];
            let recorded_hash = entry
                .get("hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let payload_json = serde_json::to_string(payload).unwrap_or_default();
            let computed = hex_hash(&payload_json);

            if recorded_hash != computed {
                if first_invalid_seq.is_none() {
                    first_invalid_seq = Some(expected_seq);
                    failure_message = Some(format!(
                        "Old-format entry at position {} has invalid standalone hash.",
                        expected_seq
                    ));
                }
                break;
            }

            prev_hash = recorded_hash.to_string();
            expected_seq += 1;
            continue;
        }

        let entry_seq = entry["seq"].as_u64().unwrap_or(0) as usize;
        let entry_prev_hash = entry
            .get("prev_hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let recorded_hash = entry
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let payload = &entry["payload"];
        let payload_json = serde_json::to_string(payload).unwrap_or_default();

        if entry_seq != expected_seq {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "Sequence number mismatch: expected {}, found {} at entry {}.",
                    expected_seq, entry_seq, total
                ));
            }
            break;
        }

        if entry_prev_hash != prev_hash {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "prev_hash mismatch at seq {}: expected '{}', found '{}'.",
                    entry_seq,
                    &prev_hash[..prev_hash.len().min(16)],
                    &entry_prev_hash[..entry_prev_hash.len().min(16)]
                ));
            }
            break;
        }

        let computed_hash = hex_hash(&format!("{}{}", entry_prev_hash, payload_json));
        if recorded_hash != computed_hash {
            if first_invalid_seq.is_none() {
                first_invalid_seq = Some(entry_seq);
                failure_message = Some(format!(
                    "Hash mismatch at seq {}: recorded '{}...', computed '{}...'.",
                    entry_seq,
                    &recorded_hash[..recorded_hash.len().min(16)],
                    &computed_hash[..computed_hash.len().min(16)]
                ));
            }
            break;
        }

        prev_hash = recorded_hash.to_string();
        expected_seq += 1;
    }

    let valid = first_invalid_seq.is_none();
    let message = if valid {
        format!(
            "{} ledger: all {} entries verified successfully.",
            kind, total
        )
    } else {
        failure_message.unwrap_or_else(|| format!("{} ledger: integrity check failed.", kind))
    };

    WormIntegrityResult {
        kind: kind.to_string(),
        total_entries: total,
        valid,
        first_invalid_seq,
        message,
    }
}

pub(crate) fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
