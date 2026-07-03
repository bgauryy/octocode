#!/usr/bin/env node
/**
 * awareness.mjs — Node.js implementation of the Octocode awareness memory backend.
 *
 * Drop-in replacement for awareness.py. Requires Node >=22 (node:sqlite built-in).
 * Zero npm dependencies — uses only Node built-ins.
 *
 * Commands implemented (same interface as awareness.py):
 *   tell-memory       Record a lesson/decision to the shared memory store.
 *   get-memory        Recall memories (FTS5 + decay scoring).
 *   reflect           Post-task self-reflection: learning memory + optional repo fix.
 *   refine-set        Queue a codebase fix for the next agent.
 *   refine-get        List open/ongoing refinements.
 *   pre-flight-intent Create a write-intent + file locks (pre-edit hook).
 *   release-file-lock Release file locks for a completed intent (alias: release-intent).
 *   status            DB stats: memory counts, lock counts, semantic coverage.
 *   init              Create / migrate the awareness database.
 *   self-test         Smoke-test against a temporary in-memory database.
 *
 * Usage:
 *   node awareness.mjs <command> [options]
 *   node awareness.mjs tell-memory --agent-id agent --task-context "..." --observation "..."
 *       --importance-score 7 --label GOTCHA [--tag foo] [--reference url]
 *   node awareness.mjs get-memory --query "sqlite FTS" --limit 5 --min-importance 3
 *   node awareness.mjs reflect --agent-id agent --task "..." --outcome worked
 *   node awareness.mjs status
 *   node awareness.mjs self-test
 */

import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join, resolve, basename, dirname, sep } from 'node:path';
import { homedir, platform } from 'node:os';
import { randomUUID } from 'node:crypto';
import { execSync, spawnSync } from 'node:child_process';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_DB_NAME = 'awareness.sqlite3';
const MEMORY_HOME_ENV = 'OCTOCODE_MEMORY_HOME';
const FTS_INDEX_VERSION = '2';

const MEMORY_LABELS = new Set([
  'BUG','FEATURE','SUGGESTION','GOTCHA','IMPROVEMENT','DECISION',
  'ARCHITECTURE','SECURITY','PERFORMANCE','TEST','BUILD','DOCS',
  'CONFIG','WORKFLOW','REFACTOR','API','RELEASE','INCIDENT','OTHER',
]);

const REFLECTION_IMPORTANCE = { failed: 8, partial: 6, worked: 5 };

// Decay weights — mirror awareness.py DECAY_WEIGHTS
const DECAY_WEIGHTS = { importance: 0.25, recency: 0.30, access: 0.15, lexical: 0.30 };
const DEFAULT_HALF_LIFE_DAYS = 30.0;
const ACCESS_SATURATION = 50.0;

let COMPACT_OUTPUT = process.env.OCTOCODE_AWARENESS_COMPACT === '1';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function utcNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}

function memoryHome() {
  const configured = process.env[MEMORY_HOME_ENV];
  if (configured && configured.trim()) return resolve(configured.trim());
  const h = homedir();
  const p = platform();
  if (p === 'win32') {
    const appData = process.env.APPDATA ?? join(h, 'AppData', 'Roaming');
    return join(appData, '.octocode', 'memory');
  }
  if (p === 'darwin') return join(h, '.octocode', 'memory');
  const xdg = process.env.XDG_CONFIG_HOME ?? join(h, '.config');
  return join(xdg, '.octocode', 'memory');
}

function resolveDbPath(dbArg) {
  if (dbArg) return resolve(dbArg);
  return join(memoryHome(), DEFAULT_DB_NAME);
}

function emit(payload, exitCode = 0) {
  payload.ok = payload.ok ?? (exitCode === 0);
  payload.schema_version = 1;
  const text = COMPACT_OUTPUT
    ? JSON.stringify(payload)
    : JSON.stringify(payload, null, 2);
  process.stdout.write(text + '\n');
  return exitCode;
}

function die(message, extras = {}) {
  process.stdout.write(JSON.stringify({ ok: false, error: message, schema_version: 1, ...extras }, null, 2) + '\n');
  process.exit(1);
}

// ─── Arg parser (no npm deps) ─────────────────────────────────────────────────

/**
 * Minimal flag parser. Supports:
 *   --flag value        (string / number)
 *   --flag              (boolean true)
 *   --no-flag           (boolean false)
 *   --flag a --flag b   (multi-value → array via knownArrayFlags)
 */
function parseArgs(argv, knownArrayFlags = new Set()) {
  const result = { _: [] };
  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];
    if (arg === '--') { result._.push(...argv.slice(i + 1)); break; }
    if (arg.startsWith('--no-')) {
      const key = arg.slice(5).replace(/-/g, '_');
      result[key] = false; i++; continue;
    }
    if (arg.startsWith('--')) {
      const key = arg.slice(2).replace(/-/g, '_');
      const next = argv[i + 1];
      if (next === undefined || next.startsWith('--')) {
        result[key] = true; i++; continue;
      }
      const val = next; i += 2;
      if (knownArrayFlags.has(key)) {
        if (!Array.isArray(result[key])) result[key] = [];
        result[key].push(val);
      } else {
        result[key] = val;
      }
      continue;
    }
    result._.push(arg); i++;
  }
  return result;
}

// ─── Database init ────────────────────────────────────────────────────────────

function connect(dbPath) {
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new DatabaseSync(dbPath);
  db.exec('PRAGMA foreign_keys = ON');
  db.exec('PRAGMA busy_timeout = 5000');
  db.exec('PRAGMA journal_mode = WAL');
  initDb(db);
  return db;
}

function initDb(db) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS agent_memories (
      memory_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      task_context TEXT NOT NULL,
      observation TEXT NOT NULL,
      importance_score INTEGER NOT NULL CHECK(importance_score BETWEEN 1 AND 10),
      state TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by TEXT,
      tags_json TEXT NOT NULL DEFAULT '[]',
      tags_text TEXT NOT NULL DEFAULT ',',
      references_json TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      repo TEXT,
      ref TEXT,
      file_tree_fingerprint TEXT,
      file TEXT,
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

    CREATE TABLE IF NOT EXISTS agent_intents (
      intent_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      plan_doc_ref TEXT,
      rationale TEXT NOT NULL,
      test_plan TEXT NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')) DEFAULT 'ACTIVE',
      workspace_path TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS file_locks (
      lock_id TEXT PRIMARY KEY,
      file_path TEXT NOT NULL,
      intent_id TEXT NOT NULL,
      agent_id TEXT NOT NULL,
      lock_type TEXT NOT NULL CHECK(lock_type IN ('SHARED','EXCLUSIVE')),
      acquired_at TEXT NOT NULL,
      expires_at TEXT,
      FOREIGN KEY(intent_id) REFERENCES agent_intents(intent_id) ON DELETE CASCADE,
      UNIQUE(file_path, intent_id)
    );

    CREATE TABLE IF NOT EXISTS refinements (
      refinement_id TEXT PRIMARY KEY,
      agent_id TEXT NOT NULL,
      workspace_path TEXT NOT NULL,
      repo TEXT,
      ref TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      reasoning TEXT NOT NULL,
      remember TEXT NOT NULL,
      quality TEXT NOT NULL CHECK(quality IN ('good','bad')) DEFAULT 'good',
      state TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS notifications (
      notification_id TEXT PRIMARY KEY,
      workspace_path TEXT NOT NULL,
      repo TEXT,
      ref TEXT,
      from_agent TEXT NOT NULL,
      to_agent TEXT,
      kind TEXT NOT NULL,
      subject TEXT NOT NULL,
      body TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      refs_json TEXT NOT NULL DEFAULT '[]',
      thread_id TEXT NOT NULL,
      in_reply_to TEXT,
      importance INTEGER NOT NULL DEFAULT 5,
      status TEXT NOT NULL DEFAULT 'open',
      created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS awareness_meta (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS notification_reads (
      notification_id TEXT NOT NULL,
      agent_id TEXT NOT NULL,
      read_at TEXT NOT NULL,
      PRIMARY KEY (notification_id, agent_id)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_memories_importance ON agent_memories(importance_score);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_created_at ON agent_memories(created_at);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_state ON agent_memories(state);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_label ON agent_memories(label);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_file ON agent_memories(file);
    CREATE INDEX IF NOT EXISTS idx_agent_memories_failure_sig ON agent_memories(failure_signature);
    CREATE INDEX IF NOT EXISTS idx_file_locks_file_path ON file_locks(file_path);
    CREATE INDEX IF NOT EXISTS idx_file_locks_agent_id ON file_locks(agent_id);
    CREATE INDEX IF NOT EXISTS idx_file_locks_acquired_at ON file_locks(acquired_at);
    CREATE INDEX IF NOT EXISTS idx_file_locks_expires_at ON file_locks(expires_at);
    CREATE INDEX IF NOT EXISTS idx_refinements_state ON refinements(state);
    CREATE INDEX IF NOT EXISTS idx_refinements_repo ON refinements(repo);
  `);

  ensureMemoryColumns(db);
  ensureIntentColumns(db);

  // FTS5 virtual table for memory search
  try {
    db.exec(`
      CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts
      USING fts5(memory_id UNINDEXED, task_context, observation, tags)
    `);
  } catch { /* already exists or fts5 unavailable */ }

  ensureFtsVersion(db);
}

function tableColumns(db, tableName) {
  return new Set(
    db.prepare(`PRAGMA table_info(${tableName})`).all().map(r => r.name)
  );
}

function ensureMemoryColumns(db) {
  const cols = tableColumns(db, 'agent_memories');
  const alterations = [
    ['state', 'TEXT NOT NULL DEFAULT \'ACTIVE\''],
    ['label', 'TEXT NOT NULL DEFAULT \'OTHER\''],
    ['superseded_by', 'TEXT'],
    ['updated_at', 'TEXT'],
    ['file', 'TEXT'],
    ['last_accessed_at', 'TEXT'],
    ['access_count', 'INTEGER NOT NULL DEFAULT 0'],
    ['decay_half_life_days', 'REAL'],
    ['failure_signature', 'TEXT'],
    ['valid_from', 'TEXT'],
    ['valid_to', 'TEXT'],
    ['expired_at', 'TEXT'],
    ['references_json', "TEXT NOT NULL DEFAULT '[]'"],
    ['workspace_path', 'TEXT'],
    ['repo', 'TEXT'],
    ['ref', 'TEXT'],
    ['embedding', 'BLOB'],
    ['embedding_model', 'TEXT'],
  ];
  for (const [col, def] of alterations) {
    if (!cols.has(col)) {
      db.exec(`ALTER TABLE agent_memories ADD COLUMN ${col} ${def}`);
    }
  }
}

function ensureIntentColumns(db) {
  const cols = tableColumns(db, 'agent_intents');
  if (!cols.has('workspace_path')) db.exec('ALTER TABLE agent_intents ADD COLUMN workspace_path TEXT');
  if (!cols.has('files_json')) db.exec("ALTER TABLE agent_intents ADD COLUMN files_json TEXT NOT NULL DEFAULT '[]'");
}

function hasFts(db) {
  const row = db.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_fts'"
  ).get();
  return Boolean(row);
}

function ftsTermsForRow(row) {
  const tags = parseJsonList(row.tags_json);
  const refs = parseJsonList(row.references_json);
  const label = (row.label || 'OTHER').toLowerCase();
  const parts = [
    ...tags, ...refs, label,
    row.file || '', row.failure_signature || '',
    row.workspace_path || '', row.repo || '', row.ref || '',
  ];
  return parts.filter(Boolean).join(' ');
}

function rebuildFts(db) {
  db.exec("DELETE FROM memory_fts");
  const rows = db.prepare('SELECT * FROM agent_memories').all();
  const insert = db.prepare(
    'INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
  );
  for (const row of rows) {
    insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
  }
}

function ensureFtsVersion(db) {
  if (!hasFts(db)) return;
  const row = db.prepare(
    "SELECT value FROM awareness_meta WHERE key='memory_fts_version'"
  ).get();
  if (row && row.value === FTS_INDEX_VERSION) return;
  rebuildFts(db);
  db.prepare(
    "INSERT OR REPLACE INTO awareness_meta(key, value) VALUES ('memory_fts_version', ?)"
  ).run(FTS_INDEX_VERSION);
}

// ─── Data helpers ─────────────────────────────────────────────────────────────

function parseJsonList(value) {
  if (!value) return [];
  if (Array.isArray(value)) return value.map(String).filter(Boolean);
  try {
    const parsed = JSON.parse(String(value));
    return Array.isArray(parsed) ? parsed.map(String).filter(Boolean) : [];
  } catch { return []; }
}

function tagsText(tags) {
  return tags.length === 0 ? ',' : ',' + tags.join(',') + ',';
}

function normalizeTags(tags = [], csv = '') {
  const raw = [...(tags || [])];
  if (csv) raw.push(...csv.split(','));
  const seen = new Set();
  const out = [];
  for (const t of raw) {
    const cleaned = t.trim().toLowerCase().replace(/[^a-zA-Z0-9_.:-]+/g, '-').replace(/^-|-$/g, '');
    if (cleaned && !seen.has(cleaned)) { out.push(cleaned); seen.add(cleaned); }
  }
  return out;
}

function normalizeReferences(refs = []) {
  const seen = new Set();
  return (refs || [])
    .map(r => (r || '').trim().slice(0, 512))
    .filter(r => r && !seen.has(r) && seen.add(r))
    .slice(0, 20);
}

function normalizeLabel(value) {
  if (!value) return 'OTHER';
  const cleaned = value.trim().toUpperCase().replace(/[\s-]+/g, '_');
  return MEMORY_LABELS.has(cleaned) ? cleaned : 'OTHER';
}

function normalizeFilePath(filePath) {
  if (!filePath) return null;
  const p = resolve(filePath);
  return p;
}

function rowToMemory(row) {
  return {
    memory_id: row.memory_id,
    agent_id: row.agent_id,
    task_context: row.task_context,
    observation: row.observation,
    importance_score: row.importance_score,
    state: row.state ?? 'ACTIVE',
    label: row.label ?? 'OTHER',
    superseded_by: row.superseded_by ?? null,
    tags: parseJsonList(row.tags_json),
    references: parseJsonList(row.references_json),
    workspace_path: row.workspace_path ?? null,
    repo: row.repo ?? null,
    ref: row.ref ?? null,
    file: row.file ?? null,
    failure_signature: row.failure_signature ?? null,
    access_count: row.access_count ?? 0,
    last_accessed_at: row.last_accessed_at ?? null,
    decay_half_life_days: row.decay_half_life_days ?? null,
    valid_from: row.valid_from ?? null,
    valid_to: row.valid_to ?? null,
    expired_at: row.expired_at ?? null,
    file_tree_fingerprint: row.file_tree_fingerprint ?? null,
    created_at: row.created_at,
    updated_at: row.updated_at ?? null,
  };
}

// ─── Decay / salience scoring ─────────────────────────────────────────────────

function decayScore(memory, lexical, weights = DECAY_WEIGHTS) {
  const halfLife = memory.decay_half_life_days ?? DEFAULT_HALF_LIFE_DAYS;
  const lastUsedStr = memory.last_accessed_at ?? memory.created_at;
  let recency = 0;
  if (lastUsedStr) {
    const ageDays = Math.max(0, (Date.now() - new Date(lastUsedStr).getTime()) / 86400000);
    recency = Math.exp(-Math.LN2 * ageDays / Math.max(halfLife, 0.01));
  }
  const importance = (memory.importance_score ?? 0) / 10;
  const access = Math.log1p(memory.access_count ?? 0) / Math.log1p(ACCESS_SATURATION);
  const lexNorm = Math.max(0, Math.min(1, lexical));
  return (
    weights.importance * importance +
    weights.recency * recency +
    weights.access * Math.min(access, 1) +
    weights.lexical * lexNorm
  );
}

// ─── Git detection ────────────────────────────────────────────────────────────

function runCmd(cmd, args, cwd) {
  try {
    const r = spawnSync(cmd, args, { cwd: cwd || process.cwd(), encoding: 'utf8', timeout: 5000 });
    return r.status === 0 ? r.stdout.trim() : null;
  } catch { return null; }
}

function detectGit(cwd) {
  const root = runCmd('git', ['-C', cwd || '.', 'rev-parse', '--show-toplevel']);
  if (!root) return { is_repo: false };
  const branch = runCmd('git', ['-C', root, 'rev-parse', '--abbrev-ref', 'HEAD']);
  const remote = runCmd('git', ['-C', root, 'remote', 'get-url', 'origin']);
  const repoName = remote
    ? (remote.match(/github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/) || [])[1] || basename(root)
    : basename(root);
  return {
    is_repo: true,
    root,
    repo: repoName,
    branch,
    remote,
  };
}

function autoFillScope(args, cwd) {
  if (args.workspace_path && args.repo) return;
  const git = detectGit(cwd || process.cwd());
  if (!git.is_repo) return;
  if (!args.workspace_path && git.root) args.workspace_path = git.root;
  if (!args.repo && git.repo) args.repo = git.repo;
  if (!args.ref && git.branch) args.ref = git.branch;
}

// ─── FTS search ───────────────────────────────────────────────────────────────

function buildFtsQuery(query) {
  // Extract meaningful tokens; FTS5 syntax: word OR word
  const stopWords = new Set(['the','and','for','with','this','that','about','before','after']);
  const tokens = [...new Set(
    (query.toLowerCase().match(/[a-z0-9_]{2,}/g) || [])
      .filter(t => !stopWords.has(t))
  )].slice(0, 16);
  if (!tokens.length) return null;
  return tokens.join(' OR ');
}

function lexicalSearch(db, query, limit, minImportance, tags, labels, states) {
  const ftsQuery = query ? buildFtsQuery(query) : null;
  const params = [];
  const conditions = ['m.importance_score >= ?', `m.state IN (${states.map(() => '?').join(',')})`];
  params.push(minImportance, ...states);

  if (labels && labels.length > 0) {
    conditions.push(`m.label IN (${labels.map(() => '?').join(',')})`);
    params.push(...labels);
  }
  for (const tag of (tags || [])) {
    conditions.push('m.tags_text LIKE ?');
    params.push(`%,${tag},%`);
  }

  let rows;
  if (ftsQuery && hasFts(db)) {
    try {
      // Use FTS5 BM25; negative score = more relevant
      const sql = `
        SELECT m.*, ABS(bm25(memory_fts)) AS _bm25
        FROM agent_memories m
        JOIN memory_fts ON memory_fts.memory_id = m.memory_id
        WHERE memory_fts MATCH ?
          AND ${conditions.join(' AND ')}
        ORDER BY _bm25 DESC
        LIMIT ?
      `;
      rows = db.prepare(sql).all(ftsQuery, ...params, limit);
    } catch {
      rows = fallbackSearch(db, conditions, params, limit);
    }
  } else {
    rows = fallbackSearch(db, conditions, params, limit);
  }

  // Normalize lexical score 0..1 using BM25 if available, else constant 0.5
  const maxBm25 = rows.reduce((m, r) => Math.max(m, r._bm25 ?? 0), 0);
  return rows.map(row => {
    const lexical = maxBm25 > 0 ? (row._bm25 ?? 0) / maxBm25 : 0.5;
    const mem = rowToMemory(row);
    mem.score = decayScore(mem, lexical);
    return mem;
  });
}

function fallbackSearch(db, conditions, params, limit) {
  const sql = `
    SELECT m.*, 0 AS _bm25
    FROM agent_memories m
    WHERE ${conditions.join(' AND ')}
    ORDER BY m.importance_score DESC, m.created_at DESC
    LIMIT ?
  `;
  return db.prepare(sql).all(...params, limit);
}

function bumpAccess(db, memoryIds) {
  if (!memoryIds.length) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => '?').join(',');
  db.prepare(`
    UPDATE agent_memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}

// ─── Commands ─────────────────────────────────────────────────────────────────

// tell-memory
function cmdTellMemory(db, args) {
  const {
    agent_id = 'agent',
    task_context,
    observation,
    importance_score,
    label,
    tag,
    tags: tagsStr,
    reference,
    references: refsArg,
    supersedes: supersedesArg,
    failure_signature = null,
    valid_from: vf,
    valid_to: vt,
    workspace,
    repo: repoArg,
    ref: refArg,
    file: fileArg,
    file_tree_fingerprint = null,
    compact,
  } = args;

  if (compact) COMPACT_OUTPUT = true;
  if (!task_context) die('--task-context is required');
  if (!observation) die('--observation is required');
  const imp = parseInt(importance_score, 10);
  if (isNaN(imp) || imp < 1 || imp > 10) die('--importance-score must be 1–10');

  const memoryId = 'mem_' + randomUUID().replace(/-/g, '');
  const tagList = normalizeTags(
    Array.isArray(tag) ? tag : tag ? [tag] : [],
    typeof tagsStr === 'string' ? tagsStr : ''
  );
  const refList = normalizeReferences(
    Array.isArray(reference) ? reference
      : reference ? [reference]
      : Array.isArray(refsArg) ? refsArg
      : refsArg ? [refsArg]
      : []
  );
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFrom = vf || createdAt;
  const memFile = fileArg ? normalizeFilePath(fileArg) : null;

  // Auto-detect workspace/repo from git if not supplied
  const scope = { workspace_path: workspace || null, repo: repoArg || null, ref: refArg || null };
  autoFillScope(scope, process.cwd());

  const supersedesList = Array.isArray(supersedesArg)
    ? supersedesArg
    : supersedesArg ? [supersedesArg] : [];

  db.prepare(`
    INSERT INTO agent_memories (
      memory_id, agent_id, task_context, observation, importance_score,
      label, tags_json, tags_text, references_json, workspace_path, repo, ref,
      file_tree_fingerprint, file, created_at, updated_at,
      last_accessed_at, access_count, failure_signature, valid_from, valid_to
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?)
  `).run(
    memoryId, agent_id, task_context, observation, imp,
    normalizedLabel, JSON.stringify(tagList), tagsText(tagList), JSON.stringify(refList),
    scope.workspace_path, scope.repo, scope.ref,
    file_tree_fingerprint, memFile, createdAt, createdAt,
    createdAt, failure_signature || null, validFrom, vt || null
  );

  // Insert into FTS
  if (hasFts(db)) {
    db.prepare(
      'INSERT INTO memory_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    ).run(
      memoryId, task_context, observation,
      ftsTermsForRow({ tags_json: JSON.stringify(tagList), references_json: JSON.stringify(refList), label: normalizedLabel, file: memFile, failure_signature: failure_signature || null, workspace_path: scope.workspace_path, repo: scope.repo, ref: scope.ref })
    );
  }

  // Supersede old memories
  const superseded = [];
  for (const oldId of supersedesList) {
    const r = db.prepare(`
      UPDATE agent_memories
      SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
          valid_to = COALESCE(valid_to, ?), expired_at = ?
      WHERE memory_id = ? AND memory_id <> ?
    `).run(memoryId, createdAt, validFrom, createdAt, oldId, memoryId);
    if (r.changes) superseded.push(oldId);
  }

  return emit({
    db_path: resolveDbPath(args.db),
    memory: {
      memory_id: memoryId,
      agent_id,
      importance_score: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      repo: scope.repo,
      ref: scope.ref,
      file: memFile,
      state: 'ACTIVE',
      created_at: createdAt,
    },
    superseded,
  });
}

// get-memory
function cmdGetMemory(db, args) {
  const {
    query = '',
    limit: limitArg = '3',
    min_importance: minImpArg = '1',
    label,
    tag,
    smart,
    workspace,
    state: stateArg,
    compact,
    sort = 'smart',
    global_only,
    as_of,
  } = args;

  if (compact) COMPACT_OUTPUT = true;

  const limit = Math.min(20, Math.max(1, parseInt(limitArg, 10) || 3));
  let minImportance = parseInt(minImpArg, 10) || 1;

  // smart=true: widen threshold
  if (smart === true || smart === 'true') {
    minImportance = Math.max(1, minImportance - 1);
  }

  const states = stateArg ? (Array.isArray(stateArg) ? stateArg : [stateArg]) : ['ACTIVE'];
  const labels = label ? (Array.isArray(label) ? label.map(normalizeLabel) : [normalizeLabel(label)]) : [];
  const tags = tag ? (Array.isArray(tag) ? tag : [tag]) : [];

  let memories = lexicalSearch(db, query, limit * 3, minImportance, tags, labels, states);

  // Filter by workspace scope (non-strict: include global + workspace-specific)
  const scope = { workspace_path: workspace || null };
  if (!global_only && workspace) {
    memories = memories.filter(m =>
      !m.workspace_path || m.workspace_path === scope.workspace_path
    );
  }

  // Point-in-time filter
  if (as_of) {
    const asOfDate = new Date(as_of);
    memories = memories.filter(m => {
      const vf = m.valid_from ? new Date(m.valid_from) : null;
      const vt = m.valid_to ? new Date(m.valid_to) : null;
      return (!vf || vf <= asOfDate) && (!vt || vt > asOfDate);
    });
  }

  // Sort by score descending
  memories.sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  memories = memories.slice(0, limit);

  // Bump access counts
  bumpAccess(db, memories.map(m => m.memory_id));

  return emit({
    db_path: resolveDbPath(args.db),
    count: memories.length,
    memories,
    mode: hasFts(db) ? 'lexical' : 'fallback',
    sort,
    as_of: as_of ?? null,
    global_only: Boolean(global_only),
    states,
  });
}

// refine-set
function cmdRefineSet(db, args) {
  const {
    agent_id = 'agent',
    reasoning,
    remember,
    quality = 'good',
    state: stateRaw,
    workspace,
    repo: repoArg,
    ref: refArg,
    file: filesArg,
    compact,
  } = args;
  // state is in ARRAY_FLAGS — normalize to string
  const stateArg = Array.isArray(stateRaw) ? stateRaw[0] : (stateRaw ?? 'open');

  if (compact) COMPACT_OUTPUT = true;
  if (!reasoning) die('--reasoning is required');
  if (!remember) die('--remember is required');

  const refinementId = 'ref_' + randomUUID().replace(/-/g, '');
  const now = utcNow();
  const scope = { workspace_path: workspace || null, repo: repoArg || null, ref: refArg || null };
  autoFillScope(scope, process.cwd());

  const files = Array.isArray(filesArg) ? filesArg : filesArg ? [filesArg] : [];

  db.prepare(`
    INSERT INTO refinements (
      refinement_id, agent_id, workspace_path, repo, ref,
      files_json, reasoning, remember, quality, state, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `).run(
    refinementId, agent_id,
    scope.workspace_path || process.cwd(),
    scope.repo || null, scope.ref || null,
    JSON.stringify(files), reasoning, remember, quality, stateArg, now, now
  );

  return emit({
    db_path: resolveDbPath(args.db),
    refinement: {
      refinement_id: refinementId,
      agent_id,
      workspace_path: scope.workspace_path,
      repo: scope.repo,
      ref: scope.ref,
      reasoning,
      remember,
      quality,
      state: stateArg,
      created_at: now,
    },
  });
}

// refine-get
function cmdRefineGet(db, args) {
  const {
    workspace,
    repo: repoArg,
    state: stateArg,
    limit: limitArg = '10',
    compact,
  } = args;

  if (compact) COMPACT_OUTPUT = true;

  const limit = Math.min(50, Math.max(1, parseInt(limitArg, 10) || 10));
  const states = stateArg
    ? (Array.isArray(stateArg) ? stateArg : [stateArg])
    : ['open', 'ongoing'];

  const scope = { workspace_path: workspace || null, repo: repoArg || null };
  autoFillScope(scope, process.cwd());

  const params = [...states];
  const stateFilter = `state IN (${states.map(() => '?').join(',')})`;
  let sql = `SELECT * FROM refinements WHERE ${stateFilter}`;

  if (scope.repo) { sql += ' AND (repo = ? OR repo IS NULL)'; params.push(scope.repo); }
  else if (scope.workspace_path) { sql += ' AND (workspace_path = ? OR workspace_path IS NULL)'; params.push(scope.workspace_path); }

  sql += ` ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC LIMIT ?`;
  params.push(limit);

  const rows = db.prepare(sql).all(...params);
  const refinements = rows.map(r => ({
    refinement_id: r.refinement_id,
    agent_id: r.agent_id,
    workspace_path: r.workspace_path,
    repo: r.repo,
    ref: r.ref,
    files: parseJsonList(r.files_json),
    reasoning: r.reasoning,
    remember: r.remember,
    quality: r.quality,
    state: r.state,
    created_at: r.created_at,
    updated_at: r.updated_at,
  }));

  return emit({
    db_path: resolveDbPath(args.db),
    count: refinements.length,
    refinements,
  });
}

// reflect
function cmdReflect(db, dbPath, args) {
  const {
    agent_id = 'agent',
    task,
    outcome,
    lesson,
    worked,
    didnt_work,
    fix_repo,
    fix_harness,
    failure_signature: failSig,
    importance: impArg,
    workspace,
    repo: repoArg,
    ref: refArg,
    compact,
  } = args;

  if (compact) COMPACT_OUTPUT = true;
  if (!task) die('--task is required');
  const validOutcomes = ['worked', 'partial', 'failed'];
  const resolvedOutcome = validOutcomes.includes(outcome) ? outcome : 'partial';

  // Build narrative observation
  const bits = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didnt_work) bits.push(`didn't work: ${didnt_work}`);
  if (fix_harness) bits.push(`harness fix: ${fix_harness}`);
  const narrative = bits.join(' | ');
  const observation = lesson ? (bits.length > 1 ? `${lesson}  (${narrative})` : lesson) : narrative;

  const importance = impArg ? parseInt(impArg, 10) : REFLECTION_IMPORTANCE[resolvedOutcome];
  const tags = ['reflection', resolvedOutcome];
  if (fix_harness) tags.push('harness');

  const sig = failSig || (resolvedOutcome === 'failed' && fix_harness ? 'harness:reflection|outcome:failed' : null);

  // Record learning memory — capture stdout so cmdTellMemory doesn't emit
  const origWriteTell = process.stdout.write.bind(process.stdout);
  let capturedTell = '';
  process.stdout.write = (data) => { capturedTell += data; return true; };
  const tellArgs = {
    agent_id, task_context: task, observation,
    importance_score: String(importance),
    label: 'OTHER',
    tag: tags,
    failure_signature: sig,
    workspace, repo: repoArg, ref: refArg,
    db: dbPath,
  };
  cmdTellMemory(db, tellArgs);
  process.stdout.write = origWriteTell;

  // Get the memory_id from the captured tell output
  let memoryId = null;
  try {
    const tellResult = JSON.parse(capturedTell);
    memoryId = tellResult?.memory?.memory_id ?? null;
  } catch {
    // Fallback: query DB directly
    const memRow = db.prepare(
      'SELECT memory_id FROM agent_memories WHERE agent_id = ? ORDER BY created_at DESC LIMIT 1'
    ).get(agent_id);
    memoryId = memRow?.memory_id ?? null;
  }

  // Restore compact flag
  if (compact) COMPACT_OUTPUT = true;

  // Optional repo fix refinement
  let refinementId = null;
  if (fix_repo) {
    const refineArgs = {
      agent_id,
      reasoning: `Fix in repo (from ${resolvedOutcome} reflection): ${fix_repo}`,
      remember: fix_repo,
      quality: 'bad',
      state: 'open',
      workspace, repo: repoArg, ref: refArg,
      db: dbPath,
    };
    // Capture refine-set output without emitting
    const origWrite = process.stdout.write.bind(process.stdout);
    let captured = '';
    process.stdout.write = (data) => { captured += data; return true; };
    cmdRefineSet(db, refineArgs);
    process.stdout.write = origWrite;
    try {
      const refResult = JSON.parse(captured);
      refinementId = refResult?.refinement?.refinement_id ?? null;
    } catch { /* ignore */ }
  }

  return emit({
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fix_harness),
    eval_failure_count: 0,
    eval_failure_ids: [],
    next: 'refine-get → repo fixes for the next agent · mine-weakness → recurring failures · export-harness → preview harness improvements. A human merges.',
  });
}

// pre-flight-intent
function cmdPreFlightIntent(db, args) {
  const {
    agent_id = 'agent',
    workspace,
    rationale = 'agent write operation',
    test_plan = 'post-edit verification',
    target_file: targetFilesArg, // primary: --target-file
    file: filesArgLegacy,        // legacy: --file
    lock_type = 'EXCLUSIVE',
    ttl_minutes: ttlMinutesArg,  // primary: --ttl-minutes
    ttl_seconds: ttlSecondsArg,  // legacy: --ttl-seconds
    compact,
  } = args;

  if (compact) COMPACT_OUTPUT = true;

  const intentId = 'intent_' + randomUUID().replace(/-/g, '');
  const now = utcNow();

  // Merge --target-file (primary) and --file (legacy)
  const rawFiles = targetFilesArg ?? filesArgLegacy;
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [rawFiles] : [];
  const absFiles = files.map(f => resolve(f));

  const scope = { workspace_path: workspace || null };
  if (!scope.workspace_path) scope.workspace_path = process.cwd();

  // TTL: prefer --ttl-minutes, fall back to --ttl-seconds
  const ttlMs = ttlMinutesArg
    ? parseInt(ttlMinutesArg, 10) * 60000
    : ttlSecondsArg
    ? parseInt(ttlSecondsArg, 10) * 1000
    : null;

  // Check for conflicts (EXCLUSIVE locks on these files by other agents)
  const conflicts = [];
  for (const absPath of absFiles) {
    const existing = db.prepare(`
      SELECT fl.*, ai.agent_id AS intent_agent_id FROM file_locks fl
      JOIN agent_intents ai ON ai.intent_id = fl.intent_id
      WHERE fl.file_path = ?
        AND ai.agent_id <> ?
        AND ai.status = 'ACTIVE'
        AND fl.lock_type = 'EXCLUSIVE'
        AND (fl.expires_at IS NULL OR fl.expires_at > ?)
    `).all(absPath, agent_id, now);
    conflicts.push(...existing);
  }

  if (conflicts.length > 0) {
    return emit({
      ok: false,
      conflict: true,
      conflicts: conflicts.map(c => ({
        file_path: c.file_path,
        lock_type: c.lock_type,
        agent_id: c.intent_agent_id,
        acquired_at: c.acquired_at,
        expires_at: c.expires_at,
      })),
    }, 2);
  }

  // Create intent (stores absolute paths)
  db.prepare(`
    INSERT INTO agent_intents (intent_id, agent_id, rationale, test_plan, status, workspace_path, files_json, created_at, updated_at)
    VALUES (?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)
  `).run(intentId, agent_id, rationale, test_plan, scope.workspace_path, JSON.stringify(absFiles), now, now);

  // Acquire locks
  const acquiredLocks = [];
  const expiresAt = ttlMs ? new Date(Date.now() + ttlMs).toISOString().replace(/\.\d{3}Z$/, 'Z') : null;
  for (const absPath of absFiles) {
    const lockId = 'lock_' + randomUUID().replace(/-/g, '');
    db.prepare(`
      INSERT OR REPLACE INTO file_locks (lock_id, file_path, intent_id, agent_id, lock_type, acquired_at, expires_at)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(lockId, absPath, intentId, agent_id, lock_type, now, expiresAt);
    acquiredLocks.push({ lock_id: lockId, file_path: absPath, lock_type, expires_at: expiresAt });
  }

  // Response mirrors awareness.py: { intent: { intent_id, target_files, locks, ... } }
  return emit({
    db_path: resolveDbPath(args.db),
    intent: {
      intent_id: intentId,
      agent_id,
      lock_type,
      workspace_path: scope.workspace_path,
      target_files: absFiles,
      locks: acquiredLocks,
      status: 'ACTIVE',
      created_at: now,
    },
  });
}

// release-file-lock
// Accepts --intent-id (release all locks for that intent) OR --target-file
// (release locks held by agent_id on those specific files). Mirrors awareness.py.
function cmdReleaseFileLock(db, args) {
  const {
    intent_id,
    target_file: targetFilesArg,
    file: filesArgLegacy,
    agent_id = 'agent',
    status: statusArg = 'SUCCESS',
    compact,
  } = args;
  if (compact) COMPACT_OUTPUT = true;

  const now = utcNow();

  // Build WHERE clause — must always scope to agent_id
  const whereClauses = ['fl.agent_id = ?'];
  const whereParams = [agent_id];

  if (intent_id) {
    whereClauses.push('fl.intent_id = ?');
    whereParams.push(intent_id);
  }

  const rawFiles = targetFilesArg ?? filesArgLegacy;
  const targetFiles = rawFiles
    ? (Array.isArray(rawFiles) ? rawFiles : [rawFiles]).map(f => resolve(f))
    : [];

  if (targetFiles.length > 0) {
    const ph = targetFiles.map(() => '?').join(',');
    whereClauses.push(`fl.file_path IN (${ph})`);
    whereParams.push(...targetFiles);
  }

  const where = whereClauses.join(' AND ');

  // Find matching locks (need intent_ids to update intents after)
  const locks = db.prepare(
    `SELECT fl.lock_id, fl.intent_id, fl.file_path FROM file_locks fl WHERE ${where}`
  ).all(...whereParams);

  // Delete the locks (SQLite doesn't support table aliases in DELETE)
  const deleteWhere = where.replace(/\bfl\./g, '');
  db.prepare(`DELETE FROM file_locks WHERE ${deleteWhere}`).run(...whereParams);

  // For each affected intent: if no locks remain, update its status
  const intentIds = [...new Set([
    ...(intent_id ? [intent_id] : []),
    ...locks.map(l => l.intent_id),
  ])];

  for (const iid of intentIds) {
    const remaining = db.prepare(
      'SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1'
    ).get(iid);
    if (!remaining) {
      db.prepare(
        'UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ?'
      ).run(statusArg, now, iid, agent_id);
    }
  }

  return emit({
    db_path: resolveDbPath(args.db),
    agent_id,
    status: statusArg,
    released: locks.length > 0 || Boolean(intent_id),
    locks_released: locks.length,
    intent_ids: intentIds,
    updated_at: now,
  });
}

// status
function cmdStatus(db, dbPath, args) {
  const { workspace, limit: limitArg = '20', compact } = args;
  if (compact) COMPACT_OUTPUT = true;

  const now = utcNow();
  // Cleanup expired locks
  db.prepare("DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?").run(now);

  const memCount = db.prepare('SELECT COUNT(*) AS count FROM agent_memories').get().count;
  const memStates = Object.fromEntries(
    db.prepare("SELECT state, COUNT(*) AS count FROM agent_memories GROUP BY state").all()
      .map(r => [r.state, r.count])
  );
  const memLabels = Object.fromEntries(
    db.prepare("SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM agent_memories GROUP BY label").all()
      .map(r => [r.label, r.count])
  );
  const activeIntents = db.prepare(
    "SELECT COUNT(*) AS count FROM agent_intents WHERE status='ACTIVE'"
  ).get().count;
  const limit = Math.min(100, Math.max(1, parseInt(limitArg, 10) || 20));
  const locks = db.prepare(
    'SELECT file_path, intent_id, agent_id, lock_type, acquired_at, expires_at FROM file_locks ORDER BY acquired_at DESC LIMIT ?'
  ).all(limit);

  const openRefinements = db.prepare(
    "SELECT COUNT(*) AS count FROM refinements WHERE state IN ('open','ongoing')"
  ).get().count;

  return emit({
    db_path: dbPath,
    fts_enabled: hasFts(db),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    active_intent_count: activeIntents,
    open_refinements: openRefinements,
    locks,
    workspace_path: workspace || null,
  });
}

// init
function cmdInit(db, dbPath, args) {
  const { compact } = args;
  if (compact) COMPACT_OUTPUT = true;
  // initDb already called by connect(); just confirm
  const memCount = db.prepare('SELECT COUNT(*) AS count FROM agent_memories').get().count;
  return emit({ db_path: dbPath, initialized: true, memory_count: memCount });
}

// self-test
function cmdSelfTest(_dbPath, args) {
  const { compact } = args;
  if (compact) COMPACT_OUTPUT = true;

  // Use in-memory DB for smoke test
  const testDb = new DatabaseSync(':memory:');
  testDb.exec('PRAGMA foreign_keys = ON');
  initDb(testDb);

  const testAgent = 'self-test-agent';
  const now = utcNow();

  // Tell
  const memId = 'mem_' + randomUUID().replace(/-/g, '');
  testDb.prepare(`
    INSERT INTO agent_memories (memory_id, agent_id, task_context, observation, importance_score, label, tags_json, tags_text, references_json, created_at, updated_at, last_accessed_at)
    VALUES (?, ?, ?, ?, 7, 'GOTCHA', '["smoke-test"]', ',smoke-test,', '[]', ?, ?, ?)
  `).run(memId, testAgent, 'self-test task', 'This is a smoke-test memory.', now, now, now);

  if (hasFts(testDb)) {
    testDb.prepare('INSERT INTO memory_fts VALUES (?, ?, ?, ?)').run(
      memId, 'self-test task', 'This is a smoke-test memory.', 'gotcha smoke-test'
    );
  }

  // Get
  const results = lexicalSearch(testDb, 'smoke-test', 5, 1, [], [], ['ACTIVE']);
  if (!results.length) {
    return emit({ ok: false, error: 'FTS recall returned no results' }, 1);
  }

  // Reflect
  const reflMemId = 'mem_' + randomUUID().replace(/-/g, '');
  testDb.prepare(`
    INSERT INTO agent_memories (memory_id, agent_id, task_context, observation, importance_score, label, tags_json, tags_text, references_json, created_at, updated_at, last_accessed_at)
    VALUES (?, ?, ?, ?, 5, 'OTHER', '["reflection","worked"]', ',reflection,worked,', '[]', ?, ?, ?)
  `).run(reflMemId, testAgent, 'self-test', '[reflection:worked] smoke test', now, now, now);

  // Refine
  const refId = 'ref_' + randomUUID().replace(/-/g, '');
  testDb.prepare(`
    INSERT INTO refinements (refinement_id, agent_id, workspace_path, reasoning, remember, quality, state, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, 'bad', 'open', ?, ?)
  `).run(refId, testAgent, process.cwd(), 'test fix', 'fix something', now, now);

  const refRow = testDb.prepare("SELECT * FROM refinements WHERE state='open'").get();

  return emit({
    ok: true,
    db: ':memory:',
    fts_enabled: hasFts(testDb),
    memory_written: memId,
    memory_recalled: results[0].memory_id,
    reflection_memory: reflMemId,
    refinement_id: refRow?.refinement_id ?? null,
    checks: {
      write: true,
      fts_recall: results.length > 0,
      scoring: typeof results[0].score === 'number',
      refinement: Boolean(refRow),
    },
  });
}

// ─── Entry point ──────────────────────────────────────────────────────────────

const ARRAY_FLAGS = new Set([
  'tag', 'reference', 'file', 'target_file', 'supersedes', 'label', 'state',
]);

const argv = process.argv.slice(2);
if (!argv.length || argv.includes('--help') || argv.includes('-h')) {
  process.stdout.write([
    'usage: awareness.mjs <command> [options]',
    '',
    'commands: tell-memory  get-memory  reflect  refine-set  refine-get',
    '          pre-flight-intent  release-file-lock  status  init  self-test',
    '',
    'common options:',
    '  --db <path>          Override DB path (default: $OCTOCODE_MEMORY_HOME/awareness.sqlite3)',
    '  --compact            Compact JSON output (also OCTOCODE_AWARENESS_COMPACT=1)',
    '',
    'tell-memory:',
    '  --agent-id <id>  --task-context <text>  --observation <text>',
    '  --importance-score <1-10>  --label <LABEL>  [--tag <t>]...  [--reference <r>]...',
    '  [--supersedes <id>]  [--failure-signature <sig>]',
    '',
    'get-memory:',
    '  --query <text>  [--limit <n>]  [--min-importance <n>]  [--label <L>]',
    '  [--tag <t>]...  [--smart]  [--workspace <path>]',
    '',
    'reflect:',
    '  --agent-id <id>  --task <text>  --outcome worked|partial|failed',
    '  [--lesson <text>]  [--worked <text>]  [--didnt-work <text>]',
    '  [--fix-repo <text>]  [--fix-harness <text>]  [--failure-signature <sig>]',
    '  [--importance <1-10>]',
    '',
    'refine-set:',
    '  --agent-id <id>  --reasoning <text>  --remember <text>',
    '  [--quality good|bad]  [--state open|ongoing|done]  [--workspace <path>]',
    '',
    'refine-get:',
    '  [--workspace <path>]  [--repo <name>]  [--state <s>]...  [--limit <n>]',
    '',
    'pre-flight-intent:',
    '  --agent-id <id>  --workspace <path>  [--target-file <path>]...  [--ttl-minutes <n>]  [--lock-type EXCLUSIVE|SHARED]',
    '',
    'release-file-lock:',
    '  --agent-id <id>  (--intent-id <id> | --target-file <path>)  [--status PENDING|SUCCESS|FAILED]',
  ].join('\n') + '\n');
  process.exit(0);
}

// --db <path> is the only global flag that takes a value and can appear before
// the command. Extract it first so the command is simply argv[0] after removal.
function extractGlobalDb(argv) {
  let dbPath = null;
  const filtered = [];
  let i = 0;
  while (i < argv.length) {
    if (argv[i] === '--db' && i + 1 < argv.length) {
      dbPath = argv[i + 1]; i += 2;
    } else {
      filtered.push(argv[i]); i++;
    }
  }
  return { dbPath, filtered };
}
const { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(argv);
const [command, ...rest] = filteredArgv;
const args = parseArgs(rest, ARRAY_FLAGS);
if (globalDb) args.db = globalDb;

const dbPath = resolveDbPath(args.db);
let db;

if (!command) {
  process.stdout.write('No command given. Run --help for usage.\n');
  process.exit(1);
}
if (command === 'self-test') {
  process.exit(cmdSelfTest(dbPath, args));
}

db = connect(dbPath);

let exitCode = 0;
try {
  switch (command) {
    case 'tell-memory':
      exitCode = cmdTellMemory(db, { ...args, db: dbPath });
      break;
    case 'get-memory':
      exitCode = cmdGetMemory(db, { ...args, db: dbPath });
      break;
    case 'reflect':
      exitCode = cmdReflect(db, dbPath, args);
      break;
    case 'refine-set':
      exitCode = cmdRefineSet(db, { ...args, db: dbPath });
      break;
    case 'refine-get':
      exitCode = cmdRefineGet(db, { ...args, db: dbPath });
      break;
    case 'pre-flight-intent':
      exitCode = cmdPreFlightIntent(db, { ...args, db: dbPath });
      break;
    case 'release-file-lock':
    case 'release-intent': // alias for backward compat
      exitCode = cmdReleaseFileLock(db, { ...args, db: dbPath });
      break;
    case 'status':
      exitCode = cmdStatus(db, dbPath, args);
      break;
    case 'init':
      exitCode = cmdInit(db, dbPath, args);
      break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1);
  }
} catch (err) {
  exitCode = emit({ error: String(err.message || err), stack: err.stack }, 1);
}

process.exit(exitCode);
