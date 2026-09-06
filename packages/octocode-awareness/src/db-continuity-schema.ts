/** Durable communication, handoff, and authorization records in the canonical store. */
export const CONTINUITY_SCHEMA_DDL = `
      CREATE TABLE IF NOT EXISTS handoffs (
        handoff_id TEXT PRIMARY KEY,
        workspace_path TEXT NOT NULL,
        agent_id TEXT NOT NULL,
        summary TEXT NOT NULL,
        files_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        cleared_at TEXT
      );

      CREATE TABLE IF NOT EXISTS event_outbox (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,
        event_id TEXT NOT NULL UNIQUE,
        workspace_path TEXT NOT NULL,
        event_type TEXT NOT NULL,
        aggregate_kind TEXT,
        aggregate_id TEXT,
        aggregate_revision TEXT,
        actor_json TEXT NOT NULL,
        provenance_json TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        session_id TEXT,
        correlation_id TEXT,
        created_at TEXT NOT NULL,
        expires_at TEXT
      );

      CREATE TABLE IF NOT EXISTS event_consumers (
        workspace_path TEXT NOT NULL,
        consumer_id TEXT NOT NULL,
        sequence INTEGER NOT NULL DEFAULT 0,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(workspace_path, consumer_id)
      );

      CREATE TABLE IF NOT EXISTS event_acknowledgements (
        event_id TEXT NOT NULL REFERENCES event_outbox(event_id) ON DELETE CASCADE,
        consumer_id TEXT NOT NULL,
        decision TEXT NOT NULL CHECK(decision IN ('accept', 'hold', 'refuse')),
        decided_at TEXT NOT NULL,
        PRIMARY KEY(event_id, consumer_id)
      );

      CREATE TABLE IF NOT EXISTS pending_interactions (
        interaction_id TEXT PRIMARY KEY,
        workspace_path TEXT NOT NULL,
        session_id TEXT NOT NULL,
        correlation_id TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL CHECK(kind IN ('question', 'authorization')),
        request_json TEXT NOT NULL,
        answer_json TEXT,
        status TEXT NOT NULL CHECK(status IN ('pending', 'answered', 'cancelled', 'expired')),
        created_at TEXT NOT NULL,
        expires_at TEXT,
        resolved_at TEXT
      );

      CREATE TABLE IF NOT EXISTS authorization_receipts (
        receipt_id TEXT PRIMARY KEY,
        interaction_id TEXT NOT NULL,
        workspace_path TEXT NOT NULL,
        session_id TEXT NOT NULL,
        plan_id TEXT NOT NULL,
        revision TEXT NOT NULL,
        scope_json TEXT NOT NULL,
        actor_json TEXT NOT NULL,
        provenance_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        expires_at TEXT,
        consumed_at TEXT
      );

      CREATE TABLE IF NOT EXISTS capability_receipts (
        receipt_id TEXT PRIMARY KEY,
        workspace_path TEXT NOT NULL,
        receipt_json TEXT NOT NULL,
        created_at TEXT NOT NULL
      );

`;
