//! Wire receipt is custody, not acceptance. Server completion is not a local repair.
use serde_json::Value;
use sifttext_file_graph::Result;

pub(super) fn compatible(value: &Value) -> Result<()> {
    if value["protocol"] != 1 || value["operation"] != "snapshot-import-v1"
        || value["delivery"] != "durable-receipt-v1" {
        return Err("sync service requires durable-receipt-v1 support; no transfer attempted".into());
    }
    Ok(())
}

pub(super) fn acknowledge(value: &Value, commit: &str) -> Result<()> {
    compatible(value)?;
    if value["incoming"] != commit || value["received"] != commit {
        return Err("service did not acknowledge durable receipt of the exact incoming commit; retain queued history".into());
    }
    Ok(())
}

pub(super) fn processing(pending: &Value) -> Result<bool> {
    let records = pending.as_array().ok_or("missing hosted pending state")?;
    if records.iter().any(|record| !record["delivery_error"].is_null()) {
        return Err("hosted completion blocked; original request retained in remote_pending for recovery".into());
    }
    Ok(records.iter().any(|record| record["status"] == "received"))
}
