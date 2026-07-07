#!/usr/bin/env node
import { DatabaseSync } from "node:sqlite";
import { existsSync, mkdirSync } from "node:fs";
import { homedir, platform } from "node:os";
import { dirname, join, resolve } from "node:path";

process.removeAllListeners("warning");
process.on("warning", (warning) => {
  if (warning?.name === "ExperimentalWarning" && String(warning?.message).includes("SQLite")) return;
  console.error(warning?.stack ?? String(warning));
});

const DEFAULT_DB_NAME = "awareness.sqlite3";
const LEGACY_TABLES = [
  "agent_memories",
  "memory_references",
  "memory_fts",
  "memory_fts_data",
  "memory_fts_idx",
  "memory_fts_content",
  "memory_fts_docsize",
  "memory_fts_config",
  "agent_intents",
  "file_locks",
  "intent_events",
  "notifications",
  "notification_reads",
  "agent_identities",
  "awareness_meta",
];
const LABELS = new Set([
  "BUG", "FEATURE", "SUGGESTION", "GOTCHA", "IMPROVEMENT", "DECISION",
  "ARCHITECTURE", "SECURITY", "PERFORMANCE", "TEST", "BUILD", "DOCS",
  "CONFIG", "WORKFLOW", "REFACTOR", "API", "RELEASE", "INCIDENT",
  "EXPERIENCE", "OVERRIDE", "OTHER",
]);

function usage() {
  return `Usage:
  node scripts/legacy-migrate.mjs [--db <path>] [--write] [--drop-legacy]

Default is inspect-only. Use --write to copy legacy agent_memories into the
current memories/memory_refs/memories_fts schema. Add --drop-legacy only after
the write summary looks correct.

Options:
  --db <path>       Override DB path. Default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3
  --write           Mutate the DB by copying legacy memory rows.
  --drop-legacy     With --write, drop old legacy tables after copy/rebuild.
  --help, -h        Show this help.`;
}

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") out.help = true;
    else if (arg === "--write") out.write = true;
    else if (arg === "--drop-legacy") out.dropLegacy = true;
    else if (arg === "--db") out.db = argv[++i];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return out;
}

function memoryHome() {
  if (process.env.OCTOCODE_MEMORY_HOME?.trim()) return resolve(process.env.OCTOCODE_MEMORY_HOME.trim());
  const h = homedir();
  if (platform() === "win32") return join(process.env.APPDATA ?? join(h, "AppData", "Roaming"), ".octocode", "memory");
  if (platform() === "darwin") return join(h, ".octocode", "memory");
  return join(process.env.XDG_CONFIG_HOME ?? join(h, ".config"), ".octocode", "memory");
}

function resolveDbPath(arg) {
  return arg ? resolve(arg) : join(memoryHome(), DEFAULT_DB_NAME);
}

function emit(payload, code = 0) {
  payload.ok ??= code === 0;
  payload.schema_version = 1;
  process.stdout.write(JSON.stringify(payload, null, 2) + "\n");
  return code;
}

function parseJsonList(value) {
  if (!value) return [];
  try {
    const parsed = JSON.parse(String(value));
    return Array.isArray(parsed) ? parsed.map(String).filter(Boolean) : [];
  } catch {
    return [];
  }
}

function normalizeLabel(value) {
  const cleaned = String(value ?? "OTHER").trim().toUpperCase().replace(/[\s-]+/g, "_");
  return LABELS.has(cleaned) ? cleaned : "OTHER";
}

function normalizeReferences(values) {
  const seen = new Set();
  const out = [];
  for (const value of values) {
    const ref = String(value ?? "").trim().slice(0, 512);
    if (ref && !seen.has(ref)) {
      seen.add(ref);
      out.push(ref);
    }
  }
  return out.slice(0, 20);
}

function referenceKind(reference) {
  if (/^https?:\/\//.test(reference)) return "url";
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.-]*):/);
  return m ? m[1].toLowerCase() : "other";
}

function tableExists(db, table) {
  const row = db.prepare("SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?").get(table);
  return Boolean(row);
}

function tableColumns(db, table) {
  return new Set(db.prepare(`PRAGMA table_info("${table}")`).all().map((row) => row.name));
}

function tableCount(db, table) {
  return tableExists(db, table) ? db.prepare(`SELECT COUNT(*) AS c FROM "${table}"`).get().c : 0;
}

function ensureCleanSchema(db) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      state TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by TEXT,
      tags_json TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      artifact TEXT,
      repo TEXT,
      ref TEXT,
      file_tree_fingerprint TEXT,
      novelty_score REAL,
      last_accessed_at TEXT,
      access_count INTEGER NOT NULL DEFAULT 0,
      decay_half_life_days REAL,
      failure_signature TEXT,
      valid_from TEXT,
      valid_to TEXT,
      expired_at TEXT,
      embedding BLOB,
      embedding_model TEXT,
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT
    );
    CREATE TABLE IF NOT EXISTS memory_refs (
      memory_id TEXT NOT NULL,
      reference TEXT NOT NULL,
      kind TEXT,
      ordinal INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (memory_id, reference),
      FOREIGN KEY(memory_id) REFERENCES memories(memory_id) ON DELETE CASCADE
    );
  `);
  try {
    db.exec("CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(memory_id UNINDEXED, task_context, observation, tags)");
  } catch {
    // FTS5 may be unavailable; migration still preserves memories.
  }
}

function hasFts(db) {
  return tableExists(db, "memories_fts");
}

function ftsTerms(tagsJson, label) {
  return [...parseJsonList(tagsJson), String(label ?? "OTHER").toLowerCase()].filter(Boolean).join(" ");
}

function rebuildFts(db) {
  if (!hasFts(db)) return false;
  db.exec("DELETE FROM memories_fts");
  const rows = db.prepare("SELECT memory_id, task_context, observation, tags_json, label FROM memories").all();
  const insert = db.prepare("INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)");
  for (const row of rows) insert.run(row.memory_id, row.task_context, row.observation, ftsTerms(row.tags_json, row.label));
  return true;
}

function legacyRefsFor(db, memoryId) {
  const refs = [];
  if (tableExists(db, "memory_references")) {
    const cols = tableColumns(db, "memory_references");
    if (cols.has("memory_id") && cols.has("reference")) {
      const order = cols.has("ordinal") ? " ORDER BY ordinal" : "";
      refs.push(...db.prepare(`SELECT reference FROM memory_references WHERE memory_id = ?${order}`).all(memoryId).map((row) => row.reference));
    }
  }
  return refs;
}

function clampImportance(value) {
  const n = Number(value ?? 5);
  if (!Number.isFinite(n)) return 5;
  return Math.min(10, Math.max(1, Math.round(n)));
}

function migrate(db, { write, dropLegacy }) {
  const legacyTables = LEGACY_TABLES.filter((table) => tableExists(db, table));
  const legacyCounts = Object.fromEntries(legacyTables.map((table) => [table, tableCount(db, table)]));
  const sourceMemories = legacyCounts.agent_memories ?? 0;
  const existingMemories = tableExists(db, "agent_memories") && tableExists(db, "memories")
    ? db.prepare("SELECT COUNT(*) AS c FROM agent_memories am WHERE EXISTS (SELECT 1 FROM memories m WHERE m.memory_id = am.memory_id)").get().c
    : 0;

  if (!write) {
    return {
      dry_run: true,
      write: false,
      drop_legacy: Boolean(dropLegacy),
      legacy_tables: legacyTables,
      legacy_counts: legacyCounts,
      source_memories: sourceMemories,
      existing_memories: existingMemories,
      copied_memories: 0,
      skipped_existing: existingMemories,
      copied_references: 0,
      dropped_tables: [],
      fts_rebuilt: false,
    };
  }

  let copiedMemories = 0;
  let skippedExisting = 0;
  let copiedReferences = 0;
  const droppedTables = [];
  let ftsRebuilt = false;

  ensureCleanSchema(db);
  db.exec("BEGIN IMMEDIATE");
  try {
    if (tableExists(db, "agent_memories")) {
      const cols = tableColumns(db, "agent_memories");
      const rows = db.prepare("SELECT * FROM agent_memories ORDER BY created_at, memory_id").all();
      const insertMemory = db.prepare(`
        INSERT INTO memories (
          memory_id, agent_id, task_context, observation, importance,
          state, label, superseded_by, tags_json, workspace_path, artifact, repo, ref,
          file_tree_fingerprint, novelty_score, last_accessed_at, access_count,
          decay_half_life_days, failure_signature, valid_from, valid_to, expired_at,
          embedding, embedding_model, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
      const insertRef = db.prepare("INSERT OR REPLACE INTO memory_refs(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)");

      for (const row of rows) {
        const memoryId = String(row.memory_id ?? "").trim();
        if (!memoryId) continue;
        if (db.prepare("SELECT 1 FROM memories WHERE memory_id = ?").get(memoryId)) {
          skippedExisting++;
          continue;
        }
        const label = normalizeLabel(row.label);
        const tagsJson = JSON.stringify(parseJsonList(row.tags_json));
        const createdAt = String(row.created_at ?? new Date().toISOString());
        const references = normalizeReferences([
          ...parseJsonList(row.references_json),
          ...legacyRefsFor(db, memoryId),
          ...(row.file ? [`file:${String(row.file)}`] : []),
        ]);

        insertMemory.run(
          memoryId,
          String(row.agent_id ?? "legacy-agent"),
          String(row.task_context ?? ""),
          String(row.observation ?? ""),
          clampImportance(cols.has("importance") ? row.importance : row.importance_score),
          String(row.state ?? "ACTIVE").toUpperCase() === "SUPERSEDED" ? "SUPERSEDED" : "ACTIVE",
          label,
          row.superseded_by ? String(row.superseded_by) : null,
          tagsJson,
          row.workspace_path ? String(row.workspace_path) : null,
          row.repo ? String(row.repo) : null,
          row.ref ? String(row.ref) : null,
          row.file_tree_fingerprint ? String(row.file_tree_fingerprint) : null,
          row.novelty_score == null ? null : Number(row.novelty_score),
          row.last_accessed_at ? String(row.last_accessed_at) : null,
          Number(row.access_count ?? 0),
          row.decay_half_life_days == null ? null : Number(row.decay_half_life_days),
          row.failure_signature ? String(row.failure_signature) : null,
          row.valid_from ? String(row.valid_from) : createdAt,
          row.valid_to ? String(row.valid_to) : null,
          row.expired_at ? String(row.expired_at) : null,
          row.embedding ?? null,
          row.embedding_model ? String(row.embedding_model) : null,
          createdAt,
          row.updated_at ? String(row.updated_at) : createdAt,
        );
        references.forEach((ref, i) => insertRef.run(memoryId, ref, referenceKind(ref), i));
        copiedReferences += references.length;
        copiedMemories++;
      }
    }

    ftsRebuilt = rebuildFts(db);

    if (dropLegacy) {
      for (const table of legacyTables) {
        try {
          db.exec(`DROP TABLE IF EXISTS "${table}"`);
          droppedTables.push(table);
        } catch {
          // FTS shadow tables can disappear when their virtual table is dropped.
        }
      }
    }
    db.exec("COMMIT");
  } catch (error) {
    try { db.exec("ROLLBACK"); } catch {}
    throw error;
  }

  return {
    write: true,
    drop_legacy: Boolean(dropLegacy),
    legacy_tables: legacyTables,
    legacy_counts: legacyCounts,
    source_memories: sourceMemories,
    existing_memories: existingMemories,
    copied_memories: copiedMemories,
    skipped_existing: skippedExisting,
    copied_references: copiedReferences,
    dropped_tables: droppedTables,
    fts_rebuilt: ftsRebuilt,
  };
}

let exitCode = 0;
try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    process.stdout.write(usage() + "\n");
    process.exit(0);
  }
  if (args.dropLegacy && !args.write) {
    throw new Error("--drop-legacy requires --write");
  }
  const dbPath = resolveDbPath(args.db);
  if (!existsSync(dbPath)) {
    process.exit(emit({ error: `database not found: ${dbPath}`, db_path: dbPath }, 1));
  }
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("PRAGMA busy_timeout = 5000");
  db.exec("PRAGMA journal_mode = WAL");
  exitCode = emit({ db_path: dbPath, ...migrate(db, { write: args.write, dropLegacy: args.dropLegacy }) }, 0);
} catch (error) {
  exitCode = emit({ error: error instanceof Error ? error.message : String(error) }, 1);
}
process.exit(exitCode);
