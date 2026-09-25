//! Bounded operational diagnostics. Observations never replay or acknowledge mail.
use crate::{database::query, store::Store, store::now};
use anyhow::Result;
use serde_json::{Value, json};

impl Store {
    pub(crate) fn health(&self, input: &Value) -> Result<Value> {
        let at = now();
        let threshold = input["staleAfterMs"].as_f64().unwrap_or(300_000.0) as i64;
        let limit = input["limit"].as_f64().unwrap_or(25.0) as usize;
        let transaction = self.db.unchecked_transaction()?;
        // Start with recipient + unacknowledged index entries; never read bodies or audit history.
        let pending = "WITH pending AS (
            SELECT d.message,d.recipient,s.name,s.vendor,m.wake,m.expiresAt,
              coalesce(x.state,'queued') AS state,x.attemptedAt,x.submittedAt,
              CASE
                WHEN x.state='uncertain' THEN 'uncertain'
                WHEN m.wake='action' AND m.expiresAt<=?2 THEN 'expiredAction'
                WHEN m.wake='action' AND s.expiresAt<=?2 THEN 'offlineRecipient'
                WHEN x.state='staged' AND x.attemptedAt<=?3 THEN 'stalledOffer'
                WHEN x.state='submitted' AND m.wake='action' AND x.submittedAt<=?3 THEN 'overdueHandling'
                ELSE NULL END AS issue
            FROM sessions s JOIN deliveries d ON d.recipient=s.id AND d.acknowledgedAt IS NULL
            JOIN messages m ON m.id=d.message
            LEFT JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient
            WHERE s.workspace=?1)
        ";
        let params = [json!(self.workspace), json!(at), json!(at - threshold)];
        let counts = query(
            &transaction,
            &format!("{pending} SELECT count(*) AS unacknowledged,
                coalesce(sum(issue IS NOT NULL),0) AS attention,
                coalesce(sum(state IN ('queued','ready') AND wake='action' AND expiresAt>?2),0) AS queuedAction,
                coalesce(sum(state IN ('queued','ready') AND wake='passive' AND expiresAt>?2),0) AS waitingPassive,
                coalesce(sum(state='staged'),0) AS staged,
                coalesce(sum(state='submitted'),0) AS submittedUnacknowledged,
                coalesce(sum(state='uncertain'),0) AS uncertain,
                coalesce(sum(expiresAt<=?2),0) AS expiredUnacknowledged FROM pending"),
            &params,
        )?;
        let counts = &counts[0];
        let mut args = params.to_vec();
        args.extend([
            input["after"]["message"]
                .clone()
                .as_f64()
                .map_or(json!(0), |n| json!(n as i64)),
            json!(input["after"]["recipient"].as_str().unwrap_or("")),
            json!(limit + 1),
        ]);
        let mut issues = query(
            &transaction,
            &format!("{pending} SELECT message,recipient,name,vendor,state,issue,attemptedAt,submittedAt
                FROM pending WHERE issue IS NOT NULL AND (message>?4 OR (message=?4 AND recipient>?5))
                ORDER BY message,recipient LIMIT ?6"),
            &args,
        )?;
        let more = issues.len() > limit;
        issues.truncate(limit);
        let next = if more {
            issues.last().map(|row| {
                json!({"command":"health","input":{
                "staleAfterMs":threshold,"limit":limit,
                "after":{"message":row["message"],"recipient":row["recipient"]}}})
            })
        } else {
            None
        };
        let mut actions = serde_json::Map::new();
        for row in &issues {
            let (code, action) = match row["issue"].as_str().unwrap_or("") {
                "uncertain" => (
                    "uncertain",
                    "Inspect the recipient and dispatch token before any explicit retry; context may have arrived.",
                ),
                "expiredAction" => (
                    "expiredAction",
                    "Inspect whether work is still needed; acknowledge a deliberate no-action decision or send a new request with a new key.",
                ),
                "offlineRecipient" => (
                    "offlineRecipient",
                    "Check the recipient and its delivery owner; resume deliberately. Do not revive its expired leases.",
                ),
                "stalledOffer" => (
                    "stalledOffer",
                    "Inspect the in-flight owner and recipient before recovery; elapsed time is not permission to replay.",
                ),
                _ => (
                    "overdueHandling",
                    "Check the recipient's progress. Submission is not handling; do not auto-ack or resend.",
                ),
            };
            actions.insert(code.to_owned(), json!(action));
        }
        let status = if counts["attention"] != 0 {
            "attention"
        } else if ["queuedAction", "staged", "submittedUnacknowledged"]
            .iter()
            .any(|key| counts[key].as_i64().unwrap_or(0) > 0)
        {
            "pending"
        } else {
            "clear"
        };
        let result = json!({
            "status":status,
            "workspace":self.workspace,"observedAt":at,"staleAfterMs":threshold,
            "counts":counts,"issues":issues,"next":next,"actions":actions
        });
        transaction.commit()?;
        Ok(result)
    }
}
