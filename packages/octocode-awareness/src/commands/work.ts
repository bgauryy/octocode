import type { DatabaseSync } from 'node:sqlite';
import { preFlightIntent } from '../intents-preflight.js';
import { releaseFileLock } from '../intents-release.js';
import { endWork, listWork, showWork, startWork, touchWork } from '../work.js';
import type { WorkMutationResult } from '../types/work-maintenance.js';
import { auditUnverified } from '../verify-audit.js';
import { markVerified } from '../verify-mark.js';
import { summarizeText } from '../helpers.js';
import { normalizeWorkspacePath } from '../git.js';
import { MAX_CLI_TTL_SECONDS, ParsedArgs } from './args.js';
import { EmitOptions, die, emit } from '../command-output.js';
import { firstValue, listLimit, parseBoundedSeconds, resolveAgentId, valuesFor } from './args.js';
import { requiredArg } from './plans.js';

export function cmdPreFlightIntent(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTarget = args['target_file'];
  const targetFiles = Array.isArray(rawTarget) ? rawTarget : rawTarget ? [String(rawTarget)] : [];
  // Reject empty target — otherwise an ACTIVE run is created that locks
  // nothing (phantom lock) and pollutes the workboard. --target-file is required.
  if (targetFiles.length === 0) die('lock acquire requires at least one --target-file');
  const ttlMinutes = args['ttl_minutes'] ? parseInt(String(args['ttl_minutes']), 10) : null;
  const ttlSeconds = args['ttl_seconds'] ? parseInt(String(args['ttl_seconds']), 10) : null;
  if (ttlMinutes != null && (!Number.isInteger(ttlMinutes) || ttlMinutes < 1)) die('--ttl-minutes must be >= 1');
  if (ttlSeconds != null && (!Number.isInteger(ttlSeconds) || ttlSeconds < 1)) die('--ttl-seconds must be >= 1');
  if (ttlMinutes != null && ttlMinutes > 10) die('--ttl-minutes must be <= 10');
  if (ttlSeconds != null && ttlSeconds > MAX_CLI_TTL_SECONDS) die('--ttl-seconds must be <= 600');
  const ttlMs = ttlSeconds != null ? ttlSeconds * 1000 : ttlMinutes != null ? ttlMinutes * 60000 : null;

  const claimParams = {
    agentId: resolveAgentId(args),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    runId: firstValue(args, 'run_id') ?? null,
    rationale: String(args['rationale'] ?? 'agent write operation'),
    testPlan: String(args['test_plan'] ?? 'post-edit verification'),
    contextRef: args['context_ref'] ? String(args['context_ref']) : null,
    targetFiles,
    ttlMs,
  };
  const result = preFlightIntent(db, claimParams);

  if (!result.ok) return emit({ db_path: dbPath, ...result }, 2, opts);
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function cmdAuditUnverified(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  // Normalize the workspace filter to the git-root key (same as write paths)
  // so work.verify audit from a package/subdir does not miss pending
  // work and report a false "0 unverified".
  const rawAuditWs = args['workspace'] ? String(args['workspace']) : null;
  const result = auditUnverified(db, {
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    workspacePath: rawAuditWs ? normalizeWorkspacePath(rawAuditWs, rawAuditWs) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    olderThanDays: args['older_than_days'] !== undefined ? Number(args['older_than_days']) : null,
    origins: valuesFor(args, 'origin').map((origin) => origin.toUpperCase() as 'TASK' | 'WORK' | 'HOOK'),
    before: args['before'] ? String(args['before']) : null,
  });
  const compact = opts.compact;
  const paginate = compact || args['limit'] !== undefined || args['offset'] !== undefined;
  if (!paginate) return emit({ db_path: dbPath, ...result }, result.count > 0 ? 1 : 0, opts);

  const limit = listLimit(args, compact ? 20 : 200);
  const offset = args['offset'] === undefined ? 0 : Number(args['offset']);
  if (!Number.isInteger(offset) || offset < 0) die('--offset must be a non-negative integer');
  const rows = [
    ...result.unverified.map((value) => ({ kind: 'unverified' as const, value })),
    ...result.stale_active.map((value) => ({ kind: 'stale_active' as const, value })),
  ];
  const page = rows.slice(offset, offset + limit);
  const unverified = page.flatMap((row) => row.kind === 'unverified' ? [row.value] : []);
  const staleActive = page.flatMap((row) => row.kind === 'stale_active' ? [row.value] : []);
  const nextOffset = offset + page.length;
  const hasMore = nextOffset < rows.length;
  const nextParams: Record<string, unknown> = { limit, offset: nextOffset };
  for (const field of ['agent_id', 'workspace', 'artifact', 'older_than_days', 'before'] as const) {
    if (args[field] !== undefined) nextParams[field] = args[field];
  }
  const origins = valuesFor(args, 'origin');
  if (origins.length) nextParams['origin'] = origins;

  return emit({
    db_path: dbPath,
    ...result,
    unverified,
    stale_active: staleActive,
    unverified_count: result.unverified.length,
    stale_active_count: result.stale_active.length,
    returned_count: page.length,
    omitted_count: result.count - page.length,
    pagination: { offset, limit, total: rows.length, has_more: hasMore, ...(hasMore ? { next_offset: nextOffset } : {}) },
    ...(hasMore ? { next: { operation: 'work.verify', params: {
      action: 'audit',
      ...Object.fromEntries(Object.entries(nextParams).filter(([key]) => key !== 'agent_id' && key !== 'workspace')),
    } } } : {}),
  }, result.count > 0 ? 1 : 0, opts);
}

export function cmdVerify(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const allPending = Boolean(args['all_pending']);
  const runIds = valuesFor(args, 'run_id');
  if (!allPending && runIds.length === 0) {
    return emit({ error: '--run-id is required (or use --all-pending)' }, 1, opts);
  }
  const statusArg = args['status'] ? String(args['status']) : 'SUCCESS';
  if (statusArg !== 'SUCCESS' && statusArg !== 'FAILED') {
    return emit({ error: `--status must be SUCCESS or FAILED, got "${statusArg}"` }, 1, opts);
  }
  const message = args['message'] ? String(args['message']).trim() : '';
  if (statusArg === 'SUCCESS' && !message) {
    return emit({ error: 'SUCCESS verification requires --message with the evidence receipt' }, 1, opts);
  }
  if (allPending && !args['workspace'] && !args['artifact']) {
    return emit({ error: '--all-pending requires --workspace or --artifact; otherwise pass explicit --run-id values' }, 1, opts);
  }
  const adoptVerification = Boolean(args['adopt_verification']);
  if (adoptVerification && (allPending || runIds.length !== 1 || !args['workspace'])) {
    return emit({ error: '--adopt-verification requires exactly one --run-id and --workspace' }, 1, opts);
  }
  if (!allPending && runIds.length > 1) {
    const results = runIds.map((runId) => markVerified(db, {
      runId,
      agentId: resolveAgentId(args),
      workspacePath: args['workspace'] ? String(args['workspace']) : null,
      artifact: args['artifact'] ? String(args['artifact']) : null,
      message: message || undefined,
      status: statusArg as 'SUCCESS' | 'FAILED',
    }));
    const failed = results.find((result) => !result.ok);
    if (failed && !failed.ok) {
      return emit({ db_path: dbPath, ok: false, error: failed.error, run_id: null, run_ids: runIds, results }, 1, opts);
    }
    return emit({
      db_path: dbPath,
      run_id: null,
      run_ids: runIds,
      count: results.length,
      status: statusArg,
      results,
    }, 0, opts);
  }
  const result = markVerified(db, {
    runId: runIds[0],
    agentId: resolveAgentId(args),
    allPending,
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    message: message || undefined,
    status: statusArg as 'SUCCESS' | 'FAILED',
    adoptVerification,
  });
  return emit({ db_path: dbPath, ...result }, result.ok ? 0 : 1, opts);
}

export function cmdReleaseFileLock(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTarget = args['target_file'];
  const targetFiles = rawTarget
    ? (Array.isArray(rawTarget) ? rawTarget : [String(rawTarget)])
    : [];

  const runId = firstValue(args, 'run_id');

  if (!runId && targetFiles.length === 0) {
    return emit({ error: 'lock release requires --run-id or --target-file' }, 1, opts);
  }

  const status = String(args['status'] ?? 'PENDING').toUpperCase();
  if (!['PENDING', 'FAILED'].includes(status)) {
    return emit({ error: `--status must be PENDING or FAILED; use verify mark --message <receipt> for SUCCESS, got "${status}"` }, 1, opts);
  }

  const result = releaseFileLock(db, {
    agentId: resolveAgentId(args),
    workspacePath: args['workspace'] ? String(args['workspace']) : null,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    runId: runId ?? null,
    targetFiles,
    status: status as 'PENDING' | 'FAILED',
  });

  if (!result.released) {
    return emit({ db_path: dbPath, ...result, ok: false }, result.ambiguousRelease ? 2 : 1, opts);
  }
  return emit({ db_path: dbPath, ...result }, 0, opts);
}

export function projectCompactWorkMutation(result: WorkMutationResult): Record<string, unknown> {
  const peers = result.peers.slice(0, 1).map(peer => ({
    run_id: peer.run_id,
    agent_id: peer.agent_id,
    file_path: peer.file_path,
    exclusive: peer.exclusive,
    expires_at: peer.expires_at,
  }));
  return {
    run_id: result.run.run_id,
    agent_id: result.run.agent_id,
    status: result.run.status,
    file_count: result.files.length,
    peer_count: result.peer_count,
    ...(peers.length > 0 ? { peers, peer_omitted_count: Math.max(0, result.peer_count - peers.length) } : {}),
  };
}

export function cmdWork(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const action = requiredArg(args, 'action');
  const rawFiles = args['file'];
  const targetFiles = rawFiles
    ? (Array.isArray(rawFiles) ? rawFiles.map(String) : [String(rawFiles)])
    : [];
  const runId = firstValue(args, 'run_id');
  const workspacePath = args['workspace'] ? String(args['workspace']) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;

  const ttlSeconds = parseBoundedSeconds(args, 'ttl_seconds', 1, 60 * 60);
  let ttlMs = ttlSeconds == null ? undefined : ttlSeconds * 1000;
  if (ttlMs == null && args['ttl_minutes'] != null) {
    const minutes = Number(String(args['ttl_minutes']));
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > 60) {
      die('--ttl-minutes must be an integer between 1 and 60');
    }
    ttlMs = minutes * 60_000;
  }

  if (action === 'start') {
    const result = startWork(db, {
      agentId: resolveAgentId(args),
      sessionId: args['session_id'] ? String(args['session_id']) : null,
      workspacePath,
      artifact,
      runId,
      rationale: args['rationale'] ? String(args['rationale']) : undefined,
      testPlan: args['test_plan'] ? String(args['test_plan']) : undefined,
      contextRef: args['context_ref'] ? String(args['context_ref']) : null,
      targetFiles,
      exclusive: Boolean(args['exclusive']),
      ttlMs,
    });
    const payload = opts.compact && result.ok ? projectCompactWorkMutation(result) : result;
    return emit({ db_path: dbPath, ...payload }, result.ok ? 0 : 2, opts);
  }

  if (action === 'touch') {
    if (!runId) die('--run-id is required');
    const result = touchWork(db, {
      agentId: resolveAgentId(args),
      runId,
      targetFiles: targetFiles.length > 0 ? targetFiles : undefined,
      ttlMs,
    });
    const payload = opts.compact ? projectCompactWorkMutation(result) : result;
    return emit({ db_path: dbPath, ...payload }, 0, opts);
  }

  if (action === 'end') {
    if (!runId) die('--run-id is required');
    const result = endWork(db, {
      agentId: resolveAgentId(args),
      runId,
      targetFiles: targetFiles.length > 0 ? targetFiles : undefined,
    });
    const payload = opts.compact ? projectCompactWorkMutation(result) : result;
    return emit({ db_path: dbPath, ...payload }, 0, opts);
  }

  if (action === 'list' || action === 'show') {
    if (action === 'show' && targetFiles.length !== 1) die('work show requires exactly one --file');
    const params = {
      workspacePath,
      artifact,
      agentId: args['agent_id'] ? String(args['agent_id']) : null,
      runId,
      activeOnly: !Boolean(args['all']),
      limit: listLimit(args, opts.compact ? 5 : 20),
      offset: args['offset'] === undefined ? 0 : Number(args['offset']),
    };
    const result = action === 'show'
      ? showWork(db, { ...params, filePath: targetFiles[0] ?? '' })
      : listWork(db, params);
    const continuation = result.next?.list.params;
    const continuationParams = continuation ? {
      kind: 'presence',
      ...(continuation.artifact ? { artifact: continuation.artifact } : {}),
      ...(continuation.runId ? { run_id: continuation.runId } : {}),
      ...(continuation.filePath ? { file: [continuation.filePath] } : {}),
      ...(continuation.limit ? { limit: continuation.limit } : {}),
      ...(continuation.offset !== undefined ? { offset: continuation.offset } : {}),
      ...(continuation.activeOnly === false ? { all: true } : {}),
      ...(args['full'] ? { full: true } : {}),
    } : undefined;
    const pagination = {
      partial: result.partial, partialReasons: result.partialReasons,
      next: continuationParams ? { list: {
        operation: action === 'show' ? 'work.show' : 'work.list',
        params: continuationParams,
      } } : undefined,
    };
    if (Boolean(args['full'])) return emit({ db_path: dbPath, ...result, ...pagination }, 0, opts);
    const files = result.files.map((file) => ({
      run_id: file.run_id,
      task_id: file.task_id,
      origin: file.origin,
      agent_id: file.agent_id,
      file_path: file.file_path,
      rationale: summarizeText(file.rationale, 160),
      heartbeat_at: file.heartbeat_at,
      expires_at: file.expires_at,
      exclusive: file.exclusive,
    }));
    return emit({
      db_path: dbPath,
      count: files.length,
      total_count: result.total_count,
      omitted_count: result.omitted_count,
      files,
      ...pagination,
    }, 0, opts);
  }

  return emit({ db_path: dbPath, error: `unknown work action: ${action}` }, 1, opts);
}
