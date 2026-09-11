import type { DatabaseSync } from 'node:sqlite';
import { DatabaseSync as DatabaseSyncCtor } from '@octocodeai/agent-contracts/sqlite';
import { initDb } from '../db-init.js';
import { hasFts } from '../db-maintenance.js';
import { insertMemory } from '../memory-write.js';
import { getMemory } from '../memory-recall.js';
import { getWorkspaceStatus } from '../maintenance-workspace.js';
import { pruneNotifications, agentSignal } from '../notifications-signals.js';
import { registerAgent, listAgents } from '../agents.js';
import { normalizeNotificationKind, summarizeText } from '../helpers.js';
import { normalizeWorkspacePath } from '../git.js';
import { ParsedArgs } from './args.js';
import { EmitOptions, die, emit } from '../command-output.js';
import { resolveAgentId } from './args.js';

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
    action: action as import('../types/notifications-agents.js').AgentSignalAction,
    agentId: resolveAgentId(args),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    kind: publishKind,
    subject: args['subject'] ? String(args['subject']) : undefined,
    body: args['body'] ? String(args['body']) : null,
    data: args['data'] === undefined ? undefined : String(args['data']),
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
    limit: requestedLimit,
    cursor: args['cursor'] ? String(args['cursor']) : undefined,
  });
  const continuation = result.action === 'list' ? result.next?.list.request : undefined;
  const continuationArgs = ['--db', dbPath];
  if (continuation) {
    const flags: Record<string, string> = {
      agent_id: 'agent-id', workspace_path: 'workspace', artifact: 'artifact', repo: 'repo', ref: 'ref',
      kinds: 'kind', signal_id: 'signal-id', thread_id: 'thread-id', limit: 'limit', cursor: 'cursor',
    };
    for (const [key, flag] of Object.entries(flags)) {
      const value = continuation[key];
      for (const item of Array.isArray(value) ? value : value === undefined ? [] : [value]) {
        continuationArgs.push(`--${flag}`, String(item));
      }
    }
    if (continuation['unread_only'] === false) continuationArgs.push('--all');
    if (continuation['mark_read']) continuationArgs.push('--mark-read');
    if (args['include_bodies']) continuationArgs.push('--include-bodies');
    if (opts.compact) continuationArgs.push('--compact');
  }
  const pagination = result.action === 'list' ? {
    partial: result.partial ?? false,
    partialReasons: result.partialReasons ?? [],
    ...(continuation ? { next: { list: { command: { name: 'signal list', args: continuationArgs } } } } : {}),
  } : {};
  const cliActions = result.action === 'list' && result.actions ? {
    ack: { ...result.actions.ack, command: { name: 'signal ack', args: ['--db', dbPath, '--agent-id', String(result.actions.ack.request.agent_id), ...((result.actions.ack.request.signal_id as string[]).flatMap(id => ['--signal-id', id])), '--compact'] } },
    ...(result.actions.reply ? { reply: result.actions.reply.map(action => ({ ...action, command: { name: 'signal reply', args: ['--db', dbPath, '--agent-id', String(action.request.agent_id), '--in-reply-to', String(action.request.in_reply_to), '--subject', String(action.request.subject), '--compact'] } })) } : {}),
  } : undefined;
  if (compactList && result.action === 'list') {
    const signals = result.signals.map((signal) => {
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
        ...(signal.data ? { has_data: true } : {}),
      };
    });
    return emit({
      db_path: dbPath,
      action: 'list',
      count: signals.length,
      signals,
      unread_only: result.unread_only,
      bodies: 'omitted',
      ...(cliActions ? { actions: cliActions } : {}),
      ...pagination,
    }, 0, opts);
  }
  if (result.action === 'list' && !Boolean(args['include_bodies'])) {
    return emit({
      db_path: dbPath,
      ...result,
      ...(cliActions ? { actions: cliActions } : {}),
      ...pagination,
      bodies: 'summarized',
      signals: result.signals.map(({ data, ...signal }) => ({
        ...signal,
        body: signal.body == null ? null : summarizeText(signal.body, 160),
        ...(data ? { has_data: true } : {}),
      })),
    }, 0, opts);
  }
  return emit({ db_path: dbPath, ...result, ...pagination }, 0, opts);
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
    const label = (flag: string, maxLength: number): string | undefined => {
      const raw = args[flag];
      if (raw === undefined || raw === '') return undefined;
      if (typeof raw !== 'string' || !raw.trim() || raw.trim().length > maxLength) die(`--${flag.replaceAll('_', '-')} must be a non-empty string of at most ${maxLength} characters`);
      return raw.trim();
    };
    const agent = registerAgent(db, {
      agentId: resolveAgentId(args),
      agentName: label('agent_name', 256),
      agentVendor: label('agent_vendor', 128),
      agentHost: label('agent_host', 128),
      workspacePath: workspacePath ?? process.cwd(),
      artifact,
      context: args['context'] ? String(args['context']) : null,
    });
    return emit({ db_path: dbPath, action: 'register', agent }, 0, opts);
  }

  const defaultLimit = opts.compact ? 5 : 50;
  const limit = Number(args['limit'] ?? defaultLimit);
  const offset = Number(args['offset'] ?? 0);
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 200) die('--limit must be an integer between 1 and 200');
  if (!Number.isSafeInteger(offset) || offset < 0) die('--offset must be a non-negative integer');
  const result = listAgents(db, { workspacePath, artifact });
  const rows = result.agents.slice(offset, offset + limit);
  const omittedCount = Math.max(0, result.count - offset - rows.length);
  const continuationArgs = ['--db', dbPath, '--limit', String(limit), '--offset', String(offset + rows.length)];
  if (workspacePath) continuationArgs.push('--workspace', workspacePath);
  if (artifact) continuationArgs.push('--artifact', artifact);
  if (opts.compact) continuationArgs.push('--compact');
  const agents = opts.compact
    ? rows.map((agent) => ({
        agent_id: agent.agent_id,
        agent_name: agent.agent_name,
        agent_vendor: agent.agent_vendor,
        agent_host: agent.agent_host,
        workspace_path: agent.workspace_path,
        last_seen_at: agent.last_seen_at,
        context_summary: agent.context == null ? null : summarizeText(agent.context, 48),
      }))
    : rows;
  return emit({
    db_path: dbPath,
    action: 'list',
    count: agents.length,
    total_count: result.count,
    omitted_count: omittedCount,
    offset,
    partial: omittedCount > 0,
    partialReasons: omittedCount > 0 ? ['limit'] : [],
    ...(omittedCount > 0 ? { next: { list: { command: { name: 'agent list', args: continuationArgs } } } } : {}),
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
    (db.prepare(`SELECT state, COUNT(*) AS count FROM awareness_memories ${memWhere} GROUP BY state`).all(...memScopeBinds) as Array<{ state: string; count: number }>)
      .map(r => [r.state, r.count])
  );
  const memCount = Object.values(memStates).reduce((sum, count) => sum + count, 0);
  const memLabels = Object.fromEntries(
    (db.prepare(`SELECT COALESCE(label,'OTHER') AS label, COUNT(*) AS count FROM awareness_memories ${memWhere} GROUP BY label`).all(...memScopeBinds) as Array<{ label: string; count: number }>)
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
    locks,
    workspace_path: wsPath,
    artifact,
  }, 0, opts);
}

export function cmdInit(db: DatabaseSync, dbPath: string, opts: EmitOptions): number {
  const memCount = (db.prepare('SELECT COUNT(*) AS count FROM awareness_memories').get() as { count: number }).count;
  return emit({ db_path: dbPath, initialized: true, memory_count: memCount }, 0, opts);
}

export async function cmdSelfTest(opts: EmitOptions): Promise<number> {
  const testDb = new DatabaseSyncCtor(':memory:');
  testDb.exec('PRAGMA foreign_keys = ON');
  initDb(testDb);

  const testAgent = 'self-test-agent';

  // Write
  const { memoryId } = (await insertMemory(testDb, {
    agentId: testAgent,
    taskContext: 'self-test task',
    observation: 'This is a smoke-test memory.',
    importance: 7,
    label: 'GOTCHA',
    tags: ['smoke-test'],
  }));

  // Get
  const { memories: results } = (await getMemory(testDb, { query: 'smoke-test', limit: 5 }));
  if (results.length === 0) {
    return emit({ ok: false, error: 'FTS recall returned no results' }, 1, opts);
  }

  return emit({
    ok: true,
    db: ':memory:',
    fts_enabled: hasFts(testDb),
    memory_written: memoryId,
    memory_recalled: results[0]!.memory_id,
    checks: {
      write: Boolean(memoryId),
      fts_recall: results.length > 0,
      scoring: typeof results[0]!.score === 'number',
    },
  }, 0, opts);
}
