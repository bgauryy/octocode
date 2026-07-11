import type { DatabaseSync } from 'node:sqlite';
import { DatabaseSync as DatabaseSyncCtor, initDb, hasFts } from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';
import { reflect } from '../src/reflect.js';
import { getWorkspaceStatus } from '../src/maintenance.js';
import { pruneNotifications, agentSignal } from '../src/notifications.js';
import { registerAgent, listAgents } from '../src/agents.js';
import { normalizeNotificationKind, summarizeText } from '../src/helpers.js';
import { normalizeWorkspacePath } from '../src/git.js';
import { ParsedArgs } from './cli-model.js';
import { EmitOptions, die, emit, resolveAgentId } from './cli-routing.js';

export function cmdAgentSignal(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = String(args['action'] ?? '');
  if (!['publish', 'list', 'reply', 'resolve', 'ack'].includes(action)) {
    return emit({ error: '--action must be publish, list, reply, resolve, or ack' }, 1, opts);
  }
  const rawImportance = args['importance'];
  const importance = rawImportance === undefined
    ? undefined
    : typeof rawImportance === 'string' ? Number(rawImportance) : Number.NaN;
  if (importance !== undefined && (!Number.isInteger(importance) || importance < 1 || importance > 10)) {
    die('--importance must be an integer between 1 and 10');
  }
  const rawLimit = args['limit'];
  const limit = rawLimit === undefined
    ? undefined
    : typeof rawLimit === 'string' ? Number(rawLimit) : Number.NaN;
  if (limit !== undefined && (!Number.isInteger(limit) || limit < 1)) {
    die('--limit must be a positive integer');
  }
  const rawTo = args['to_agent'] ?? args['to'];
  const toAgents = Array.isArray(rawTo) ? rawTo : rawTo ? [String(rawTo)] : [];
  const rawFiles = args['file'];
  const files = Array.isArray(rawFiles) ? rawFiles : rawFiles ? [String(rawFiles)] : [];
  const rawRefs = args['ref_id'];
  const refs = Array.isArray(rawRefs) ? rawRefs : rawRefs ? [String(rawRefs)] : [];
  const rawKinds = args['kind'];
  const kinds = Array.isArray(rawKinds) ? rawKinds : rawKinds ? [String(rawKinds)] : [];
  const publishKind = kinds[0]
    ? normalizeNotificationKind(kinds[0])
    : undefined;
  const rawSignalIds = args['signal_id'];
  const signalIds = Array.isArray(rawSignalIds) ? rawSignalIds : rawSignalIds ? [String(rawSignalIds)] : [];
  const compactList = action === 'list' && opts.compact && !Boolean(args['include_bodies']);
  const requestedLimit = limit ?? (compactList ? 3 : undefined);
  const result = agentSignal(db, {
    action: action as import('../src/types.js').AgentSignalAction,
    agentId: resolveAgentId(args),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    kind: publishKind,
    subject: args['subject'] ? String(args['subject']) : undefined,
    body: args['body'] ? String(args['body']) : null,
    toAgents,
    files,
    refs,
    importance,
    inReplyTo: args['in_reply_to'] ? String(args['in_reply_to']) : null,
    threadId: args['thread_id'] ? String(args['thread_id']) : null,
    signalIds,
    unreadOnly: args['all'] ? false : args['unread_only'] as boolean | undefined,
    markRead: Boolean(args['mark_read']),
    kinds: kinds.length ? kinds.map((k) => normalizeNotificationKind(k)) : [],
    limit: compactList && requestedLimit !== undefined ? requestedLimit + 1 : requestedLimit,
  });
  if (compactList && result.action === 'list') {
    const compactLimit = requestedLimit ?? 3;
    const shown = result.signals.slice(0, compactLimit);
    const signals = shown.map((signal) => {
      const shownFiles = signal.files.slice(0, 3);
      return {
        signal_id: signal.signal_id,
        from_agent: signal.from_agent,
        to_agents: signal.to_agents,
        kind: signal.kind,
        subject: signal.subject,
        thread_id: signal.thread_id,
        reply_to: signal.reply_to,
        importance: signal.importance,
        status: signal.status,
        created_at: signal.created_at,
        files: shownFiles,
        file_count: signal.files.length,
        file_omitted_count: Math.max(0, signal.files.length - shownFiles.length),
        has_body: Boolean(signal.body),
      };
    });
    return emit({
      db_path: dbPath,
      action: 'list',
      count: signals.length,
      signals,
      unread_only: result.unread_only,
      bodies: 'omitted',
      has_more: result.signals.length > compactLimit,
      next_limit: result.signals.length > compactLimit ? Math.min(200, compactLimit * 2) : null,
    }, 0, opts);
  }
  if (result.action === 'list' && !Boolean(args['include_bodies'])) {
    return emit({
      db_path: dbPath,
      ...result,
      bodies: 'summarized',
      signals: result.signals.map((signal) => ({
        ...signal,
        body: signal.body == null ? null : summarizeText(signal.body, 160),
      })),
    }, 0, opts);
  }
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdNotifyPrune(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawIds = args['signal_id'];
  const notificationIds = Array.isArray(rawIds) ? rawIds : rawIds ? [String(rawIds)] : [];
  const result = pruneNotifications(db, {
    agentId: resolveAgentId(args),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    notificationIds,
    resolvedOnly: Boolean(args['resolved']),
    olderThanDays: args['older_than_days'] ? parseInt(String(args['older_than_days']), 10) : undefined,
    dryRun: Boolean(args['dry_run']),
  });
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdAgentRegistry(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = String(args['action'] ?? 'list');
  if (!['list', 'register'].includes(action)) {
    return emit({ error: '--action must be list or register' }, 1, opts);
  }

  const workspacePath = args['workspace'] ? String(args['workspace']) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;

  if (action === 'register') {
    if (!args['agent_id']) return emit({ error: '--agent-id is required for register' }, 1, opts);
    const agent = registerAgent(db, {
      agentId: String(args['agent_id']),
      agentName: args['agent_name'] ? String(args['agent_name']) : '',
      workspacePath,
      artifact,
      context: args['context'] ? String(args['context']) : null,
    });
    return emit({ db_path: dbPath, action: 'register', agent }, 0, opts);
  }

  const defaultLimit = opts.compact ? 5 : 50;
  const limit = Math.min(200, Math.max(1, parseInt(String(args['limit'] ?? defaultLimit), 10) || defaultLimit));
  const result = listAgents(db, { workspacePath, artifact });
  const rows = result.agents.slice(0, limit);
  const agents = opts.compact
    ? rows.map((agent) => ({
        agent_id: agent.agent_id,
        agent_name: agent.agent_name,
        last_seen_at: agent.last_seen_at,
        context_summary: agent.context == null ? null : summarizeText(agent.context, 80),
      }))
    : rows;
  return emit({
    db_path: dbPath,
    action: 'list',
    count: agents.length,
    total_count: result.count,
    omitted_count: Math.max(0, result.count - agents.length),
    agents,
    workspace_path: workspacePath,
    artifact,
  }, 0, opts);
}

export function cmdStatus(db: DatabaseSync, dbPath: string, args: ParsedArgs, opts: EmitOptions): number {
  const rawWsPath = args['workspace'] ? String(args['workspace']) : null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;

  const memScope: string[] = [];
  const memScopeBinds: (string | number)[] = [];
  if (wsPath) { memScope.push('(workspace_path = ? OR workspace_path IS NULL)'); memScopeBinds.push(wsPath); }
  if (artifact) { memScope.push('(artifact = ? OR artifact IS NULL)'); memScopeBinds.push(artifact); }
  const memWhere = memScope.length > 0 ? `WHERE ${memScope.join(' AND ')}` : '';
  const memStates = Object.fromEntries(
    (db.prepare(`SELECT state, COUNT(*) AS count FROM memories ${memWhere} GROUP BY state`).all(...memScopeBinds) as Array<{ state: string; count: number }>)
      .map(r => [r.state, r.count])
  );
  const memCount = Object.values(memStates).reduce((sum, count) => sum + count, 0);
  const memLabels = Object.fromEntries(
    (db.prepare(`SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM memories ${memWhere} GROUP BY label`).all(...memScopeBinds) as Array<{ label: string; count: number }>)
      .map(r => [r.label, r.count])
  );
  const limit = Math.min(100, Math.max(1, parseInt(String(args['limit'] ?? '20'), 10) || 20));
  const status = getWorkspaceStatus(db, { workspace_path: wsPath, artifact });
  const lockLimit = opts.compact ? 1 : limit;
  const locks = status.locks.slice(0, lockLimit);

  return emit({
    db_path: dbPath,
    fts_enabled: hasFts(db),
    memory_count: memCount,
    memory_states: memStates,
    memory_labels: memLabels,
    ...status,
    lock_count: status.lock_count,
    lock_shown_count: locks.length,
    lock_omitted_count: Math.max(0, status.lock_count - locks.length),
    locks: opts.compact
      ? locks.map(lock => ({
          file_path: lock.file_path,
          agent_id: lock.agent_id,
          run_id: lock.run_id,
          expires_at: lock.expires_at,
        }))
      : locks,
    workspace_path: wsPath,
    artifact,
  }, 0, opts);
}

export function cmdInit(db: DatabaseSync, dbPath: string, opts: EmitOptions): number {
  const memCount = (db.prepare('SELECT COUNT(*) AS count FROM memories').get() as { count: number }).count;
  return emit({ db_path: dbPath, initialized: true, memory_count: memCount }, 0, opts);
}

export function cmdSelfTest(opts: EmitOptions): number {
  const testDb = new DatabaseSyncCtor(':memory:');
  testDb.exec('PRAGMA foreign_keys = ON');
  initDb(testDb);

  const testAgent = 'self-test-agent';

  // Write
  const { memoryId } = insertMemory(testDb, {
    agentId: testAgent,
    taskContext: 'self-test task',
    observation: 'This is a smoke-test memory.',
    importance: 7,
    label: 'GOTCHA',
    tags: ['smoke-test'],
  });

  // Get
  const { memories: results } = getMemory(testDb, { query: 'smoke-test', limit: 5 });
  if (results.length === 0) {
    return emit({ ok: false, error: 'FTS recall returned no results' }, 1, opts);
  }

  // Reflect (direct call — no stdout patching)
  const reflectResult = reflect(testDb, {
    agentId: testAgent, task: 'self-test', outcome: 'worked', fixRepo: 'test fix',
  });

  return emit({
    ok: true,
    db: ':memory:',
    fts_enabled: hasFts(testDb),
    memory_written: memoryId,
    memory_recalled: results[0]!.memory_id,
    reflection_memory: reflectResult.learning_memory_id,
    refinement_id: reflectResult.repo_fix_refinement_id,
    checks: {
      write: Boolean(memoryId),
      fts_recall: results.length > 0,
      scoring: typeof results[0]!.score === 'number',
      refinement: Boolean(reflectResult.repo_fix_refinement_id),
    },
  }, 0, opts);
}
