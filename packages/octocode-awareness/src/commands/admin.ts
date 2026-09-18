import type { DatabaseSync } from 'node:sqlite';
import { agentSignal } from '../notifications-signals.js';
import { normalizeNotificationKind, summarizeText } from '../helpers.js';
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
  const continuationParams = continuation ? {
    ...(continuation.artifact ? { artifact: continuation.artifact } : {}),
    ...(continuation.repo ? { repo: continuation.repo } : {}),
    ...(continuation.ref ? { ref: continuation.ref } : {}),
    ...(continuation.kinds ? { kind: continuation.kinds } : {}),
    ...(continuation.signal_id ? { signal_id: continuation.signal_id } : {}),
    ...(continuation.thread_id ? { thread_id: continuation.thread_id } : {}),
    ...(continuation.limit ? { limit: continuation.limit } : {}),
    ...(continuation.cursor ? { cursor: continuation.cursor } : {}),
    ...(continuation.unread_only === false ? { all: true } : {}),
    ...(continuation.mark_read ? { mark_read: true } : {}),
    ...(args['include_bodies'] ? { include_bodies: true } : {}),
  } : undefined;
  const pagination = result.action === 'list' ? {
    partial: result.partial ?? false,
    partialReasons: result.partialReasons ?? [],
    ...(continuationParams ? { next: { list: { operation: 'message.list', params: continuationParams } } } : {}),
  } : {};
  const cliActions = result.action === 'list' && result.actions ? {
    ...(result.actions.reply ? { reply: result.actions.reply.map(action => ({
      ...action,
      operation: {
        operation: 'message.reply',
        params: { in_reply_to: action.request.in_reply_to, subject: action.request.subject },
      },
    })) } : {}),
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
