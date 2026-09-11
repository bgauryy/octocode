import path from 'node:path';
import {
  createAwarenessClient,
  type AwarenessExecutableCall,
  type AwarenessOperationResult,
} from '@octocodeai/octocode-awareness';

export interface CheckpointInfo { id: string; label: string; ts: number; filesChanged: number; kind?: 'edit'|'checkpoint'|'restore'; side?: 'before'|'after' }
export interface DiffStatEntry { status: string; path: string }
export interface CheckpointPage { checkpoints: CheckpointInfo[]; nextCall?: AwarenessExecutableCall }
export interface RestoreResult { verificationRunId: string }
export interface CheckpointEngine {
  listCheckpoints(limit?: number, nextCall?: AwarenessExecutableCall): Promise<CheckpointPage>;
  restoreFiles(id: string, paths?: string[]): Promise<RestoreResult>;
  diffStat(id: string): Promise<DiffStatEntry[]>;
}
export type HistoryOperationRunner = (request: AwarenessExecutableCall) => Promise<AwarenessOperationResult>;
export interface CheckpointStoreOptions { agentId?: string; run?: HistoryOperationRunner }
interface Payload { ok?: boolean; [key: string]: unknown }

async function invoke(run: HistoryOperationRunner, request: AwarenessExecutableCall): Promise<Payload> {
  const result = await run(request);
  const payload = result.payload as Payload | null;
  if (result.exitCode !== 0 || payload?.ok !== true) throw new Error(JSON.stringify(result.payload));
  return payload;
}

/** Canonical Awareness-backed rewind adapter. */
export async function initCheckpointStore(cwd: string, opts: CheckpointStoreOptions = {}): Promise<CheckpointEngine> {
  const workspace = path.resolve(cwd); const agentId = opts.agentId ?? process.env['OCTOCODE_AGENT_ID'];
  if (!agentId) throw new Error('OCTOCODE_AGENT_ID is required for local history');
  const run = opts.run ?? createAwarenessClient({ workspace, agentId }).execute;
  const previews = new Map<string, string>();
  const observed = new Map<string, CheckpointInfo>();
  const list = async (limit = 30, nextCall?: AwarenessExecutableCall): Promise<CheckpointPage> => {
    const payload = await invoke(run, nextCall ?? { operation: 'history.timeline', params: { limit } });
    const checkpoints = (Array.isArray(payload.operations) ? payload.operations : []).map((raw) => {
      const op = raw as Record<string, unknown>; const kind = op.kind as CheckpointInfo['kind'];
      const side: 'before' | 'after' = kind === 'checkpoint' ? 'after' : 'before';
      return { id: String(op.operation_id), label: String(op.label ?? ''), ts: Date.parse(String(op.created_at)), filesChanged: Number(op.file_count ?? 0), kind, side };
    });
    checkpoints.forEach(item => observed.set(item.id, item));
    const next = payload.next && typeof payload.next === 'object' ? payload.next as Record<string, unknown> : undefined;
    const continuation = next?.call && typeof next.call === 'object'
      ? next.call as AwarenessExecutableCall
      : next?.operation && typeof next.operation === 'string'
        ? next as unknown as AwarenessExecutableCall
        : undefined;
    return { checkpoints, ...(continuation ? { nextCall: continuation } : {}) };
  };
  return {
    listCheckpoints: list,
    async restoreFiles(id, paths) {
      const key = `${id}\0${[...(paths ?? [])].sort().join('\0')}`;
      const previewId = previews.get(key);
      if (!previewId) throw new Error('Preview this restore before applying it.');
      const applied = await invoke(run, { operation: 'history.restore', params: { action: 'apply', preview_id: previewId } });
      if (typeof applied.verification_run_id !== 'string' || !applied.verification_run_id) {
        throw new Error('Awareness restore returned no verification run id');
      }
      previews.delete(key);
      return { verificationRunId: applied.verification_run_id };
    },
    async diffStat(id) {
      const operation = observed.get(id);
      if (!operation) throw new Error(`History operation not found in the bounded timeline: ${id}`);
      const preview = await invoke(run, { operation: 'history.restore', params: { action: 'preview', operation_id: id, side: operation.side ?? 'before' } });
      if (typeof preview.preview_id !== 'string') throw new Error('Awareness restore preview returned no preview id');
      previews.set(`${id}\0`, preview.preview_id);
      const changes = Array.isArray(preview.changes) ? preview.changes as Array<Record<string, unknown>> : [];
      const status = { create: 'A', update: 'M', delete: 'D', unchanged: '=' } as const;
      return changes.map(change => ({ status: status[change.action as keyof typeof status] ?? '?', path: String(change.path ?? '') })).filter(change => change.path);
    },
  };
}
