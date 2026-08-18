use std::collections::BTreeMap;

use repository_engineering_runtime::model::{
    AcceptedResultCapsule, ArtifactReference, AuditRecord, AuditSuccessPayload, AuditVerdict,
    DispatchIntent, WorkerResult,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    pid: u32,
    cwd_empty: bool,
    environment: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let intent: DispatchIntent = serde_json::from_str(&arguments.next().ok_or("missing intent")?)?;
    let mode = arguments.next().unwrap_or_else(|| "success".to_owned());
    match mode.as_str() {
        "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "oversized" => {
            print!("{}", "x".repeat(512 * 1024));
            return Ok(());
        }
        "success" => {}
        _ => return Err("unknown mode".into()),
    }

    let cwd_empty = std::fs::read_dir(std::env::current_dir()?)?
        .next()
        .is_none();
    let environment = std::env::vars()
        .collect::<BTreeMap<_, _>>()
        .into_keys()
        .collect();
    let receipt_bytes = serde_json::to_vec(&Receipt {
        pid: std::process::id(),
        cwd_empty,
        environment,
    })?;
    let record_bytes = serde_json::to_vec(&AuditRecord {
        schema_version: "v0".to_owned(),
        row_id: intent.row_id.clone(),
        verdict: AuditVerdict::Confirmed,
    })?;
    let capsule = AcceptedResultCapsule {
        schema_version: "v0".to_owned(),
        result: WorkerResult::Succeeded {
            schema_version: "v0".to_owned(),
            attempt_id: intent.attempt_id.clone(),
            invocation_id: intent.invocation_id.clone(),
            assignment_id: intent.assignment_id.clone(),
            worker_instance_id: intent.worker_instance_id.clone(),
            worker_instance_receipt: ArtifactReference {
                schema_version: "v0".to_owned(),
                path: format!("receipts/{}.json", intent.invocation_id),
                sha256: digest(&receipt_bytes),
                media_type: "application/json".to_owned(),
            },
            payload: AuditSuccessPayload {
                row_id: intent.row_id.clone(),
                verdict: AuditVerdict::Confirmed,
                record: ArtifactReference {
                    schema_version: "v0".to_owned(),
                    path: format!("records/{}.json", intent.row_id),
                    sha256: digest(&record_bytes),
                    media_type: "application/json".to_owned(),
                },
            },
        },
        record_bytes: Some(record_bytes),
        worker_instance_receipt_bytes: receipt_bytes,
    };
    serde_json::to_writer(std::io::stdout(), &capsule)?;
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
