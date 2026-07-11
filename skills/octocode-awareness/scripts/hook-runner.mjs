#!/usr/bin/env node
process.removeAllListeners('warning');
process.on('warning', (warning) => {
  if (warning?.name === 'ExperimentalWarning' && String(warning?.message).includes('SQLite')) return;
  console.error(warning?.stack ?? String(warning));
});
import{createHash as gi}from"node:crypto";import{basename as mi,relative as Si,resolve as Ti}from"node:path";import{mkdirSync as as}from"node:fs";import{join as _e,resolve as Sn,dirname as cs}from"node:path";import{homedir as ls,platform as us}from"node:os";import{resolve as ln}from"node:path";var un=["BUG","FEATURE","SUGGESTION","GOTCHA","IMPROVEMENT","DECISION","ARCHITECTURE","SECURITY","PERFORMANCE","TEST","BUILD","DOCS","CONFIG","WORKFLOW","REFACTOR","API","RELEASE","INCIDENT","EXPERIENCE","OVERRIDE","OTHER"],Jr=new Set(un);function T(){return new Date().toISOString().replace(/\.\d{3}Z$/,"Z")}function X(e){if(!e)return[];if(Array.isArray(e))return e.map(String).filter(Boolean);try{let t=JSON.parse(String(e));return Array.isArray(t)?t.map(String).filter(Boolean):[]}catch{return[]}}function Te(e=[]){let t=new Set;return e.map(n=>(n??"").trim().slice(0,512)).filter(n=>n&&!t.has(n)&&t.add(n)).slice(0,20)}function be(e){if(e==null||String(e).trim()==="")return"OTHER";let t=String(e).trim().toUpperCase().replace(/[\s-]+/g,"_");if(Jr.has(t))return t;throw new Error(`invalid label "${String(e)}"; allowed: ${un.join(", ")}`)}var Zr=["claim","handoff","question","reply","blocker","request","decision","fyi"],Eo=new Set(Zr);function dn(e,t){if(!e)return null;let n=String(e);return t?ln(t,n):ln(n)}function h(e){if(e==null)return null;let t=String(e).trim().slice(0,256);return t.length>0?t:null}function ut(e){return{memory_id:e.memory_id,agent_id:e.agent_id,task_context:e.task_context,observation:e.observation,importance:e.importance,state:e.state??"ACTIVE",label:e.label??"OTHER",superseded_by:e.superseded_by??null,tags:X(e.tags_json),references:[],workspace_path:e.workspace_path??null,artifact:e.artifact??null,repo:e.repo??null,ref:e.ref??null,novelty_score:e.novelty_score??null,failure_signature:e.failure_signature??null,access_count:e.access_count??0,last_accessed_at:e.last_accessed_at??null,decay_half_life_days:e.decay_half_life_days??null,valid_from:e.valid_from??null,valid_to:e.valid_to??null,expired_at:e.expired_at??null,file_tree_fingerprint:e.file_tree_fingerprint??null,created_at:e.created_at,updated_at:e.updated_at??null}}function Fe(e,t){let n=e.replace(/\s+/g," ").trim();return n.length<=t?n:n.slice(0,Math.max(0,t-3)).trimEnd()+"..."}var Qr=new Map([[44,6],[50,7],[51,3]]);function es(e){let t=/^(\d+)\.(\d+)\.(\d+)(?:\D.*)?$/.exec(e.trim());if(!t)return null;let n=Number(t[1]),r=Number(t[2]),s=Number(t[3]);return[n,r,s].every(Number.isSafeInteger)?[n,r,s]:null}function ts(e){let t=es(e);if(!t)return{sqliteVersion:e,safe:!1,reason:"the embedded SQLite version could not be parsed"};let[n,r,s]=t,i=n>3||n===3&&r>51,o=n===3?Qr.get(r):void 0,a=i||o!==void 0&&s>=o;return{sqliteVersion:e,safe:a,reason:a?"the embedded SQLite includes the concurrent WAL reset fix":"concurrent WAL requires SQLite 3.44.6, 3.50.7, or 3.51.3 (or a newer fixed release)"}}function pn(e){return ts(e).safe?"WAL":"DELETE"}import{createHash as ns}from"node:crypto";var he=`
    CREATE TABLE IF NOT EXISTS sessions (
      session_id     TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      workspace_path TEXT,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      started_at     TEXT NOT NULL,
      ended_at       TEXT,
      summary        TEXT
    );

    CREATE TABLE IF NOT EXISTS memories (
      memory_id             TEXT PRIMARY KEY,
      agent_id              TEXT NOT NULL,
      task_context          TEXT NOT NULL,
      observation           TEXT NOT NULL,
      importance            INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      state                 TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label                 TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by         TEXT,
      tags_json             TEXT NOT NULL DEFAULT '[]',
      workspace_path        TEXT,
      artifact              TEXT,
      repo                  TEXT,
      ref                   TEXT,
      file_tree_fingerprint TEXT,
      novelty_score         REAL,
      last_accessed_at      TEXT,
      access_count          INTEGER NOT NULL DEFAULT 0,
      decay_half_life_days  REAL,
      failure_signature     TEXT,
      valid_from            TEXT,
      valid_to              TEXT,
      expired_at            TEXT,
      embedding             BLOB,
      embedding_model       TEXT,
      created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at            TEXT
    );

    CREATE TABLE IF NOT EXISTS plans (
      plan_id        TEXT PRIMARY KEY,
      name           TEXT NOT NULL,
      objective      TEXT NOT NULL,
      lead_agent_id  TEXT NOT NULL,
      status         TEXT NOT NULL DEFAULT 'DRAFT'
                     CHECK(status IN ('DRAFT','ACTIVE','PAUSED','COMPLETED','CANCELLED')),
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      doc_dir        TEXT NOT NULL,
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS plan_members (
      plan_id    TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      agent_id   TEXT NOT NULL,
      role       TEXT NOT NULL DEFAULT 'CONTRIBUTOR' CHECK(role IN ('LEAD','CONTRIBUTOR')),
      joined_at  TEXT NOT NULL,
      PRIMARY KEY(plan_id, agent_id)
    );

    CREATE TABLE IF NOT EXISTS plan_docs (
      plan_id       TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      relative_path TEXT NOT NULL,
      title         TEXT NOT NULL,
      kind          TEXT NOT NULL DEFAULT 'SUPPORTING' CHECK(kind IN ('PRIMARY','SUPPORTING')),
      ordinal       INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(plan_id, relative_path)
    );

    CREATE TABLE IF NOT EXISTS tasks (
      task_id      TEXT PRIMARY KEY,
      plan_id      TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
      title        TEXT NOT NULL,
      reasoning    TEXT NOT NULL,
      acceptance_criteria TEXT NOT NULL,
      status       TEXT NOT NULL DEFAULT 'OPEN'
                   CHECK(status IN ('OPEN','IN_PROGRESS','BLOCKED','VERIFY','DONE','FAILED','CANCELLED')),
      priority     INTEGER NOT NULL DEFAULT 0,
      created_by   TEXT NOT NULL,
      created_at   TEXT NOT NULL,
      updated_at   TEXT NOT NULL,
      completed_at TEXT
    );

    CREATE TABLE IF NOT EXISTS task_paths (
      task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      path    TEXT NOT NULL,
      ordinal INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(task_id, path)
    );

    CREATE TABLE IF NOT EXISTS task_dependencies (
      task_id            TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      depends_on_task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      created_by         TEXT NOT NULL,
      created_at         TEXT NOT NULL,
      PRIMARY KEY(task_id, depends_on_task_id),
      CHECK(task_id <> depends_on_task_id)
    );

    CREATE TABLE IF NOT EXISTS task_runs (
      run_id         TEXT PRIMARY KEY,
      task_id        TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
      origin         TEXT NOT NULL DEFAULT 'TASK' CHECK(origin IN ('TASK','WORK','HOOK')),
      agent_id       TEXT NOT NULL,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      rationale      TEXT NOT NULL,
      test_plan      TEXT NOT NULL,
      context_ref    TEXT,
      status         TEXT NOT NULL DEFAULT 'ACTIVE'
                     CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')),
      workspace_path TEXT,
      artifact       TEXT,
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS run_files (
      run_id         TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      file_path      TEXT NOT NULL,
      reason_override TEXT,
      source         TEXT NOT NULL CHECK(source IN ('EXPLICIT','HOOK')),
      started_at     TEXT NOT NULL,
      heartbeat_at   TEXT NOT NULL,
      expires_at     TEXT NOT NULL,
      ended_at       TEXT,
      PRIMARY KEY(run_id, file_path)
    );

    CREATE TABLE IF NOT EXISTS task_claims (
      task_id      TEXT PRIMARY KEY REFERENCES tasks(task_id) ON DELETE CASCADE,
      run_id       TEXT NOT NULL UNIQUE REFERENCES task_runs(run_id) ON DELETE CASCADE,
      agent_id     TEXT NOT NULL,
      claimed_at   TEXT NOT NULL,
      heartbeat_at TEXT NOT NULL,
      expires_at   TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS task_events (
      event_id   TEXT PRIMARY KEY,
      task_id    TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
      run_id     TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      agent_id   TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message    TEXT NOT NULL,
      created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      run_id      TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      UNIQUE(file_path, run_id)
    );

    CREATE TABLE IF NOT EXISTS delivery_state (
      consumer_id TEXT NOT NULL,
      channel     TEXT NOT NULL,
      scope_key   TEXT NOT NULL,
      fingerprint TEXT NOT NULL,
      delivered_at TEXT NOT NULL,
      PRIMARY KEY(consumer_id, channel, scope_key)
    );

    CREATE TABLE IF NOT EXISTS run_log (
      event_id   TEXT PRIMARY KEY,
      run_id     TEXT,
      agent_id   TEXT NOT NULL,
      event_type TEXT NOT NULL,
      message    TEXT NOT NULL,
      created_at TEXT NOT NULL,
      FOREIGN KEY(run_id) REFERENCES task_runs(run_id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS refinements (
      refinement_id  TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      reasoning      TEXT NOT NULL,
      remember       TEXT NOT NULL,
      quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff','instructions')) DEFAULT 'good',
      state          TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS signals (
      signal_id      TEXT PRIMARY KEY,
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      from_agent     TEXT NOT NULL,
      to_agent       TEXT,
      kind           TEXT NOT NULL,
      subject        TEXT NOT NULL,
      body           TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      refs_json      TEXT NOT NULL DEFAULT '[]',
      thread_id      TEXT NOT NULL,
      reply_to       TEXT,
      importance     INTEGER NOT NULL DEFAULT 5,
      status         TEXT NOT NULL DEFAULT 'open',
      resolved_at    TEXT,
      created_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS signal_reads (
      signal_id TEXT NOT NULL,
      agent_id  TEXT NOT NULL,
      read_at   TEXT NOT NULL,
      PRIMARY KEY (signal_id, agent_id),
      FOREIGN KEY(signal_id) REFERENCES signals(signal_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS memory_refs (
      memory_id TEXT    NOT NULL,
      reference TEXT    NOT NULL,
      kind      TEXT,
      ordinal   INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (memory_id, reference),
      FOREIGN KEY(memory_id) REFERENCES memories(memory_id) ON DELETE CASCADE
    );

    -- ARCH-5: Agent identity registry \u2014 maps opaque agentIds to human-readable names.
    -- Separate from memories so the mapping persists even when memories are pruned.
    -- ON CONFLICT logic in agents.ts ensures a non-empty name is never overwritten by ''.
    CREATE TABLE IF NOT EXISTS agents (
      agent_id       TEXT PRIMARY KEY,
      agent_name     TEXT NOT NULL DEFAULT '',
      workspace_path TEXT,
      artifact       TEXT,
      context        TEXT,   -- 'pi' | 'cursor' | 'claude-code' | etc
      registered_at  TEXT NOT NULL,
      last_seen_at   TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS edit_log (
      edit_id        TEXT PRIMARY KEY,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      run_id         TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      agent_id       TEXT NOT NULL,
      file_path      TEXT NOT NULL,
      operation      TEXT NOT NULL CHECK(operation IN ('create','update','delete','move','rename')),
      old_file_path  TEXT,          -- populated for move/rename operations
      lines_added    INTEGER,
      lines_removed  INTEGER,
      content_hash   TEXT,          -- sha256 of file content after edit
      workspace_path TEXT,
      artifact       TEXT,
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );

    CREATE TABLE IF NOT EXISTS harness_log (
      harness_id   TEXT PRIMARY KEY,
      session_id   TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      agent_id     TEXT NOT NULL,
      workspace_path TEXT,
      artifact     TEXT,
      event_type   TEXT NOT NULL CHECK(event_type IN ('mine','propose','validate','apply','capture','reflect')),
      payload_json TEXT,           -- JSON with event-specific data
      memory_id    TEXT REFERENCES memories(memory_id) ON DELETE SET NULL,
      run_id       TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
      created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
    );
`,ve=`
  CREATE INDEX IF NOT EXISTS idx_sessions_agent     ON sessions(agent_id);
  CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_sessions_scope     ON sessions(workspace_path, artifact);

  CREATE INDEX IF NOT EXISTS idx_memories_importance      ON memories(importance);
  CREATE INDEX IF NOT EXISTS idx_memories_created_at      ON memories(created_at);
  CREATE INDEX IF NOT EXISTS idx_memories_state           ON memories(state);
  CREATE INDEX IF NOT EXISTS idx_memories_label           ON memories(label);
  CREATE INDEX IF NOT EXISTS idx_memories_failure_sig     ON memories(failure_signature);
  CREATE INDEX IF NOT EXISTS idx_memories_workspace_path  ON memories(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_memories_scope           ON memories(workspace_path, repo, ref);
  CREATE INDEX IF NOT EXISTS idx_memories_artifact_scope  ON memories(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_memories_repo_ref        ON memories(repo, ref);
  CREATE INDEX IF NOT EXISTS idx_memories_valid           ON memories(valid_from, valid_to);
  CREATE INDEX IF NOT EXISTS idx_memories_embedding_model ON memories(embedding_model);

  CREATE INDEX IF NOT EXISTS idx_plans_scope          ON plans(workspace_path, artifact, status);
  CREATE INDEX IF NOT EXISTS idx_plans_lead           ON plans(lead_agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_plan_members_agent   ON plan_members(agent_id, plan_id);
  CREATE INDEX IF NOT EXISTS idx_tasks_plan_status    ON tasks(plan_id, status, priority DESC, created_at);
  CREATE INDEX IF NOT EXISTS idx_task_deps_dependency ON task_dependencies(depends_on_task_id);
  CREATE INDEX IF NOT EXISTS idx_task_claims_agent    ON task_claims(agent_id, expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_claims_expiry   ON task_claims(expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_runs_status     ON task_runs(status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_agent      ON task_runs(agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_task       ON task_runs(task_id, created_at DESC);
  CREATE INDEX IF NOT EXISTS idx_task_runs_scope      ON task_runs(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_task_events_task     ON task_events(task_id, created_at);

  CREATE INDEX IF NOT EXISTS idx_run_files_path_active ON run_files(file_path, ended_at, expires_at);
  CREATE INDEX IF NOT EXISTS idx_run_files_heartbeat   ON run_files(heartbeat_at);

  CREATE INDEX IF NOT EXISTS idx_locks_file_path   ON locks(file_path);
  CREATE INDEX IF NOT EXISTS idx_locks_acquired_at ON locks(acquired_at);
  CREATE INDEX IF NOT EXISTS idx_locks_expires_at  ON locks(expires_at);

  CREATE INDEX IF NOT EXISTS idx_delivery_state_delivered ON delivery_state(delivered_at);

  CREATE INDEX IF NOT EXISTS idx_refinements_state         ON refinements(state);
  CREATE INDEX IF NOT EXISTS idx_refinements_scope         ON refinements(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_refinements_repo          ON refinements(repo);
  CREATE INDEX IF NOT EXISTS idx_refinements_state_updated ON refinements(state, updated_at DESC);

  CREATE INDEX IF NOT EXISTS idx_signals_status         ON signals(status);
  CREATE INDEX IF NOT EXISTS idx_signals_to_agent       ON signals(to_agent);
  CREATE INDEX IF NOT EXISTS idx_signals_workspace_path ON signals(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_signals_scope          ON signals(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_signals_created_at     ON signals(created_at);
  CREATE INDEX IF NOT EXISTS idx_signals_thread         ON signals(thread_id);

  CREATE INDEX IF NOT EXISTS idx_memory_refs_ref  ON memory_refs(reference);
  CREATE INDEX IF NOT EXISTS idx_memory_refs_kind ON memory_refs(kind);

  CREATE INDEX IF NOT EXISTS idx_agents_workspace ON agents(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_agents_scope     ON agents(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_agents_last_seen ON agents(last_seen_at DESC);

  CREATE INDEX IF NOT EXISTS idx_edit_log_session     ON edit_log(session_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_run         ON edit_log(run_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_agent       ON edit_log(agent_id);
  CREATE INDEX IF NOT EXISTS idx_edit_log_file        ON edit_log(file_path);
  CREATE INDEX IF NOT EXISTS idx_edit_log_workspace   ON edit_log(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_edit_log_scope       ON edit_log(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_edit_log_created_at  ON edit_log(created_at);

  CREATE INDEX IF NOT EXISTS idx_harness_log_session    ON harness_log(session_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_agent      ON harness_log(agent_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_scope      ON harness_log(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_harness_log_event_type ON harness_log(event_type);
  CREATE INDEX IF NOT EXISTS idx_harness_log_memory     ON harness_log(memory_id);
  CREATE INDEX IF NOT EXISTS idx_harness_log_run        ON harness_log(run_id);
`,Ue=`
  CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(memory_id UNINDEXED, task_context, observation, tags)
`;var He;function rs(){if(He)return He;let e=new Re(":memory:");try{e.exec(he);let t=e.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").all();return He=new Map(t.map(({name:n})=>[n,e.prepare(`PRAGMA table_info(${n})`).all()])),He}finally{e.close()}}function ss(e){return e.replace(/--[^\n]*/g," ").replace(/["`\[\]]/g,"").replace(/\bIF\s+NOT\s+EXISTS\b/gi,"").replace(/\s+/g," ").replace(/\s*([(),])\s*/g,"$1").trim().toLowerCase()}function En(e){return e.prepare(`
    SELECT type, name, tbl_name, sql
    FROM sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
    ORDER BY type, name
  `).all().map(n=>({type:n.type,name:n.name,tableName:n.tbl_name,sql:ss(n.sql??"")}))}function fn(e){return ns("sha256").update(JSON.stringify(e)).digest("hex")}var _n=new Map;function is(e){let t=_n.get(e);if(t)return t;let n=new Re(":memory:");try{n.exec(he),n.exec(ve),e&&n.exec(Ue);let r=fn(En(n));return _n.set(e,r),r}finally{n.close()}}function We(e,t){let n=t??dt(e).relations,r=new Set(rs().keys()),s=new Set(n.map(({name:c})=>c)),i=[...r].filter(c=>!s.has(c)),o=n.filter(({name:c,type:p})=>p!=="table"||!r.has(c)&&c!=="memories_fts");if(i.length===0&&o.length===0)return;let a=[i.length>0?`missing: ${i.join(", ")}`:null,o.length>0?`unexpected: ${o.map(({name:c})=>c).join(", ")}`:null].filter(c=>c!==null).join("; ");throw new Error(`canonical relation contract mismatch (${a})`)}function $e(e){let t=En(e),n=t.some(({type:i,name:o})=>i==="table"&&o==="memories_fts"),r=is(n),s=fn(t);if(s!==r)throw new Error(`canonical schema fingerprint mismatch (expected ${r}, got ${s})`)}function B(e){return!!e.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories_fts'").get()}function pt(e){let t=X(e.tags_json),n=(e.label??"OTHER").toLowerCase();return[...t,n,...e.references??[]].filter(Boolean).join(" ")}function Ie(e){e.exec("SAVEPOINT rebuild_fts");try{e.exec("DELETE FROM memories_fts");let t=e.prepare("SELECT memory_id, task_context, observation, tags_json, label FROM memories").all();if(t.length>0){let r=e.prepare(`SELECT r.memory_id, r.reference
         FROM memory_refs r
         JOIN memories m ON m.memory_id = r.memory_id
         ORDER BY r.memory_id, r.ordinal`).all(),s=new Map;for(let i of r){let o=s.get(i.memory_id)??[];o.push(i.reference),s.set(i.memory_id,o)}for(let i of t)i.references=s.get(i.memory_id)??[]}let n=e.prepare("INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)");for(let r of t)n.run(r.memory_id,r.task_context,r.observation,pt(r));e.exec("RELEASE SAVEPOINT rebuild_fts")}catch(t){try{e.exec("ROLLBACK TO SAVEPOINT rebuild_fts")}catch{}try{e.exec("RELEASE SAVEPOINT rebuild_fts")}catch{}throw t}}function gn(e,t){if((t??Xe(e))==="canonical"){e.isTransaction||e.exec("PRAGMA foreign_keys = ON");return}if(e.isTransaction)throw new Error("cannot initialize canonical Awareness inside a caller-owned transaction");e.exec("PRAGMA foreign_keys = OFF");let r=!1;try{_t(()=>e.exec("BEGIN IMMEDIATE")),r=!0,Xe(e)==="fresh"&&os(e),e.exec("COMMIT"),r=!1}catch(s){if(r)try{e.exec("ROLLBACK")}catch{}throw s}finally{e.exec("PRAGMA foreign_keys = ON")}}function os(e){e.exec(he),e.exec(ve);try{e.exec(Ue)}catch{}B(e)&&Ie(e),We(e),$e(e),mn(e),e.exec(`PRAGMA application_id = ${Ne}`)}var Tn=process.listeners("warning");process.removeAllListeners("warning");var ds=e=>{if(!(e?.name==="ExperimentalWarning"&&String(e?.message).includes("SQLite")))for(let t of Tn)t.call(process,e)};process.on("warning",ds);var{DatabaseSync:Re}=await import("node:sqlite");await new Promise(e=>setImmediate(e));process.removeAllListeners("warning");for(let e of Tn)process.on("warning",e);var ps="awareness.sqlite3",_s="OCTOCODE_MEMORY_HOME",Ne=1329812529,Es=25,hn=1e4,fs=new Int32Array(new SharedArrayBuffer(4)),gs;function Rn(){let e=process.env[_s];if(e?.trim())return Sn(e.trim());let t=ls(),n=us();if(n==="win32"){let s=process.env.APPDATA??_e(t,"AppData","Roaming");return _e(s,".octocode","memory")}if(n==="darwin")return _e(t,".octocode","memory");let r=process.env.XDG_CONFIG_HOME??_e(t,".config");return _e(r,".octocode","memory")}function Y(e){return e?Sn(e):_e(Rn(),ps)}function Et(e){as(cs(e),{recursive:!0});let t=new Re(e);try{t.exec(`PRAGMA busy_timeout = ${hn}`);let n=Xe(t),r=t.prepare("SELECT sqlite_version() AS version").get(),s=pn(r.version);return _t(()=>t.exec(`PRAGMA journal_mode = ${s}`)),t.exec("PRAGMA foreign_keys = ON"),gn(t,n),gs=t,t}catch(n){throw t.close(),n}}function dt(e){let t=e.prepare("PRAGMA application_id").get(),n=e.prepare(`
    SELECT name, type
    FROM sqlite_schema
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
      AND name NOT GLOB 'memory_fts_*'
    ORDER BY name
  `).all();return{applicationId:t.application_id??0,relations:n}}function Xe(e){let t=dt(e);if(t.applicationId===Ne)return We(e,t.relations),$e(e),"canonical";if(t.applicationId!==0)throw new Error(`refusing foreign Awareness application_id ${t.applicationId}; expected ${Ne}`);if(t.relations.length===0)return"fresh";let n=t.relations.map(({name:r})=>r).join(", ");throw new Error(`refusing unrecognized or unrelated SQLite store; relations: ${n}`)}function mn(e){let n=e.prepare("PRAGMA integrity_check").all().filter(({integrity_check:s})=>s!=="ok");if(n.length>0)throw new Error(`canonical integrity_check failed: ${n.map(s=>s.integrity_check).join("; ")}`);let r=e.prepare("PRAGMA foreign_key_check").all();if(r.length>0)throw new Error(`canonical foreign_key_check failed with ${r.length} row(s)`)}function ms(e){if(!(e instanceof Error))return!1;let t=e;return t.errcode===5||/database is (?:locked|busy)/i.test(`${t.errstr??""} ${e.message}`)}function _t(e){let t=Date.now()+hn;for(;;)try{return e()}catch(n){if(!ms(n)||Date.now()>=t)throw n;Atomics.wait(fs,0,0,Es)}}function ft(e){try{e.exec("PRAGMA wal_checkpoint(TRUNCATE)")}catch{}}function gt(e,t){return e.prepare(`SELECT fingerprint FROM delivery_state
    WHERE consumer_id = ? AND channel = ? AND scope_key = ?`).get(t.consumerId,t.channel,t.scopeKey)?.fingerprint??null}function mt(e,t){e.prepare(`INSERT INTO delivery_state
      (consumer_id, channel, scope_key, fingerprint, delivered_at)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(consumer_id, channel, scope_key) DO UPDATE SET
      fingerprint = excluded.fingerprint,
      delivered_at = excluded.delivered_at`).run(t.consumerId,t.channel,t.scopeKey,t.fingerprint,t.deliveredAt??T())}import{spawnSync as Rs}from"node:child_process";import{realpathSync as In}from"node:fs";import{basename as Tt,dirname as Is,join as Ns,resolve as ke}from"node:path";function St(e,t,n){try{let r=Rs(e,t,{cwd:n??process.cwd(),encoding:"utf8",timeout:5e3});return r.status===0?r.stdout.trim():null}catch{return null}}function ks(e){let t=St("git",["-C",e??".","rev-parse","--show-toplevel"]);if(!t)return{is_repo:!1};let n=St("git",["-C",t,"rev-parse","--abbrev-ref","HEAD"]),r=St("git",["-C",t,"remote","get-url","origin"]),s=r?(r.match(/github\.com[:/]([^/]+\/[^/]+?)(?:\.git)?$/)??[])[1]??Tt(t):Tt(t);return{is_repo:!0,root:t,repo:s,branch:n,remote:r}}function U(e){let t=ke(e),n=[];for(let r=0;r<4096;r+=1)try{return n.length?Ns(In(t),...n):In(t)}catch{let s=Is(t);if(s===t)return ke(e);n.unshift(Tt(t)),t=s}return ke(e)}function b(e,t){let r={workspace_path:e.workspace_path?U(e.workspace_path):null,artifact:e.artifact??null,repo:e.repo??null,ref:e.ref??null},s=ks(r.workspace_path??t??process.cwd());return s.is_repo&&(s.root&&(r.workspace_path=U(s.root)),!r.repo&&s.repo&&(r.repo=s.repo),!r.ref&&s.branch&&(r.ref=s.branch)),r}function N(e,t){let n=e?ke(e):t?ke(t):null,r=b({workspace_path:n},n??process.cwd());return r.workspace_path?r.workspace_path:n}import Be from"node:path";import{randomUUID as Os}from"node:crypto";import{realpathSync as An}from"node:fs";import{randomUUID as aa}from"node:crypto";var Nn=`INSERT INTO sessions (session_id, agent_id, workspace_path, artifact, repo, ref, started_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)`;var kn=`SELECT session_id, agent_id, workspace_path, artifact, repo, ref, started_at, ended_at, summary
   FROM sessions WHERE session_id = ?`;function wn(e){return e?N(e,e):null}function Oe(e,t){let n=t.sessionId.trim();if(!n)throw new Error("session id is required");let r=wn(t.workspacePath),s=h(t.artifact),i=On(e,n);if(i){if(i.agent_id!==t.agentId)throw new Error(`session ${n} belongs to agent ${i.agent_id}`);if(i.workspace_path!==r)throw new Error(`session ${n} belongs to workspace ${i.workspace_path??"(none)"}`);if(i.artifact!==s)throw new Error(`session ${n} belongs to artifact ${i.artifact??"(none)"}`);if(i.ended_at!=null)throw new Error(`session ${n} has already ended`);return i}let o=T();return e.prepare(Nn).run(n,t.agentId,r,s,null,null,o),On(e,n)}function ht(e,t){let n=T(),r=["session_id = ?","agent_id = ?","ended_at IS NULL"],s=[t.sessionId,t.agentId];return t.workspacePath!==void 0&&(r.push("workspace_path IS ?"),s.push(wn(t.workspacePath))),t.artifact!==void 0&&(r.push("artifact IS ?"),s.push(h(t.artifact))),e.prepare(`UPDATE sessions SET ended_at = ?, summary = ? WHERE ${r.join(" AND ")} RETURNING *`).get(n,t.summary??null,...s)??null}function On(e,t){return e.prepare(kn).get(t)??null}var Sa=Os().slice(0,8);function H(e,t){if(typeof t=="string"&&t.trim().length>0)e.push(t.trim());else if(Array.isArray(t))for(let n of t)H(e,n)}function Ln(e,t){if(typeof t=="string")for(let n of t.split(`
`)){let r=n.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);if(r){e.push(r[1].trim());continue}let s=n.match(/^\*\*\* Move to: (.+)$/);s&&e.push(s[1].trim())}}function yn(e){return e&&typeof e=="object"?e:{}}function Dn(...e){for(let t of e)if(typeof t=="string"&&t.trim())return t.trim();return null}function ws(e,t){if(Array.isArray(t))for(let n of t){let r=yn(n);H(e,r.path),H(e,r.filePath),H(e,r.file_path),H(e,r.paths),H(e,r.filePaths),H(e,r.file_paths)}}function je(e,t={},n={}){let r=String(e??"").toLowerCase(),s=!!n.assumeWrite||["write","edit","multi_edit","multiedit","notebookedit","notebook_edit","apply_patch","applypatch"].includes(r),i=yn(t),o=typeof t=="string"?t:Dn(i.command,i.patch);if(!s){let c=[];return Ln(c,o),[...new Set(c)]}let a=[];return H(a,i.path),H(a,i.filePath),H(a,i.file_path),H(a,i.paths),H(a,i.filePaths),H(a,i.file_paths),ws(a,i.queries),Ln(a,o),[...new Set(a)]}function Ge(e){let t=Be.resolve(e);try{return An(t)}catch{let n=[],r=t;for(;;){let s=Be.dirname(r);if(s===r)return t;n.unshift(Be.basename(r)),r=s;try{return Be.join(An(r),...n)}catch{continue}}}}import ze from"node:path";import{spawnSync as Ms}from"node:child_process";import{createHash as ya}from"node:crypto";import{randomUUID as Rt}from"node:crypto";import{isAbsolute as As,resolve as Ye}from"node:path";var Ls=10*6e4,ys=60*6e4,Ds=5;function Ee(e,t){let n=e?.trim()??"";if(!n)throw new Error(`${t} is required`);return n}function It(e){let t=e??process.cwd();return N(t,t)??Ye(t)}function Ve(e,t){if(e.length===0)throw new Error("at least one target file is required");let n=U(t?Ye(t):process.cwd());return[...new Set(e.map(r=>{let s=Ee(r,"target file");return U(As(s)?Ye(s):Ye(n,s))}))]}function Cn(e){let t=Math.min(Math.max(1,e??Ls),ys);return new Date(Date.now()+t).toISOString().replace(/\.\d{3}Z$/,"Z")}function we(e,t){let n=e.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(t);if(!n)throw new Error(`run not found: ${t}`);return n}function Nt(e,t){return e.prepare("SELECT * FROM run_files WHERE run_id = ? ORDER BY file_path").all(t)}function Cs(e,t,n){if(n.length===0)return[];let r=T();return e.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
      tr.rationale, rf.heartbeat_at, rf.expires_at,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive
    FROM run_files rf
    JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE rf.run_id <> ?
      AND rf.file_path IN (${n.map(()=>"?").join(",")})
      AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id`).all(r,t,...n,r).map(i=>({...i,exclusive:!!i.exclusive}))}function Ke(e,t,n){let r=Nt(e,t),s=n?new Set(n):null,i=s?r.filter(a=>s.has(a.file_path)):r,o=Cs(e,t,i.filter(a=>a.ended_at==null).map(a=>a.file_path));return{run:we(e,t),files:i,peers:o.slice(0,Ds),peer_count:o.length}}function xn(e,t,n,r){if(n.length===0)return[];let s=T(),i=n.map(()=>"?").join(",");return r?e.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
        tr.rationale, rf.heartbeat_at, rf.expires_at,
        EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
          AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
        'ACTIVE_WORK' AS conflict_type
      FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
      WHERE rf.file_path IN (${i}) AND rf.run_id <> ?
        AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
      ORDER BY rf.file_path, rf.heartbeat_at DESC`).all(s,...n,t,s):e.prepare(`SELECT l.run_id, tr.task_id, tr.origin, tr.agent_id, l.file_path,
      tr.rationale, rf.heartbeat_at, COALESCE(l.expires_at, rf.expires_at) AS expires_at,
      1 AS exclusive, 'EXCLUSIVE_LOCK' AS conflict_type
    FROM locks l
    JOIN task_runs tr ON tr.run_id = l.run_id
    LEFT JOIN run_files rf ON rf.run_id = l.run_id AND rf.file_path = l.file_path
    WHERE l.file_path IN (${i}) AND l.run_id <> ? AND tr.status = 'ACTIVE'
      AND (l.expires_at IS NULL OR l.expires_at > ?)
    ORDER BY l.file_path, l.acquired_at DESC`).all(...n,t,s)}function fe(e,t){let n=Ee(t.agentId,"agent id"),r=T(),s=Cn(t.ttlMs),i=t.origin??"WORK",o=t.source??(i==="HOOK"?"HOOK":"EXPLICIT"),a=t.workspacePath??process.cwd(),c=It(t.workspacePath),p=h(t.artifact),u=t.runId??null;u||(Ee(t.rationale,"rationale"),Ee(t.testPlan,"test plan")),e.exec("BEGIN IMMEDIATE");try{u??=`run_${Rt().replace(/-/g,"")}`;let l=e.prepare("SELECT * FROM task_runs WHERE run_id = ?").get(u);if(l){if(l.agent_id!==n)throw new Error(`run ${u} belongs to ${l.agent_id}`);if(l.status!=="ACTIVE")throw new Error(`run ${u} is not ACTIVE`);let f=It(l.workspace_path);if(t.workspacePath!=null&&c!==f)throw new Error(`workspace ${c} does not match run workspace ${f}`);let g=h(l.artifact);if(t.artifact!=null&&p!==g)throw new Error(`artifact ${p??"(none)"} does not match run artifact ${g??"(none)"}`);if(c=f,p=g,a=f,t.sessionId!=null){if(t.sessionId!==l.session_id)throw new Error(`run ${u} belongs to session ${l.session_id??"(none)"}`);Oe(e,{sessionId:t.sessionId,agentId:n,workspacePath:f,artifact:g})}}else{if(t.runId)throw new Error(`run not found: ${t.runId}`);t.sessionId&&Oe(e,{sessionId:t.sessionId,agentId:n,workspacePath:c,artifact:p}),e.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, context_ref,
         status, workspace_path, artifact, created_at, updated_at)
        VALUES (?, NULL, ?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)`).run(u,i,n,t.sessionId??null,Ee(t.rationale,"rationale"),Ee(t.testPlan,"test plan"),t.contextRef??null,c,p,r,r),l=we(e,u)}let _=Ve(t.targetFiles,a),d=xn(e,u,_,t.exclusive===!0);if(d.length>0)return e.exec("ROLLBACK"),{ok:!1,conflict:!0,conflicts:d};let E=e.prepare(`INSERT INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, NULL)
      ON CONFLICT(run_id, file_path) DO UPDATE SET
        reason_override = COALESCE(excluded.reason_override, run_files.reason_override),
        source = excluded.source,
        started_at = CASE WHEN run_files.ended_at IS NULL THEN run_files.started_at ELSE excluded.started_at END,
        heartbeat_at = excluded.heartbeat_at,
        expires_at = excluded.expires_at,
        ended_at = NULL`);for(let f of _)E.run(u,f,t.reasonOverride?.trim()||null,o,r,r,s),t.exclusive&&e.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`).run(`lock_${Rt().replace(/-/g,"")}`,f,u,r,s);return e.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(r,u),e.exec("COMMIT"),{ok:!0,...Ke(e,u,_)}}catch(l){try{e.exec("ROLLBACK")}catch{}throw l}}function xs(e,t,n={}){let r=we(e,t.runId);if(r.agent_id!==t.agentId)throw new Error(`run ${t.runId} belongs to ${r.agent_id}`);if(r.status!=="ACTIVE")throw new Error(`run ${t.runId} is not ACTIVE`);let s=T(),i=Cn(t.ttlMs);e.exec("BEGIN IMMEDIATE");try{let o=we(e,t.runId);if(o.agent_id!==t.agentId)throw new Error(`run ${t.runId} belongs to ${o.agent_id}`);if(o.status!=="ACTIVE")throw new Error(`run ${t.runId} is not ACTIVE`);let a=e.prepare("SELECT file_path FROM locks WHERE run_id = ?").all(t.runId),c=new Set(a.map(_=>_.file_path)),p=n.exclusiveOnly?[...c]:t.targetFiles?.length?Ve(t.targetFiles,o.workspace_path):Nt(e,t.runId).filter(_=>_.ended_at==null).map(_=>_.file_path);if(p.length===0){if(e.exec("COMMIT"),n.exclusiveOnly)return{result:Ke(e,t.runId,[]),locksRenewed:0,expiresAt:null};throw new Error("run has no active file presence")}if(e.prepare(`SELECT file_path FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND file_path IN (${p.map(()=>"?").join(",")})`).all(t.runId,...p).length!==p.length)throw new Error("one or more active file presences were not found for this run");e.prepare("DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?").run(s);for(let _ of p){let d=xn(e,t.runId,[_],c.has(_));if(d.length>0)throw new Error(`work lease conflict on ${_}: held by ${d.map(E=>E.agent_id).join(", ")}`)}let l=e.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?
      WHERE run_id = ? AND file_path = ? AND ended_at IS NULL`);for(let _ of p){if(l.run(s,i,t.runId,_).changes===0)throw new Error(`active file presence not found: ${_}`);c.has(_)&&e.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`).run(`lock_${Rt().replace(/-/g,"")}`,_,t.runId,s,i)}return e.prepare("UPDATE task_runs SET updated_at = ? WHERE run_id = ?").run(s,t.runId),e.exec("COMMIT"),{result:Ke(e,t.runId,p),locksRenewed:p.filter(_=>c.has(_)).length,expiresAt:i}}catch(o){try{e.exec("ROLLBACK")}catch{}throw o}}function te(e,t){return xs(e,t).result}function ge(e,t){let n=we(e,t.runId);if(n.agent_id!==t.agentId)throw new Error(`run ${t.runId} belongs to ${n.agent_id}`);if(n.origin==="TASK")throw new Error("TASK work must end through task submit or task release");let r=t.targetFiles?.length?Ve(t.targetFiles,n.workspace_path):Nt(e,t.runId).filter(i=>i.ended_at==null).map(i=>i.file_path),s=T();e.exec("BEGIN IMMEDIATE");try{if(r.length>0){if(e.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND file_path IN (${r.map(()=>"?").join(",")}) AND ended_at IS NULL`).run(s,s,s,t.runId,...r).changes!==r.length)throw new Error("one or more active file presences were not found for this run");e.prepare(`DELETE FROM locks WHERE run_id = ?
        AND file_path IN (${r.map(()=>"?").join(",")})`).run(t.runId,...r)}e.prepare(`SELECT 1 FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND expires_at > ? LIMIT 1`).get(t.runId,s)||e.prepare(`UPDATE task_runs SET status = 'PENDING', updated_at = ?
        WHERE run_id = ? AND status = 'ACTIVE' AND origin IN ('WORK','HOOK')`).run(s,t.runId),e.exec("COMMIT")}catch(i){try{e.exec("ROLLBACK")}catch{}throw i}return Ke(e,t.runId,r)}function qe(e,t={}){let n=T(),r=["1 = 1"],s=[n];t.activeOnly!==!1&&(r.push("rf.ended_at IS NULL","rf.expires_at > ?","tr.status = 'ACTIVE'"),s.push(n)),t.workspacePath&&(r.push("tr.workspace_path = ?"),s.push(It(t.workspacePath)));let i=h(t.artifact);i&&(r.push("(tr.artifact = ? OR tr.artifact IS NULL)"),s.push(i)),t.agentId&&(r.push("tr.agent_id = ?"),s.push(t.agentId)),t.runId&&(r.push("tr.run_id = ?"),s.push(t.runId)),t.filePath&&(r.push("rf.file_path = ?"),s.push(Ve([t.filePath],t.workspacePath)[0]));let o=t.limit==null?null:Math.max(1,Math.floor(t.limit)),a=o==null?"":"LIMIT ?";o!=null&&s.push(o);let c=e.prepare(`SELECT rf.*, tr.task_id, tr.origin, tr.agent_id, tr.session_id,
      tr.rationale, tr.test_plan, tr.status, tr.workspace_path, tr.artifact,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
      COUNT(*) OVER() AS result_total
    FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE ${r.join(" AND ")}
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id ${a}`).all(...s),p=c[0]?.result_total??0,u=c.map(({result_total:l,..._})=>({..._,exclusive:!!_.exclusive}));return{count:u.length,total_count:p,omitted_count:Math.max(0,p-u.length),files:u}}function Mn(e,t){return Ge(ze.isAbsolute(e)?e:ze.resolve(t,e))}function Ps(e,t){let n=Ge(e),r=Ge(t),s=ze.relative(r,n);return s===""||!!(s&&!s.startsWith("..")&&!ze.isAbsolute(s))}function bs(e){try{let t=Ms("git",["-C",e,"rev-parse","--abbrev-ref","HEAD"],{encoding:"utf8",timeout:5e3});return t.status===0?String(t.stdout).trim():null}catch{return null}}function Je(e){let{targetFiles:t,skillRoot:n,cwd:r}=e,s=e.env??process.env;if(!n||t.length===0||!t.some(a=>Ps(Mn(a,r),n)))return null;if(s.OCTOCODE_ALLOW_HARNESS_APPLY!=="1")return"octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1.";let o=bs(n);return o==="main"||o==="master"?`octocode-awareness: harness self-fix is never allowed on ${o}. Create a dedicated branch first.`:(!o||o==="HEAD")&&s.OCTOCODE_HARNESS_BRANCH_OK!=="1"?"octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge.":null}import Gd from"node:path";import{randomUUID as Fs}from"node:crypto";import{createHash as Wa}from"node:crypto";var Pn=`
  INSERT INTO edit_log (
    edit_id, session_id, run_id, agent_id,
    file_path, operation, old_file_path,
    lines_added, lines_removed, content_hash,
    workspace_path, artifact, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;function kt(e,t){let n="edit_"+Fs(),r=T();return e.prepare(Pn).run(n,t.sessionId??null,t.runId??null,t.agentId,t.filePath,t.operation,t.oldFilePath??null,t.linesAdded??null,t.linesRemoved??null,t.contentHash??null,t.workspacePath??null,h(t.artifact),r),{editId:n}}import{randomUUID as vs}from"node:crypto";import{isAbsolute as Ya,relative as Ka,resolve as Us,sep as Va}from"node:path";var Hs=30*6e4,Ws=60*6e4;function Ot(e,t,n,r,s,i,o=T()){e.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`).run(`tevt_${vs().replace(/-/g,"")}`,t,n,r,s,i,o)}function wt(e,t=T()){let n=e.prepare("SELECT task_id, run_id, agent_id FROM task_claims WHERE expires_at <= ?").all(t);if(n.length!==0){e.exec("SAVEPOINT evict_expired_task_claims");try{for(let r of n)e.prepare("DELETE FROM locks WHERE run_id = ?").run(r.run_id),e.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND ended_at IS NULL`).run(t,t,t,r.run_id),e.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'").run(t,r.run_id),e.prepare("UPDATE tasks SET status = 'OPEN', updated_at = ? WHERE task_id = ? AND status = 'IN_PROGRESS'").run(t,r.task_id),e.prepare("DELETE FROM task_claims WHERE task_id = ?").run(r.task_id),Ot(e,r.task_id,r.run_id,r.agent_id,"CLAIM_EXPIRED","claim lease expired",t);e.exec("RELEASE SAVEPOINT evict_expired_task_claims")}catch(r){try{e.exec("ROLLBACK TO SAVEPOINT evict_expired_task_claims")}catch{}try{e.exec("RELEASE SAVEPOINT evict_expired_task_claims")}catch{}throw r}}}function Ae(e,t){wt(e);let n=N(t.workspacePath,t.workspacePath)??Us(t.workspacePath),r=["c.agent_id = ?","p.workspace_path = ?","c.expires_at > ?"],s=[t.agentId,n,T()];t.artifact&&(r.push("(p.artifact = ? OR p.artifact IS NULL)"),s.push(t.artifact));let i=e.prepare(`SELECT c.* FROM task_claims c
    JOIN tasks t ON t.task_id = c.task_id
    JOIN plans p ON p.plan_id = t.plan_id
    WHERE ${r.join(" AND ")} ORDER BY c.claimed_at DESC LIMIT 2`).all(...s);return i.length===1?i[0]:null}import{randomUUID as tc}from"node:crypto";import{randomUUID as lc}from"node:crypto";import{isAbsolute as Xs,resolve as At}from"node:path";var Lt=20,Bs=10,Fn=3,vn=3,js=120;function yt(e,t=js){let n=e.replace(/\s+/g," ").trim();return n.length>t?`${n.slice(0,t-3)}...`:n}function Dt(e,t,n=Bs){if(t.length===0)return null;let r=t.slice(0,n),s=t.length-r.length;return`${e}${s>0?` (showing ${r.length} of ${t.length})`:""}: ${r.join(", ")}${s>0?`; ${s} omitted`:""}.`}function Ze(e,t={}){let n=!!(t.dry_run??t.dryRun),r=!!(t.expired_only??t.expiredOnly),s=t.older_than_minutes!=null?Number(t.older_than_minutes):t.olderThanMinutes!=null?Number(t.olderThanMinutes):null,i=typeof t.agent_id=="string"?t.agent_id:typeof t.agentId=="string"?t.agentId:null,o=typeof t.workspace=="string"?t.workspace:typeof t.workspace_path=="string"?t.workspace_path:typeof t.workspacePath=="string"?t.workspacePath:null,a=o?N(o,o):null,c=h(t.artifact),p=t.target_file??t.targetFile,u=(Array.isArray(p)?p:p!=null?[p]:[]).map(String).filter(Boolean).map(k=>{let x=o?At(o):process.cwd();return U(Xs(k)?At(k):At(x,k))}),l=T(),_=s!=null&&!r?new Date(Date.now()-s*6e4).toISOString():null,d=[],E=[],f=["(l.expires_at IS NOT NULL AND l.expires_at < ?)"];E.push(l),_&&(f.push("(l.acquired_at < ?)"),E.push(_)),d.push(`(${f.join(" OR ")})`),i&&(d.push("t.agent_id = ?"),E.push(i)),u.length>0&&(d.push(`l.file_path IN (${u.map(()=>"?").join(",")})`),E.push(...u)),a&&(d.push("t.workspace_path = ?"),E.push(a)),c&&(d.push("(t.artifact = ? OR t.artifact IS NULL)"),E.push(c));let g=d.join(" AND "),R="locks l JOIN task_runs t ON t.run_id = l.run_id",m=[];try{m=e.prepare(`SELECT l.lock_id, l.run_id FROM ${R} WHERE ${g}`).all(...E)}catch{}if(n)return{pruned_locks:0,dry_run:!0,would_prune:m.length,lock_ids:m.map(k=>k.lock_id).slice(0,20)};if(m.length===0)return{pruned_locks:0};let I=!e.isTransaction;I&&e.exec("BEGIN IMMEDIATE");try{if(m=e.prepare(`SELECT l.lock_id, l.run_id FROM ${R} WHERE ${g}`).all(...E),m.length===0)return I&&e.exec("COMMIT"),{pruned_locks:0};let k=m.map(()=>"?").join(",");e.prepare(`DELETE FROM locks WHERE lock_id IN (${k})`).run(...m.map(x=>x.lock_id)),I&&e.exec("COMMIT")}catch(k){if(I)try{e.exec("ROLLBACK")}catch{}throw k}return{pruned_locks:m.length}}function Ct(e,t={}){let n=b({workspace_path:t.workspacePath??null,artifact:h(t.artifact),repo:t.repo??null,ref:t.ref??null},t.cwd??process.cwd()),r=[],s="SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";return t.includeHandoffs||(s+=" AND quality NOT IN ('handoff','instructions')"),n.workspace_path&&(s+=" AND (workspace_path = ? OR workspace_path IS NULL)",r.push(n.workspace_path)),n.artifact&&(s+=" AND (artifact = ? OR artifact IS NULL)",r.push(n.artifact)),n.repo&&(s+=" AND (repo = ? OR repo IS NULL)",r.push(n.repo)),n.ref&&(s+=" AND (ref = ? OR ref IS NULL)",r.push(n.ref)),e.prepare(s).get(...r).c}import{spawnSync as Qs}from"node:child_process";import{createHash as ei}from"node:crypto";function Le(e,t,n,r,s,i,o,a={}){let c=t?Gn(t):null;if(t.trim()&&!c)return[];let p=[],u=["m.importance >= ?",`m.state IN (${o.map(()=>"?").join(",")})`];p.push(r,...o),i.length>0&&(u.push(`m.label IN (${i.map(()=>"?").join(",")})`),p.push(...i));for(let f of s)u.push("EXISTS (SELECT 1 FROM json_each(m.tags_json) WHERE value = ?)"),p.push(f);if(Yn(u,p,a),a.asOf)u.push("(m.valid_from IS NULL OR m.valid_from <= ?)"),u.push("(m.valid_to IS NULL OR m.valid_to > ?)"),p.push(a.asOf,a.asOf);else{let f=T();u.push(`(m.state <> 'ACTIVE' OR (
      (m.valid_from IS NULL OR m.valid_from <= ?)
      AND (m.valid_to IS NULL OR m.valid_to > ?)
    ))`),p.push(f,f)}let l=a.candidateMemoryIds?[...new Set(a.candidateMemoryIds)].filter(Boolean):null;if(l&&l.length===0)return[];let _=!1;if(l)if(l.length<=400)u.push(`m.memory_id IN (${l.map(()=>"?").join(",")})`),p.push(...l);else{e.exec("CREATE TEMP TABLE IF NOT EXISTS temp_memory_candidate_ids(memory_id TEXT PRIMARY KEY)"),e.exec("DELETE FROM temp_memory_candidate_ids");let f=e.prepare("INSERT OR IGNORE INTO temp_memory_candidate_ids(memory_id) VALUES (?)");for(let g of l)f.run(g);u.push("EXISTS (SELECT 1 FROM temp_memory_candidate_ids c WHERE c.memory_id = m.memory_id)"),_=!0}let d;try{if(c&&B(e))try{let f=`
          SELECT m.*, ABS(bm25(memories_fts, 0, 10, 7, 2)) AS _bm25
          FROM memories m
          JOIN memories_fts ON memories_fts.memory_id = m.memory_id
          WHERE memories_fts MATCH ?
            AND ${u.join(" AND ")}
          ORDER BY _bm25 DESC
          LIMIT ?
        `;d=e.prepare(f).all(c,...p,n)}catch{d=xt(e,t,u,p,n)}else d=xt(e,t,u,p,n)}finally{if(_)try{e.exec("DELETE FROM temp_memory_candidate_ids")}catch{}}let E=d.reduce((f,g)=>Math.max(f,g._bm25??0),0);return d.map(f=>{let g=E>=jn?(f._bm25??0)/(E+Bn):.5,R=ut(f);return R.lexical=g,R.score=bt(R,g),R})}function Mt(e,t){if(t.length!==0)try{let n=[...new Set(t.map(o=>o.memory_id))],r=n.map(()=>"?").join(","),s=e.prepare(`SELECT memory_id, reference
       FROM memory_refs
       WHERE memory_id IN (${r})
       ORDER BY memory_id, ordinal`).all(...n),i=new Map;for(let o of s){let a=i.get(o.memory_id)??[];a.push(o.reference),i.set(o.memory_id,a)}for(let o of t)o.references=i.get(o.memory_id)??[]}catch(n){if(!(n instanceof Error&&n.message.includes("no such table")))throw n}}function Pt(e){try{return new RegExp(e)}catch(t){let n=t instanceof Error?t.message:String(t);throw new Error(`invalid regex ${JSON.stringify(e)}: ${n}`)}}function ye(e,t){if(e===null)return new Set(t);let n=new Set;for(let r of e)t.has(r)&&n.add(r);return n}function Un(e,t){let n=Te(t);if(n.length===0)return new Set;let r=e.prepare(`SELECT memory_id
     FROM memory_refs
     WHERE reference IN (${n.map(()=>"?").join(",")})
     GROUP BY memory_id
     HAVING COUNT(DISTINCT reference) = ?`).all(...n,n.length);return new Set(r.map(s=>s.memory_id))}function Hn(e,t){let n=new Set;for(let r of e){let s=String(r??"").trim();if(!s)continue;if(n.add(s),s.startsWith("file:")){let o=s.slice(5);o&&n.add(o);continue}n.add(`file:${s}`);let i=dn(s,t??void 0);i&&(n.add(i),n.add(`file:${i}`))}return[...n]}function Wn(e,t){let n=[...new Set(t.map(s=>String(s??"").trim().slice(0,512)).filter(Boolean))];if(n.length===0)return new Set;let r=e.prepare(`SELECT DISTINCT memory_id
     FROM memory_refs
     WHERE reference IN (${n.map(()=>"?").join(",")})`).all(...n);return new Set(r.map(s=>s.memory_id))}function $n(e,t){if(t.length===0)return new Set;let n=e.prepare(`SELECT memory_id, reference
     FROM memory_refs
     WHERE kind = 'file' OR reference LIKE 'file:%'
     ORDER BY memory_id, ordinal`).all(),r=new Map;for(let i of n){let o=r.get(i.memory_id)??[];o.push(i.reference),r.set(i.memory_id,o)}let s=new Set;for(let[i,o]of r.entries())t.every(a=>o.some(c=>a.test(c)))&&s.add(i);return s}function Xn(e,t){if(t.length===0)return new Set;let n=e.prepare(`SELECT m.*, group_concat(r.reference, char(31)) AS references_text
     FROM memories m
     LEFT JOIN memory_refs r ON r.memory_id = m.memory_id
     GROUP BY m.memory_id`).all(),r=new Set;for(let s of n){let i=[s.task_context,s.observation,...X(s.tags_json),...s.references_text?s.references_text.split(""):[],s.label,s.workspace_path,s.artifact,s.repo,s.ref,s.failure_signature].filter(Boolean).join(" ");t.every(o=>o.test(i))&&r.add(s.memory_id)}return r}var Kn={importance:.25,recency:.3,access:.15,lexical:.3},Gs=30,Ys=50,Bn=1,jn=.01,Ft=.35;var vt=3;function Ut(e,t){if(e==null)return null;let n=String(e).trim();if(!/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$/.test(n))throw new Error(`${t} must be a valid ISO 8601 timestamp`);let s=new Date(n);if(!n||Number.isNaN(s.getTime()))throw new Error(`${t} must be a valid ISO 8601 timestamp`);return s.toISOString().replace(/\.\d{3}Z$/,"Z")}var Vn=new Set(["the","and","for","with","from","into","not","this","that","its","what","when","about","before","after","are","was","has","had","can","did","use","used","using"]);function Ks(e){let t=e.replace(/([a-z])([A-Z])/g,"$1 $2").replace(/([A-Z]+)([A-Z][a-z])/g,"$1 $2").replace(/[:_-]/g," ").toLowerCase();return new Set((t.match(/[a-z0-9]{3,}/g)??[]).filter(n=>!Vn.has(n)))}function Qe(e,t,n=Kn){let r=e.decay_half_life_days??Gs,s=e.last_accessed_at??e.created_at,i=0;if(s){let u=Math.max(0,(Date.now()-new Date(s).getTime())/864e5);i=Math.exp(-Math.LN2*u/Math.max(r,.01))}let o=(e.importance??0)/10,a=Math.min(Math.log1p(e.access_count??0)/Math.log1p(Ys),1),c=Math.max(0,Math.min(1,t)),p=n.importance*o+n.recency*i+n.access*a+n.lexical*c;return{importance:o,recency:i,access:a,relevance:c,weights:n,final:p}}function bt(e,t,n=Kn){return Qe(e,t,n).final}function Gn(e){let t=e.replace(/([a-z])([A-Z])/g,"$1 $2").replace(/([A-Z]+)([A-Z][a-z])/g,"$1 $2").replace(/[:_-]/g," ").toLowerCase(),n=[...new Set((t.match(/[a-z0-9]{3,}/g)??[]).filter(r=>!Vn.has(r)))].slice(0,16);return n.length===0?null:n.join(" OR ")}function Vs(e){return e.replace(/[\\%_]/g,"\\$&")}function qs(e,t,n){let r=[...Ks(e)].slice(0,16);if(r.length===0)return;let s=[];for(let i of r){let o=`%${Vs(i)}%`;s.push(`(
      lower(m.task_context) LIKE ? ESCAPE '\\'
      OR lower(m.observation) LIKE ? ESCAPE '\\'
      OR lower(m.tags_json) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.label, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.workspace_path, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.artifact, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.repo, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.ref, '')) LIKE ? ESCAPE '\\'
      OR lower(COALESCE(m.failure_signature, '')) LIKE ? ESCAPE '\\'
      OR EXISTS (
        SELECT 1 FROM memory_refs r
        WHERE r.memory_id = m.memory_id
          AND lower(r.reference) LIKE ? ESCAPE '\\'
      )
    )`),n.push(o,o,o,o,o,o,o,o,o,o)}t.push(`(${s.join(" OR ")})`)}function xt(e,t,n,r,s){let i=[...n],o=[...r];qs(t,i,o);let a=`
    SELECT m.*, 0 AS _bm25
    FROM memories m
    WHERE ${i.join(" AND ")}
    ORDER BY m.importance DESC, m.created_at DESC
    LIMIT ?
  `;return e.prepare(a).all(...o,s)}function Yn(e,t,n={}){let r=h(n.artifact),s=b({workspace_path:n.workspacePath??null,artifact:r,repo:n.repo??null,ref:n.ref??null},n.cwd??n.workspacePath??process.cwd());if(n.globalOnly){e.push("m.workspace_path IS NULL","m.artifact IS NULL","m.repo IS NULL","m.ref IS NULL");return}s.workspace_path&&(e.push(n.strictScope?"m.workspace_path = ?":"(m.workspace_path IS NULL OR m.workspace_path = ?)"),t.push(s.workspace_path)),s.artifact&&(e.push(n.strictScope?"m.artifact = ?":"(m.artifact IS NULL OR m.artifact = ?)"),t.push(s.artifact)),s.repo&&(e.push(n.strictScope?"m.repo = ?":"(m.repo IS NULL OR m.repo = ?)"),t.push(s.repo)),s.ref&&(e.push(n.strictScope?"m.ref = ?":"(m.ref IS NULL OR m.ref = ?)"),t.push(s.ref))}import{randomUUID as Kc}from"node:crypto";function Ht(e,t){if(t.length===0)return;let n=T(),r=t.map(()=>"?").join(",");e.prepare(`
    UPDATE memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${r})
  `).run(n,...t)}function Wt(e,t={}){let{query:n="",limit:r=3,minImportance:s=1,label:i,tags:o=[],smart:a=!1,workspacePath:c,artifact:p,repo:u,ref:l,states:_,sort:d="smart",globalOnly:E=!1,strictScope:f=!1,asOf:g,references:R=[],regex:m=[],fileRegex:I=[],files:k=[],explain:x=!1,candidateMemoryIds:W=[],recordAccess:re=!0,cwd:q}=t,se=W.length>0?2e3:50,G=Math.min(se,Math.max(1,Number(r)||3)),O=a===!0||a==="true",y=Math.max(1,Number(s)||1),z=!1,F=[];O&&y>1&&(y=Math.max(1,y-1),z=!0,F.push("min_importance"));let ie=_??(g?["ACTIVE","SUPERSEDED"]:["ACTIVE"]),ee=i?Array.isArray(i)?i.map(S=>be(S)):[be(i)]:[],lt=q??c??void 0,ue=Ut(g,"as_of"),oe=ue?new Date(ue):null;if(oe&&isNaN(oe.getTime()))throw new Error(`invalid --as-of value "${g}" \u2014 expected ISO 8601 date string (e.g. 2024-06-01T00:00:00Z)`);let $=W.length>0?new Set(W.filter(Boolean)):null,de=Te(R),pe=Hn(k,lt),ae=m.map(Pt),v=I.map(Pt);de.length>0&&($=ye($,Un(e,de))),pe.length>0&&($=ye($,Wn(e,pe))),v.length>0&&($=ye($,$n(e,v))),ae.length>0&&($=ye($,Xn(e,ae)));let an={workspacePath:c??q??null,artifact:p,repo:u,ref:l,strictScope:f,globalOnly:E,cwd:q,asOf:ue,candidateMemoryIds:$?[...$]:void 0},A=Le(e,n,G*vt,y,o,ee,ie,{...an});if(O&&A.length<G&&(ee.length>0||o.length>0||y>1)){ee.length>0&&!F.includes("label")&&F.push("label"),o.length>0&&!F.includes("tag")&&F.push("tag"),y>1&&!F.includes("min_importance")&&F.push("min_importance");let S=Le(e,n,G*vt,1,[],[],ie,an),w=new Map(A.map(M=>[M.memory_id,M]));for(let M of S)w.set(M.memory_id,M);A=[...w.values()],z=!0}if(Mt(e,A),pe.length>0){let S=new Set(pe);A=A.filter(w=>w.references.some(M=>S.has(M)))}if(de.length>0&&(A=A.filter(S=>de.every(w=>S.references.includes(w)))),(ae.length>0||v.length>0)&&(A=A.filter(S=>{if(v.length>0){let w=(S.references??[]).filter(M=>M.startsWith("file:"));if(!v.every(M=>w.some(zr=>M.test(zr))))return!1}if(ae.length>0){let w=[S.task_context,S.observation,...S.tags??[],...S.references??[],S.label,S.workspace_path,S.artifact,S.repo,S.ref,S.failure_signature].filter(Boolean).join(" ");if(!ae.every(M=>M.test(w)))return!1}return!0})),oe&&(A=A.filter(S=>{let w=S.valid_from?new Date(S.valid_from):null,M=S.valid_to?new Date(S.valid_to):null;return(!w||w<=oe)&&(!M||M>oe)})),d==="importance"?A.sort((S,w)=>w.importance-S.importance||(w.score??0)-(S.score??0)):d==="recent"?A.sort((S,w)=>(w.created_at??"").localeCompare(S.created_at??"")):d==="accessed"?A.sort((S,w)=>(w.last_accessed_at??w.created_at??"").localeCompare(S.last_accessed_at??S.created_at??"")):A.sort((S,w)=>(w.score??0)-(S.score??0)),A=A.slice(0,G),x)for(let S of A){let w=Qe(S,S.lexical??0);S.score_components=w,S.score=w.final}re&&Ht(e,A.map(S=>S.memory_id));let cn=B(e)?"lexical":"fallback",ce={count:A.length,memories:A,mode:cn,sort:d,as_of:ue,global_only:!!E,states:ie,...z?{smart_expanded:!0,smart_dropped_filters:F}:{}};if(n.trim()){let S=A[0]?.lexical??0;A.length===0?(ce.judgment_required=!0,ce.judgment_reason=O?"no results after smart widening \u2014 absence of recall is not proof of absence; broaden the query terms or scope":"no results \u2014 absence of recall is not proof of absence; retry with --smart or broader terms"):cn==="fallback"?(ce.judgment_required=!0,ce.judgment_reason="FTS unavailable \u2014 results are unranked substring matches; verify relevance before relying on them"):S<Ft&&(ce.judgment_required=!0,ce.judgment_reason=`weak lexical match (top relevance ${S.toFixed(2)} < ${Ft}) \u2014 treat results as leads, not answers`)}return ce}import{randomUUID as nu}from"node:crypto";var qn="UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'PENDING'",zn="UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'";var Jn=`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`,Zn=`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
   VALUES (?, ?, ?, 'ABANDONED', 'stale active (no live file presence) abandoned by audit-unverified --abandon', ?)`;var Qn="SELECT n.* FROM signals n",er="LEFT JOIN signal_reads nr ON nr.signal_id = n.signal_id AND nr.agent_id = ?",tr="ORDER BY n.created_at DESC LIMIT ?";var $t="INSERT OR IGNORE INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)";var nr=`INSERT INTO agents (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
   VALUES (?, ?, ?, ?, ?, ?, ?)
   ON CONFLICT(agent_id) DO UPDATE SET
     agent_name     = CASE WHEN excluded.agent_name <> '' THEN excluded.agent_name ELSE agent_name END,
     workspace_path = COALESCE(excluded.workspace_path, workspace_path),
     artifact       = COALESCE(excluded.artifact, artifact),
     context        = COALESCE(excluded.context, context),
     last_seen_at   = excluded.last_seen_at`;var rr="refinement_id, agent_id, workspace_path, artifact, repo, ref, files_json, reasoning, remember, quality, state, created_at, updated_at";var $l=`SELECT ${rr} FROM refinements
   WHERE state IN ('open','ongoing') AND quality NOT IN ('handoff','instructions')
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`,Xl=`SELECT ${rr} FROM refinements
   WHERE (workspace_path = ? OR workspace_path IS NULL)
   ORDER BY CASE state WHEN 'ongoing' THEN 0 ELSE 1 END, updated_at DESC`;function sr(e){return{signal_id:e.signal_id,workspace_path:e.workspace_path,artifact:e.artifact,repo:e.repo,ref:e.ref,from_agent:e.from_agent,to_agent:e.to_agent,kind:e.kind,subject:e.subject,body:e.body,files:X(e.files_json),refs:X(e.refs_json),thread_id:e.thread_id,reply_to:e.reply_to,importance:e.importance,status:e.status,created_at:e.created_at}}function Xt(e,t,n,r="n"){let s=r?`${r}.`:"";n.workspace_path&&(e.push(`(${s}workspace_path = ? OR ${s}workspace_path IS NULL)`),t.push(n.workspace_path)),n.artifact&&(e.push(`(${s}artifact = ? OR ${s}artifact IS NULL)`),t.push(n.artifact)),n.repo&&(e.push(`(${s}repo = ? OR ${s}repo IS NULL)`),t.push(n.repo)),n.ref&&(e.push(`(${s}ref = ? OR ${s}ref IS NULL)`),t.push(n.ref))}function ir(e,t){return e.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND reply_to IS NULL AND to_agent IS NULL
    LIMIT 1`).get(t)!=null}function Bt(e,t,n){return e.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND (from_agent = ? OR to_agent = ?)
    LIMIT 1`).get(t,n,n)!=null?!0:ir(e,t)?e.prepare(`SELECT 1 FROM signal_reads read
    JOIN signals signal ON signal.signal_id = read.signal_id
    WHERE signal.thread_id = ? AND read.agent_id = ?
    LIMIT 1`).get(t,n)!=null:!1}function or(e,t,n){return ir(e,t)||Bt(e,t,n)}function et(e,t){let{agentId:n,kinds:r=[],signalIds:s=[],threadId:i=null,unreadOnly:o=!0,markRead:a=!1,limit:c=20,cwd:p}=t,u=b({workspace_path:t.workspacePath??null,artifact:h(t.artifact),repo:t.repo??null,ref:t.ref??null},p??process.cwd());if(i&&!or(e,i,n))return{count:0,signals:[],unread_only:o};let l=[],_=[];Xt(l,_,u),i?(l.push("n.thread_id = ?"),_.push(i),o&&(l.push("n.status = 'open'"),l.push("nr.signal_id IS NULL"))):(l.push("(n.to_agent IS NULL OR n.to_agent = ?)"),_.push(n),l.push("n.from_agent <> ?"),_.push(n),o&&(l.push("n.status = 'open'"),l.push("nr.signal_id IS NULL"))),r.length>0&&(l.push(`n.kind IN (${r.map(()=>"?").join(",")})`),_.push(...r)),s.length>0&&(l.push(`n.signal_id IN (${s.map(()=>"?").join(",")})`),_.push(...s));let d=l.length>0?`WHERE ${l.join(" AND ")}`:"",E=o?er:"",f=o?[n,..._]:_,g=`
    ${Qn}
    ${E}
    ${d}
    ${tr}
  `,R=Math.min(200,Math.max(1,Math.floor(Number.isFinite(c)?c:20))),I=e.prepare(g).all(...f,R).map(sr);if(a&&I.length>0){let k=T(),x=e.prepare($t);for(let W of I)x.run(W.signal_id,n,k)}return{count:I.length,signals:I,unread_only:o}}var jt=["GOTCHA","BUG","DECISION","IMPROVEMENT","ARCHITECTURE","SECURITY"],ti=50,ni=180,ri=new Set(["the","and","for","with","from","into","this","that","about","before","after","fix","update","change","make","during"]);function ar(e){return new Set((e.toLowerCase().match(/[a-z0-9]{3,}/g)??[]).filter(t=>!ri.has(t)))}function si(e,t){let n=e.replace(/\s+/g," ").trim();if(Buffer.byteLength(n,"utf8")<=t)return n;let r="...",s=Buffer.byteLength(r,"utf8"),i=0,o="";for(let a of n){let c=Buffer.byteLength(a,"utf8");if(i+c+s>t)break;o+=a,i+=c}return o.trimEnd()+r}function ii(e,t){let n=ar(e);if(n.size<2)return!1;let r=ar([t.task_context,t.observation,t.label,t.failure_signature??""].join(" ")),s=0;for(let i of n)if(r.has(i)&&++s>=2)return!0;return!1}function tt(e,t={}){let n=t.workspace??null,r=h(t.artifact),s=t.format??"json",i=String(t.query??"").trim().slice(0,4e3),o=String(t.agent_id??t.agentId??"agent"),a=n??t.cwd??process.cwd(),c=[];try{let u=et(e,{agentId:o,workspacePath:n,artifact:r,unreadOnly:!0,markRead:!1,limit:5,cwd:a});for(let l of u.signals){let _=l.to_agent?`to ${l.to_agent}`:"broadcast",d=l.files.length>0?` files=${l.files.length}[${Fe(l.files[0],48)}]`:"",E=l.body?` \u2014 ${Fe(l.body,60)}`:"";c.push({kind:"notification",text:`\u{1F4E8} ${l.kind} from ${l.from_agent} (${_})${d}: ${Fe(l.subject,72)}${E}`,importance:l.importance})}}catch{}try{let u=["state = 'ACTIVE'","label = 'OVERRIDE'"],l=[];n&&(u.push("(workspace_path = ? OR workspace_path IS NULL)"),l.push(n)),r&&(u.push("(artifact = ? OR artifact IS NULL)"),l.push(r));let _=e.prepare(`SELECT memory_id, observation, importance
       FROM memories
       WHERE ${u.join(" AND ")}
       ORDER BY importance DESC, last_accessed_at DESC
       LIMIT 2`).all(...l);for(let d of _)c.push({kind:"memory",text:`OVERRIDE(${d.importance}): ${d.observation.slice(0,120)}`,importance:d.importance})}catch{}try{let u=[];if(s==="hook"){if(i){let _=Wt(e,{query:i,limit:ti,minImportance:6,label:[...jt],workspacePath:n,artifact:r,repo:t.repo??null,ref:t.ref??null,recordAccess:!1,cwd:a}).memories.find(d=>ii(i,d));_&&(u=[_])}}else{let l=["state = 'ACTIVE'","importance >= 6",`label IN (${jt.map(()=>"?").join(",")})`],_=[...jt];n&&(l.push("(workspace_path = ? OR workspace_path IS NULL)"),_.push(n)),r&&(l.push("(artifact = ? OR artifact IS NULL)"),_.push(r)),u=e.prepare(`SELECT memory_id, task_context, observation, label, importance, failure_signature
         FROM memories
         WHERE ${l.join(" AND ")}
         ORDER BY importance DESC, last_accessed_at DESC
         LIMIT 3`).all(..._)}for(let l of u)c.push({kind:"memory",text:`Memory lead \u2014 verify: ${l.label}(${l.importance}): ${l.observation.slice(0,120)}`,importance:l.importance})}catch{}try{let u=["failure_signature IS NOT NULL","state = 'ACTIVE'"],l=[];n&&(u.push("(workspace_path = ? OR workspace_path IS NULL)"),l.push(n)),r&&(u.push("(artifact = ? OR artifact IS NULL)"),l.push(r));let _=e.prepare(`SELECT failure_signature, count(*) AS freq, avg(importance) AS avg_imp
       FROM memories
       WHERE ${u.join(" AND ")}
       GROUP BY failure_signature HAVING freq >= 2
       ORDER BY freq * avg_imp DESC LIMIT 1`).get(...l);_&&c.push({kind:"weakness",text:`\u26A0\uFE0F Recurring: ${_.failure_signature} (${_.freq}x, avg imp ${Math.round(_.avg_imp)})`})}catch{}try{let u=Ct(e,{workspacePath:n,artifact:r,cwd:a});u>0&&c.push({kind:"refinement",text:`\u{1F4CB} ${u} open refinement(s) pending`})}catch{}if(c.length===0)return{ok:!0,count:0,notifications:[]};let p={ok:!0,count:c.length,notifications:c};if(s==="hook"){let u=c.slice(0,5).map(m=>({...m,text:si(m.text,ni)}));p.count=u.length,p.notifications=u;let _=[`\u{1F9E0} Brief (${u.length}${c.length>u.length?`/${c.length}`:""}):`,...u.map(m=>`  \u2022 ${m.text}`)].join(`
`),d=String(t.session_id??t.sessionId??"-"),E=b({workspace_path:n,artifact:r,repo:t.repo??null,ref:t.ref??null},a),f=JSON.stringify([d,E.workspace_path,E.artifact,E.repo,E.ref]),g=ei("sha256").update(_).digest("hex"),R={consumerId:o,channel:"briefing",scopeKey:f};if(gt(e,R)===g)return{ok:!0,count:0,notifications:[]};mt(e,{...R,fingerprint:g}),p.additionalContext=_}return p}function cr(e){let t=[];for(let n of String(e).split(`
`)){if(!n||n.length<4)continue;let r=n.slice(0,2),s=n.slice(3);if(r.includes("R")||r.includes("C")){let o=s.indexOf(" -> ");o>=0&&(s=s.slice(o+4))}let i=s.trim();i&&t.push(i)}return t}function lr(e){if(!e)return[];try{let t=Qs("git",["-C",e,"status","--porcelain=v1"],{encoding:"utf8",timeout:5e3});return t.status!==0?[]:cr(String(t.stdout))}catch{return[]}}import{randomUUID as oi}from"node:crypto";import{isAbsolute as qu,resolve as ai}from"node:path";function De(e,t={}){let n=String(t.agent_id??t.agentId??"agent"),r=t.reason?String(t.reason):null,s=t.workspace??t.workspace_path??t.workspacePath,i=typeof s=="string"&&s.trim()?ai(s.trim()):null,o=b({workspace_path:i,artifact:h(t.artifact),repo:t.repo??null,ref:t.ref??null},t.cwd??process.cwd()),a=o.workspace_path??i??process.cwd(),c=[...new Set([a,i].filter(O=>!!O))],p=o.artifact,u=c.map(()=>"?").join(","),l=e.prepare(`SELECT tr.run_id, tr.rationale, tr.test_plan, tr.context_ref, tr.status, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
     FROM task_runs tr
     WHERE tr.agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${u}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`).all(n,...c,p,p),_=[...new Set(l.flatMap(O=>X(O.files_json)))],d=lr(a),E=l.filter(O=>O.status==="ACTIVE").length,f=l.filter(O=>O.status==="PENDING").length,g=0;try{let O=["novelty_score IS NOT NULL","novelty_score < 0.2","state = 'ACTIVE'"],y=[];a&&(O.push("(workspace_path = ? OR workspace_path IS NULL)"),y.push(a)),p&&(O.push("(artifact = ? OR artifact IS NULL)"),y.push(p)),g=e.prepare(`SELECT COUNT(*) AS c FROM memories WHERE ${O.join(" AND ")}`).get(...y).c}catch{}if(l.length===0&&d.length===0)return{ok:!0,captured:!1,refinement_id:null,pending_runs:0,active_runs:0,files:[],dirty_files:[],reason:r,consolidation_opportunities:g};let R=T(),m="ref_"+oi().replace(/-/g,""),I=[...new Set([..._,...d])],k=I.slice(0,Lt),x=d.slice(0,Lt),W=l.slice(0,Fn).map(O=>{let y=X(O.files_json),z=y.slice(0,vn),F=y.length-z.length,ie=y.length>0?` files=${z.join(", ")}${F>0?` (+${F} more)`:""}`:"",ee=O.context_ref?` plan=${O.context_ref}`:"";return`${O.status} ${O.run_id}: ${yt(O.rationale)}; verify=${yt(O.test_plan)}${ee}${ie}`}),re=l.length-W.length,q=[`Session capture for ${n}${r?` (${r})`:""}.`,`Unresolved runs: ${l.length} (${E} active, ${f} pending).`,Dt("Dirty files",d),W.length>0?`Run details: ${W.join(" | ")}${re>0?` | ${re} more runs omitted`:""}`:null].filter(Boolean).join(" "),se=[`Review session handoff for ${n}: ${E} active and ${f} pending runs remain.`,Dt("Touched files",I),d.length>0?"Check dirty git state before continuing.":null,f>0?"Run the recorded verification before claiming completion.":null].filter(Boolean).join(" "),G=e.prepare(`SELECT refinement_id FROM refinements
      WHERE agent_id = ? AND workspace_path = ? AND artifact IS ? AND repo IS ? AND ref IS ?
        AND quality = 'handoff' AND state IN ('open', 'ongoing')
        AND files_json = ? AND reasoning = ? AND remember = ?
      ORDER BY datetime(updated_at) DESC LIMIT 1`).get(n,a,p,o.repo,o.ref,JSON.stringify(k),q,se);return G?{ok:!0,captured:!1,deduplicated:!0,refinement_id:G.refinement_id,pending_runs:f,active_runs:E,files:k,dirty_files:x,file_count:I.length,dirty_file_count:d.length,omitted_files:Math.max(0,I.length-k.length),omitted_dirty_files:Math.max(0,d.length-x.length),reason:r,consolidation_opportunities:g}:(e.prepare(`INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       artifact, files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`).run(m,n,a,o.repo,o.ref,p,JSON.stringify(k),q,se,R,R),{ok:!0,captured:!0,refinement_id:m,pending_runs:f,active_runs:E,files:k,dirty_files:x,file_count:I.length,dirty_file_count:d.length,omitted_files:Math.max(0,I.length-k.length),omitted_dirty_files:Math.max(0,d.length-x.length),reason:r,consolidation_opportunities:g})}import{existsSync as ci}from"node:fs";import{isAbsolute as li,resolve as ur}from"node:path";var dr=1,pr=3650;function Ce(e,t,n,r){let s=e[t]??e[n]??r,i=Number(s);if(!Number.isInteger(i)||i<dr||i>pr)throw new Error(`${t} must be an integer in ${dr}..${pr}`);return i}function _r(e,t={}){let n=Number(t.pressure_age_days??t.pressureAgeDays??1),r=Number.isFinite(n)?Math.min(3650,Math.max(1,Math.floor(n))):1,s=new Date(Date.now()-r*864e5).toISOString(),i=typeof t.workspace=="string"?t.workspace:typeof t.workspace_path=="string"?t.workspace_path:typeof t.workspacePath=="string"?t.workspacePath:null,o=i?t.workspace_normalized===!0?ur(i):N(i,i):null,a=h(t.artifact),c=[],p=[];o&&(c.push("workspace_path = ?"),p.push(o)),a&&(c.push("artifact = ?"),p.push(a));let u=c.length>0?` AND ${c.join(" AND ")}`:"",l=e.prepare(`SELECT COUNT(*) AS count FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${u}`).get(s,...p).count,_=e.prepare(`SELECT run_id FROM task_runs
      WHERE status = 'PENDING' AND updated_at < ?${u}
      ORDER BY datetime(updated_at), run_id LIMIT 3`).all(s,...p),d=e.prepare(`SELECT COUNT(*) AS count FROM signals
      WHERE status = 'open' AND created_at < ?${u}`).get(s,...p).count,E=e.prepare(`SELECT signal_id FROM signals
      WHERE status = 'open' AND created_at < ?${u}
      ORDER BY datetime(created_at), signal_id LIMIT 3`).all(s,...p),f=e.prepare(`SELECT m.memory_id, r.reference
       FROM memories m
       JOIN memory_refs r ON r.memory_id = m.memory_id
      WHERE m.state = 'ACTIVE'
        AND r.reference LIKE 'file:%'
        AND COALESCE(m.updated_at, m.created_at) < ?
        ${u.replaceAll("workspace_path","m.workspace_path").replaceAll("artifact","m.artifact")}
      ORDER BY datetime(COALESCE(m.updated_at, m.created_at)), m.memory_id
      LIMIT 1000`).all(s,...p),g=new Set;for(let R of f){let m=R.reference.slice(5).replace(/(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)$/,""),I=li(m)?m:ur(o??process.cwd(),m);ci(I)||g.add(R.memory_id)}return{pressure_age_days:r,cutoff:s,stale_pending_runs:l,stale_open_signals:d,stale_missing_refs:g.size,samples:{run_ids:_.map(R=>R.run_id),signal_ids:E.map(R=>R.signal_id),memory_ids:[...g].slice(0,3)}}}function Gt(e,t={}){let n=Ce(t,"retention_days","retentionDays",90),r=Ce(t,"refinement_handoff_retention_days","refinementHandoffRetentionDays",7),s=Ce(t,"refinement_done_retention_days","refinementDoneRetentionDays",30),i=Ce(t,"operational_retention_days","operationalRetentionDays",90);Ce(t,"pressure_age_days","pressureAgeDays",1);let o=typeof t.workspace=="string"?t.workspace:typeof t.workspace_path=="string"?t.workspace_path:typeof t.workspacePath=="string"?t.workspacePath:null,a=o?N(o,o):null,c=h(t.artifact),p=new Date().toISOString(),u=new Date(Date.now()-n*864e5).toISOString(),l=new Date(Date.now()-r*864e5).toISOString(),_=new Date(Date.now()-s*864e5).toISOString(),d=new Date(Date.now()-i*864e5).toISOString(),E=_r(e,t),f={pressure_age_days:E.pressure_age_days,stale_pending_runs:E.stale_pending_runs,stale_open_signals:E.stale_open_signals,stale_missing_refs:E.stale_missing_refs,pressure_samples:E.samples},g=[],R=[];a&&(g.push("workspace_path = ?"),R.push(a)),c&&(g.push("artifact = ?"),R.push(c));let m=g.length>0?` AND ${g.join(" AND ")}`:"",I=[],k=[];a&&(I.push("workspace_path = ?"),k.push(a)),c&&(I.push("artifact = ?"),k.push(c));let x=I.length>0?` AND ${I.join(" AND ")}`:"";if(t.dry_run){let F=e.prepare(`SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${m}`).get(p,...R).c,ie=e.prepare(`SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?${m}`).get(u,...R).c,ee=Ze(e,{...a?{workspace:a}:{},...c?{artifact:c}:{},expired_only:!0,dry_run:!0}),lt=ee.would_prune??0,ue=e.prepare(`SELECT COUNT(*) AS c FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${x}`).get(l,_,...k).c,oe=e.prepare(`SELECT COUNT(*) AS c FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${m}`).get(d,...R).c,$=e.prepare(`SELECT memory_id FROM memories
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${m}
       ORDER BY datetime(valid_to), memory_id LIMIT ?`).all(p,...R,20).map(v=>v.memory_id),de=e.prepare(`SELECT memory_id FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${m}
       ORDER BY datetime(updated_at), memory_id LIMIT ?`).all(u,...R,20).map(v=>v.memory_id),pe=e.prepare(`SELECT refinement_id FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${x}
       ORDER BY datetime(updated_at), refinement_id LIMIT ?`).all(l,_,...k,20).map(v=>v.refinement_id),ae=e.prepare(`SELECT run_id FROM task_runs
       WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
         AND status IN ('SUCCESS','FAILED') AND updated_at < ?${m}
       ORDER BY datetime(updated_at), run_id LIMIT ?`).all(d,...R,20).map(v=>v.run_id);return{ok:!0,archived_memories:0,pruned_old:0,pruned_locks:0,pruned_refinements:0,pruned_runs:0,fts_rebuilt:!1,dry_run:!0,would_archive:F,would_prune_old:ie,would_prune_locks:lt,would_prune_refinements:ue,would_prune_runs:oe,candidate_limit:20,candidate_ids:{expire_memory_ids:$,purge_memory_ids:de,lock_ids:ee.lock_ids??[],refinement_ids:pe,run_ids:ae},...f}}let W={changes:0},re={changes:0},q=0,se={changes:0},G={changes:0},O=!1,y=!e.isTransaction;y&&e.exec("BEGIN IMMEDIATE");try{W=e.prepare(`UPDATE memories
       SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${m}`).run(p,p,p,...R),re=e.prepare(`DELETE FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${m}`).run(u,...R),q=Ze(e,{...a?{workspace:a}:{},...c?{artifact:c}:{},expired_only:!0}).pruned_locks,se=e.prepare(`DELETE FROM refinements
       WHERE ((quality = 'handoff' AND state = 'done' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${x}`).run(l,_,...k),G=e.prepare(`DELETE FROM task_runs
      WHERE task_id IS NULL AND origin IN ('WORK','HOOK')
        AND status IN ('SUCCESS','FAILED') AND updated_at < ?${m}`).run(d,...R),B(e)&&(Ie(e),O=!0),y&&e.exec("COMMIT")}catch(z){if(y)try{e.exec("ROLLBACK")}catch{}throw z}return y&&ft(e),{ok:!0,archived_memories:W.changes,pruned_old:re.changes,pruned_locks:q,pruned_refinements:se.changes,pruned_runs:G.changes,fts_rebuilt:O,...f}}function Yt(e,t){let n=t.agentId,r=t.agentName??"",s=t.workspacePath?N(t.workspacePath,t.workspacePath):null,i=h(t.artifact),o=t.context??null,a=T();return e.prepare(nr).run(n,r,s,i,o,a,a),{agent_id:n,agent_name:r,workspace_path:s,artifact:i,context:o,registered_at:a,last_seen_at:a}}import{randomUUID as gr}from"node:crypto";import{randomUUID as Ei}from"node:crypto";function Kt(e,t){let n=new Map(t.map(r=>[r,[]]));for(let r=0;r<t.length;r+=500){let s=t.slice(r,r+500),i=e.prepare(`SELECT run_id, file_path FROM run_files
       WHERE run_id IN (${s.map(()=>"?").join(",")})
       ORDER BY file_path`).all(...s);for(let o of i)n.get(o.run_id)?.push(o.file_path)}return n}function nt(e,t,n){e.prepare("DELETE FROM locks WHERE run_id = ?").run(t),e.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
    WHERE run_id = ? AND ended_at IS NULL`).run(n,n,n,t)}function Vt(e,t,n,r,s){let i=e.prepare("SELECT task_id FROM task_runs WHERE run_id = ?").get(t);!i?.task_id||e.prepare(`UPDATE tasks SET status = 'FAILED', updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status IN ('IN_PROGRESS', 'VERIFY')`).run(r,r,i.task_id).changes===0||(e.prepare("DELETE FROM task_claims WHERE task_id = ?").run(i.task_id),e.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, 'ABANDONED', ?, ?)`).run(`tevt_${Ei().replace(/-/g,"")}`,i.task_id,t,n,s,r))}function rt(e,t={}){let n=t.workspacePath?N(t.workspacePath,t.workspacePath):null,r=["status = 'PENDING'"],s=[],i=null;if(t.olderThanDays!=null){if(!Number.isFinite(t.olderThanDays)||t.olderThanDays<1)throw new Error("olderThanDays must be a finite number >= 1");i=new Date(Date.now()-Math.floor(t.olderThanDays)*864e5).toISOString(),r.push("updated_at < ?"),s.push(i)}if(t.origins?.length){let d=[...new Set(t.origins)];if(d.some(E=>!["TASK","WORK","HOOK"].includes(E)))throw new Error("origins must contain only TASK, WORK, or HOOK");r.push(`origin IN (${d.map(()=>"?").join(",")})`),s.push(...d)}let o=null;if(t.before){let d=new Date(t.before);if(Number.isNaN(d.getTime()))throw new Error("before must be a valid ISO timestamp");o=d.toISOString(),r.push("created_at < ?"),s.push(o)}t.agentId&&(r.push("agent_id = ?"),s.push(t.agentId)),n&&(r.push("workspace_path = ?"),s.push(n));let a=h(t.artifact);a&&(r.push("(artifact = ? OR artifact IS NULL)"),s.push(a));let c=e.prepare(`SELECT run_id, agent_id, status, test_plan, context_ref, rationale, workspace_path, artifact, created_at
     FROM task_runs
     WHERE ${r.join(" AND ")}
     ORDER BY created_at ASC`).all(...s),p=Kt(e,c.map(d=>d.run_id)),u=c.map(d=>({run_id:d.run_id,agent_id:d.agent_id,status:d.status,test_plan:d.test_plan,context_ref:d.context_ref,rationale:d.rationale,target_files:p.get(d.run_id)??[],workspace_path:d.workspace_path,artifact:d.artifact,created_at:d.created_at}));if(t.abandon&&u.length>0){let d=T(),E=e.prepare(qn),f=e.prepare(Jn);for(let g of u){E.run(d,g.run_id),nt(e,g.run_id,d),Vt(e,g.run_id,g.agent_id,d,"pending run abandoned by verification audit");try{f.run("evt_"+gr().replace(/-/g,""),g.run_id,g.agent_id,d)}catch{}}}let l=[];try{let d=T(),E=["ai.status = 'ACTIVE'","EXISTS (SELECT 1 FROM run_files any_rf WHERE any_rf.run_id = ai.run_id)",`NOT EXISTS (
        SELECT 1 FROM run_files active_rf
        WHERE active_rf.run_id = ai.run_id AND active_rf.ended_at IS NULL
          AND active_rf.expires_at > ?
      )`,`NOT EXISTS (
        SELECT 1 FROM task_claims tc
        WHERE tc.run_id = ai.run_id AND tc.expires_at > ?
      )`],f=[d,d];if(t.agentId&&(E.push("ai.agent_id = ?"),f.push(t.agentId)),n&&(E.push("ai.workspace_path = ?"),f.push(n)),a&&(E.push("(ai.artifact = ? OR ai.artifact IS NULL)"),f.push(a)),i&&(E.push("ai.updated_at < ?"),f.push(i)),t.origins?.length){let m=[...new Set(t.origins)];E.push(`ai.origin IN (${m.map(()=>"?").join(",")})`),f.push(...m)}o&&(E.push("ai.created_at < ?"),f.push(o));let g=e.prepare(`SELECT ai.run_id, ai.agent_id, ai.rationale, ai.context_ref, ai.workspace_path, ai.artifact, ai.created_at
       FROM task_runs ai
       WHERE ${E.join(" AND ")}
       ORDER BY ai.created_at ASC`).all(...f),R=Kt(e,g.map(m=>m.run_id));for(let m of g){let I=Date.now()-new Date(m.created_at).getTime();l.push({run_id:m.run_id,agent_id:m.agent_id,status:"ACTIVE",rationale:m.rationale,context_ref:m.context_ref,target_files:R.get(m.run_id)??[],workspace_path:m.workspace_path,artifact:m.artifact,created_at:m.created_at,age_hours:Math.round(I/36e5*10)/10})}}catch(d){if(!(d instanceof Error&&d.message.includes("no such table")))throw d}if(t.abandon&&l.length>0){let d=T(),E=e.prepare(zn),f=e.prepare(Zn);for(let g of l){E.run(d,g.run_id),nt(e,g.run_id,d),Vt(e,g.run_id,g.agent_id,d,"stale task run abandoned by verification audit");try{f.run("evt_"+gr().replace(/-/g,""),g.run_id,g.agent_id,d)}catch{}}}let _=u.length+l.length;return{ok:!0,unverified:u,stale_active:l,count:_}}import{randomUUID as kp}from"node:crypto";var me="__octocode_hook_host",qt="__octocode_skill_root";function Sr(){return new Promise(e=>{let t="";process.stdin.setEncoding("utf8"),process.stdin.on("data",n=>{t+=n}),process.stdin.on("end",()=>e(t)),process.stdin.on("error",()=>e(t))})}function Tr(e){try{let t=JSON.parse(e||"{}");return t&&typeof t=="object"?t:{}}catch{return e.trim()?{input:e}:{}}}function V(e){return e&&typeof e=="object"?e:{}}function J(e){return e.tool_input??e.input??e.args??e}function hi(e){let t=J(e),n=V(t);return n===e||Object.keys(n).length===0?t:{...e,...n}}var mr=!1;function K(...e){for(let t of e)if(typeof t=="string"&&t.trim())return t.trim();return null}function zt(e){if(typeof e!="string")return null;let t=e.trim().toLowerCase();return t==="claude"||t==="codex"||t==="cursor"?t:null}function Z(e){let t=zt(e[me]??process.env.OCTOCODE_AGENT_HOST??e.host??e.client);if(t)return t;let n=K(e.hook_event_name,e.eventName)??"";return n&&n[0]===n[0]?.toLowerCase()?"cursor":"claude"}function Jt(e){return K(e[qt],process.env.OCTOCODE_SKILL_ROOT)}function hr(e,t,n){return e==="cursor"?t==="sessionStart"?{additional_context:n}:{permission:"allow",agent_message:n}:{hookSpecificOutput:{hookEventName:t,additionalContext:n}}}function le(e,t,n){return e!=="cursor"?{exitCode:2,stderr:n}:t==="stop"?{exitCode:0,payload:{followup_message:n}}:{exitCode:0,payload:{permission:"deny",user_message:n,agent_message:n}}}function Rr(e){process.stdout.write(`${JSON.stringify(e)}
`)}function st(e,t,n){Rr(hr(Z(e),t,n))}function Se(e){return e.payload&&Rr(e.payload),e.stderr&&console.error(e.stderr),e.exitCode}function L(e){let t=V(J(e)),n=K(process.env.OCTOCODE_AGENT_ID,e.agent_id,e.agentId,t.agent_id,t.agentId,e.session_id,e.sessionId,t.session_id,t.sessionId);if(n)return n;let r=K(e[me],process.env.OCTOCODE_AGENT_HOST,e.host,e.client,e.source,e.context)??"shell",s=`${r}\0${C(e)??process.cwd()}`,i=gi("sha1").update(s).digest("hex").slice(0,12),o=`hook:${r.replace(/[^a-zA-Z0-9_.:-]/g,"_")}:${i}`;return mr||(mr=!0,console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${o}". Set OCTOCODE_AGENT_ID for reliable multi-agent awareness.`)),o}function xe(e){let t=V(J(e));return K(e.session_id,e.sessionId,t.session_id,t.sessionId)}function Ir(e){let t=V(J(e)),n=K(e.prompt,e.user_prompt,e.userPrompt,e.text,e.message,typeof e.input=="string"?e.input:null,t.prompt,t.user_prompt,t.userPrompt,t.text,t.message);return n?n.slice(0,4e3):null}function it(e){let t=V(J(e));return K(xe(e),e.transcript_path,e.transcriptPath,e.conversation_id,e.conversationId,e.thread_id,e.threadId,t.transcript_path,t.transcriptPath,t.conversation_id,t.conversationId,t.thread_id,t.threadId)}function Ri(e){let t=V(J(e));return K(e.tool_name,e.toolName,e.name,t.tool_name,t.toolName)??""}function Nr(e,t){let n=Ri(e),r=t.map(a=>a.split("/").pop()||a),s=r.slice(0,3).join(", "),i=r.length>3?` +${r.length-3} more`:"";return`auto: ${n?`${n}`:"edit"} ${s}${i} (lifecycle hook)`}function Zt(e,t){let n=U(t),r=[...new Set(e.map(o=>j(o,t)))],s=r.slice(0,3).map(o=>Si(n,o)||mi(o)).join(", "),i=r.length>3?` (+${r.length-3} more)`:"";return`Verify ${s||"the edited files"}${i}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`}function kr(e){let t=process.env.OCTOCODE_AGENT_NAME??e.agent_name??e.agentName??e.agent_display_name??e.agentDisplayName;return typeof t=="string"&&t.trim()?t.trim():""}function C(e){let t=e.cwd??e.workspace??e.workspacePath;return typeof t=="string"&&t.trim()?t.trim():null}function D(e){let t=V(J(e)),n=process.env.OCTOCODE_ARTIFACT??process.env.OCTOCODE_PACKAGE??process.env.OCTOCODE_SERVICE??e.artifact??e.package??e.service??t.artifact??t.package??t.service;return typeof n=="string"&&n.trim()?n.trim():null}function Me(e){return typeof e.reason=="string"?e.reason:""}function Or(e){return!!e.stop_hook_active}function ot(e){let t=hi(e),n=V(t),r=e.tool_name??e.toolName??e.name??n.tool_name??n.toolName??"";return je(r,t,{assumeWrite:!0})}function j(e,t=process.cwd()){return U(Ti(t,e))}function Q(){return Et(Y(null))}import{basename as io,resolve as oo}from"node:path";import{fileURLToPath as ao}from"node:url";import{relative as Ji}from"node:path";import{createHash as wr}from"node:crypto";import{mkdirSync as Ii,readFileSync as Ni,writeFileSync as ki}from"node:fs";import{basename as Oi,dirname as wi,join as Ar,relative as Ai,resolve as Li}from"node:path";function yi(){let e=Ar(wi(Y(null)),"hook-state","peers");return Ii(e,{recursive:!0}),e}function Di(e,t,n){return wr("sha1").update(JSON.stringify({agent:L(e),workspace:N(n,n)??Li(n),artifact:D(e),files:t.map(r=>j(r,n)).sort()})).digest("hex")}function Ci(e){return wr("sha1").update(JSON.stringify(e.map(t=>({agent:t.agent_id,file:t.file_path,task:t.task_id,origin:t.origin,rationale:t.rationale,exclusive:t.exclusive})).sort((t,n)=>JSON.stringify(t).localeCompare(JSON.stringify(n))))).digest("hex")}function xi(e){let t=e.task_id??e.origin,n=e.rationale.replace(/\s+/g," ").trim().slice(0,40);return`${e.agent_id}:${t}${n?`(${n})`:""}`}function Lr(e,t,n,r){let s=new Set(t.map(E=>j(E,n))),i=r.filter(E=>E.agent_id!==L(e)&&s.has(E.file_path)),o=Di(e,t,n),a=Ar(yi(),`${o}.txt`),c=Ci(i),p=null;try{p=Ni(a,"utf8").trim()}catch{}if(p===c||(ki(a,c,"utf8"),i.length===0))return null;let u=i.slice(0,3).map(xi).join("; "),l=i.length>3?` +${i.length-3}`:"",_=U(n);return`AWARE ${t.slice(0,2).map(E=>Ai(_,j(E,n))||Oi(E)).join(",")} | peers ${u}${l}`}function Mi(e,t){let n=process.env.OCTOCODE_AGENT_CONTEXT??e[me]??process.env.OCTOCODE_AGENT_HOST??e.context??e.host??e.client??e.source;return typeof n=="string"&&n.trim()?n.trim():t}function ne(e,t,n){try{Yt(e,{agentId:L(t),agentName:kr(t),workspacePath:C(t),artifact:D(t),context:Mi(t,n)})}catch{}}function yr(e){let t=C(e),n=D(e);return{...t?{workspacePath:t}:{},...n?{artifact:n}:{}}}import{createHash as Qt}from"node:crypto";import{closeSync as Pi,mkdirSync as bi,openSync as Fi,readFileSync as Dr,renameSync as vi,statSync as Ui,unlinkSync as en,writeFileSync as Cr}from"node:fs";import{dirname as Hi,join as xr,resolve as at}from"node:path";var Wi=10*6e4,Mr=new Int32Array(new SharedArrayBuffer(4)),Pr=10,$i=2e3,Xi=3e4,Bi=5e3;function ji(e){let t=e,n=t&&typeof t=="object"?`${t.errstr??""} ${t.message??""}`:String(e);return t?.errcode===5||/database is (?:locked|busy)/i.test(n)}function P(e){let t=Date.now()+Bi;for(;;)try{return e()}catch(n){if(!ji(n)||Date.now()>=t)throw n;Atomics.wait(Mr,0,0,Pr)}}function Gi(){let e=xr(Hi(Y(null)),"hook-state","runs");return bi(e,{recursive:!0}),e}function tn(e){return xr(Gi(),`${e}.json`)}function Yi(e){if(!Number.isSafeInteger(e)||e<=0)return!1;try{return process.kill(e,0),!0}catch(t){return t.code!=="ESRCH"}}function Ki(e){try{let t=Number.parseInt(Dr(e,"utf8"),10),n=Date.now()-Ui(e).mtimeMs>Xi;return Yi(t)&&!n?!1:(en(e),!0)}catch(t){return t.code==="ENOENT"}}function nn(e,t){let n=`${tn(e)}.lock`,r=Date.now()+$i;for(;;)try{let s=Fi(n,"wx",384);try{Cr(s,`${process.pid}
`,"utf8")}finally{Pi(s)}try{return t()}finally{try{en(n)}catch(i){if(i.code!=="ENOENT")throw i}}}catch(s){if(s.code!=="EEXIST")throw s;if(Ki(n))continue;if(Date.now()>=r)throw new Error(`timed out waiting for hook correlation state: ${n}`);Atomics.wait(Mr,0,0,Pr)}}function br(e){try{let t=JSON.parse(Dr(tn(e),"utf8"));if(!Array.isArray(t))return[];let n=Date.now()-Wi;return t.filter(r=>{if(!r||typeof r!="object")return!1;let s=r,i=typeof s.createdAt=="string"?Date.parse(s.createdAt):NaN;return typeof s.runId=="string"&&s.runId.length>0&&Array.isArray(s.files)&&s.files.every(o=>typeof o=="string"&&o.length>0)&&Number.isFinite(i)&&i>=n})}catch{return[]}}function Fr(e,t){let n=tn(e);if(t.length===0){try{en(n)}catch{}return}let r=`${n}.${process.pid}.${Date.now()}.tmp`;Cr(r,JSON.stringify(t,null,2)+`
`,"utf8"),vi(r,n)}function Vi(e){let t=V(J(e));return K(e.tool_use_id,e.toolUseId,e.tool_call_id,e.toolCallId,e.event_id,e.eventId,e.id,t.tool_use_id,t.toolUseId,t.tool_call_id,t.toolCallId,t.event_id,t.eventId,t.id)}function vr(e,t,n){let r=Vi(e),s={agent:L(e),workspace:N(n,n)??at(n),artifact:D(e),event:r,files:r?[]:t.map(i=>j(i,n)).sort()};return Qt("sha1").update(JSON.stringify(s)).digest("hex")}var Ur="hook-scope:";function ct(e,t){let n=it(e);if(!n)return null;let r={agent:L(e),session:n,workspace:N(t,t)??at(t),artifact:h(D(e))};return`${Ur}${Qt("sha1").update(JSON.stringify(r)).digest("hex")}`}function qi(e,t,n){let r=ct(t,n);return r?e.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY updated_at DESC, created_at DESC LIMIT 1`).get(L(t),N(n,n)??at(n),h(D(t)),r)?.run_id??null:null}function zi(e,t){let n=ct(e,t);return n?`aggregate-${Qt("sha1").update(n).digest("hex")}`:null}function Hr(e,t,n,r){let s=ct(t,n),i=()=>{let a=qi(e,t,n),c=fe(e,{agentId:L(t),sessionId:xe(t),workspacePath:n,artifact:D(t),runId:a??void 0,rationale:Nr(t,r),testPlan:Zt(r,n),contextRef:s??void 0,targetFiles:r,origin:"HOOK",source:"HOOK",ttlMs:10*6e4});return c.ok&&a&&te(e,{agentId:L(t),runId:a,ttlMs:10*6e4}),c},o=zi(t,n);return o?nn(o,i):i()}function Wr(e,t,n){if(!rn(e,t))return;let r=e.prepare("SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path").all(t);e.prepare("UPDATE task_runs SET test_plan = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE run_id = ? AND origin = 'HOOK'").run(Zt(r.map(s=>s.file_path),n),t)}function rn(e,t){let n=e.prepare("SELECT origin, context_ref FROM task_runs WHERE run_id = ?").get(t);return n?.origin==="HOOK"&&n.context_ref?.startsWith(Ur)===!0}function Pe(e,t,n){let r=ct(t,n);if(!r)return[];let s=e.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY created_at`).all(L(t),N(n,n)??at(n),h(D(t)),r),i=[];for(let o of s)ge(e,{agentId:L(t),runId:o.run_id}),i.push(o.run_id);return i}function sn(e,t,n,r){let s=vr(e,t,n);nn(s,()=>{let i=br(s);i.push({runId:r,files:t.map(o=>j(o,n)),createdAt:new Date().toISOString()}),Fr(s,i.slice(-20))})}function $r(e,t,n,r){let s=vr(t,n,r);return nn(s,()=>{let o=br(s).filter(c=>{let p=new Set(qe(e,{agentId:L(t),workspacePath:r,artifact:D(t),runId:c.runId,activeOnly:!0}).files.map(u=>u.file_path));return c.files.every(u=>p.has(u))}),a=o.pop()??null;return Fr(s,o),a?.runId??null})}function on(e,t){let n=t.files.map(o=>j(o,t.workspacePath));if(n.length===0)return null;let r=qe(e,{agentId:t.agentId,workspacePath:t.workspacePath,artifact:t.artifact,activeOnly:!0}).files.filter(o=>t.origins.includes(o.origin)),s=new Map;for(let o of r){let a=s.get(o.run_id)??new Set;a.add(o.file_path),s.set(o.run_id,a)}let i=[...s].filter(([,o])=>n.every(a=>o.has(a)));return i.length===1?i[0][0]:null}function Xr(e,t){return e.prepare("SELECT origin FROM task_runs WHERE run_id = ?").get(t)?.origin??null}async function Br(e){let t=ot(e);if(t.length===0)return 0;let n=C(e)??process.cwd(),r=Je({targetFiles:t,skillRoot:Jt(e),cwd:n});if(r)return Se(le(Z(e),"pre-edit",`${r} Edit blocked.`));try{let s=Q();ne(s,e,"hook:pre-edit");let i=L(e),o=D(e),a=Ae(s,{agentId:i,workspacePath:n,artifact:o}),c=a?null:on(s,{agentId:i,workspacePath:n,artifact:o,files:t,origins:["WORK"]}),p=c?{ok:!0,...te(s,{agentId:i,runId:c,targetFiles:t,ttlMs:10*6e4})}:a?fe(s,{agentId:i,workspacePath:n,artifact:o,runId:a.run_id,targetFiles:t,origin:"HOOK",source:"HOOK",ttlMs:10*6e4}):Hr(s,e,n,t);if(!p.ok){let l=p.conflicts.slice(0,3).map(_=>`${Ji(n,_.file_path)} (${_.agent_id})`).join(", ");return Se(le(Z(e),"pre-edit",`octocode-awareness: exclusive file work blocks this edit${l?`: ${l}`:""}.`))}P(()=>Wr(s,p.run.run_id,n)),sn(e,t,n,p.run.run_id);let u=Lr(e,t,n,p.peers);return u&&st(e,Z(e)==="cursor"?"preToolUse":"PreToolUse",u),0}catch(s){return console.error(`octocode-awareness pre-flight warning (continuing): ${s instanceof Error?s.message:String(s)}`),0}}async function jr(e){let t=ot(e);if(t.length===0)return 0;let n=C(e)??process.cwd(),r=null,s="open database";try{let i=Q();s="register hook agent",P(()=>ne(i,e,"hook:post-edit"));let o=L(e),a=D(e);s="consume correlation",r=P(()=>$r(i,e,t,n)),s="resolve fallback run";let c=r??P(()=>Ae(i,{agentId:o,workspacePath:n,artifact:a}))?.run_id??P(()=>on(i,{agentId:o,workspacePath:n,artifact:a,files:t,origins:["WORK","HOOK"]}));if(!c)return console.error("octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry."),0;s="read run origin";let p=P(()=>Xr(i,c));s="finish work lifecycle",p==="HOOK"&&rn(i,c)?P(()=>te(i,{agentId:o,runId:c,ttlMs:10*6e4})):p==="HOOK"?P(()=>ge(i,{agentId:o,runId:c,targetFiles:t})):P(()=>te(i,{agentId:o,runId:c,targetFiles:t,ttlMs:10*6e4})),r=null,s="write edit log";for(let u of t)P(()=>kt(i,{agentId:o,runId:c,filePath:j(u,n),operation:"update",workspacePath:n,artifact:a}))}catch(i){if(r)try{sn(e,t,n,r)}catch{}console.error(`octocode-awareness post-edit warning during ${s} (continuing): ${i instanceof Error?i.message:String(i)}`)}return 0}async function Gr(e){let t=Je({targetFiles:ot(e),skillRoot:Jt(e),cwd:process.cwd()});return t?Se(le(Z(e),"pre-edit",`${t} Edit blocked.`)):0}import{createHash as Zi}from"node:crypto";import{mkdirSync as Qi,readFileSync as eo,writeFileSync as to}from"node:fs";import{dirname as no,join as ro}from"node:path";async function Yr(e){try{let t=Q();ne(t,e,"hook:stop-verify");let n=P(()=>Pe(t,e,C(e)??process.cwd()));if(process.env.OCTOCODE_NO_VERIFY_GATE==="1")return 0;let r=rt(t,{agentId:L(e),...yr(e)});if(r.count>0){if(Or(e)&&n.length===0)return 0;let s=[...r.unverified.map(a=>`${a.status}:${a.run_id}: ${a.test_plan}`),...r.stale_active.map(a=>`STALE:${a.run_id}: ${a.rationale}`)],i=s.slice(0,3),o=s.length>3?`; +${s.length-3} omitted`:"";return Se(le(Z(e),"stop",`octocode-awareness: concluding with unverified work. ${i.join("; ")}${o}`))}}catch(t){console.error(`octocode-awareness verify warning (continuing): ${t instanceof Error?t.message:String(t)}`)}return 0}function so(e){if(process.env.OCTOCODE_NO_DIGEST==="1"||process.env.OCTOCODE_NOTIFY_RUN_DIGEST!=="1")return null;let t=Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS??4),n=Number.isFinite(t)&&t>0?t*36e5:4*36e5,r=no(Y(null)),s=C(e)??"global",i=Zi("sha256").update(s).digest("hex").slice(0,12),o=ro(r,`.last-digest-preview-${i}-epoch-ms`);try{let a=Q(),c=0;try{c=Number(eo(o,"utf8").trim()||0)}catch{c=0}let p=Date.now();if(!c||p-c>=n){let u=Gt(a,{workspace:C(e),memoryHome:r,dry_run:!0});Qi(r,{recursive:!0}),to(o,String(p),"utf8");let l={archive:u.would_archive??0,memories:u.would_prune_old??0,locks:u.would_prune_locks??0,refinements:u.would_prune_refinements??0};if(Object.values(l).some(_=>_>0))return`Maintenance pressure: archive ${l.archive}, prune memories ${l.memories}, locks ${l.locks}, refinements ${l.refinements}. Review with octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact; apply only after review.`}}catch(a){console.error(`octocode-awareness digest warning (continuing): ${a instanceof Error?a.message:String(a)}`)}return null}async function Kr(e){if(process.env.OCTOCODE_NO_NOTIFY==="1")return 0;let t=so(e);try{let n=Q();ne(n,e,"hook:notify-deliver"),P(()=>Pe(n,e,C(e)??process.cwd()));let s=[tt(n,{agent_id:L(e),session_id:it(e)??void 0,workspace:C(e)??void 0,artifact:D(e)??void 0,query:Ir(e)??void 0,format:"hook"}).additionalContext,t].filter(Boolean).join(`
`);s&&st(e,Z(e)==="cursor"?"sessionStart":"UserPromptSubmit",s)}catch(n){console.error(`octocode-awareness session-capture warning (continuing): ${n instanceof Error?n.message:String(n)}`)}return 0}async function Vr(e){try{let t=Q();ne(t,e,"hook:session-end"),P(()=>Pe(t,e,C(e)??process.cwd())),process.env.OCTOCODE_NO_SESSION_CAPTURE!=="1"&&Me(e)!=="clear"&&De(t,{agent_id:L(e),workspace:C(e)??void 0,artifact:D(e)??void 0,reason:Me(e)||void 0});let n=xe(e);n&&ht(t,{sessionId:n,agentId:L(e),workspacePath:C(e)??process.cwd(),artifact:D(e)})}catch{}return 0}async function qr(e){try{let t=Q();ne(t,e,"hook:session-compact"),P(()=>Pe(t,e,C(e)??process.cwd())),process.env.OCTOCODE_NO_SESSION_CAPTURE!=="1"&&Me(e)!=="clear"&&De(t,{agent_id:L(e),workspace:C(e)??void 0,artifact:D(e)??void 0,reason:Me(e)||"compact"})}catch{}return 0}async function co(e,t,n={}){if(e==="help"||e==="--help"||e==="-h")return process.stdout.write(`usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json
`),0;let r={...Tr(t??await Sr()),...n.host?{[me]:n.host}:{},...n.skillRoot?{[qt]:n.skillRoot}:{}};switch(e){case"pre-edit":return Br(r);case"post-edit":return jr(r);case"harness-guard":return Gr(r);case"stop-verify":return Yr(r);case"notify-deliver":return Kr(r);case"session-compact":return qr(r);case"session-end":return Vr(r);default:return console.error(`unknown hook command: ${e}`),1}}async function lo(){let e=process.argv.indexOf("--host"),t=e>=0?process.argv[e+1]:void 0,n=zt(t);if(t&&!n)return console.error(`unknown hook host: ${t}`),1;let r=process.argv.indexOf("--skill-root"),s=r>=0?process.argv[r+1]:void 0;return co(process.argv[2]??"help",void 0,{...n?{host:n}:{},...s?{skillRoot:s}:{}})}var uo=process.argv[1]?ao(import.meta.url)===oo(process.argv[1]):!1,po=process.argv[1]?/^hook-runner\.(js|mjs|ts)$/.test(io(process.argv[1])):!1;uo&&po&&(process.exitCode=await lo());export{le as hookBlockOutcome,hr as hookContextEnvelope,po as invokedAsHookRunner,uo as isMain,lo as main,co as runHookCommand};
