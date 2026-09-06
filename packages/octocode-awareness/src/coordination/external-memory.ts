import type { MemoryItem } from '@octocodeai/octocode-shared/entities';
import { openAwarenessStore } from './open.js';

export const EXTERNAL_MEMORY_ACTIONS = ['recall', 'record', 'forget', 'review', 'suggest'] as const;
export const EXTERNAL_MEMORY_RECALL_MODES = ['lexical', 'semantic', 'recent', 'tagged'] as const;
export type ExternalMemoryAction = typeof EXTERNAL_MEMORY_ACTIONS[number];
export type ExternalMemoryRecallMode = typeof EXTERNAL_MEMORY_RECALL_MODES[number];

export interface ExternalMemoryParams {
  action: ExternalMemoryAction;
  query?: string;
  mode?: ExternalMemoryRecallMode;
  label?: string;
  observation?: string;
  importance?: number;
  taskContext?: string;
  source?: string;
  tags?: string[];
  changedFiles?: string[];
  limit?: number;
  memoryId?: string;
}

export interface ExternalMemoryReviewCandidate {
  memoryId: string;
  label: string;
  issues: string[];
  preview: string;
}

export interface ExternalMemoryResult {
  action: ExternalMemoryAction;
  summary: string;
  result?: unknown;
  count?: number;
  candidates?: ExternalMemoryReviewCandidate[];
  candidate?: Omit<ExternalMemoryParams, 'changedFiles'>;
  memoryId?: string;
  deleted?: number;
}

function validateObservation(observation: string): string | null {
  if (observation.length < 8) return 'record observation is too short to be reusable; include the durable learning and evidence.';
  if (/\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16})\b/.test(observation)) return 'record observation looks like it contains a secret/token; do not store secrets in memory.';
  if (observation.split(/\r?\n/).length > 8 || observation.length > 1200) return 'record observation looks like a raw log/dump; store a short reusable learning with evidence instead.';
  if (/\b(?:tests?|build|typecheck|lint)\s+(?:passed|green|ok)\b/i.test(observation)) return 'record observation looks like routine status; store only reusable learnings, gotchas, decisions, or command quirks.';
  if (/\b(?:AGENTS\.md|CLAUDE\.md)\b/i.test(observation)) return 'record observation references agent instruction files; fetch those from the repo instead of storing them in memory.';
  return null;
}

function limit(value: unknown, fallback = 20): number {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? Math.min(parsed, 50) : fallback;
}

function changedFileTags(files: string[] | undefined): string[] {
  const tags = new Set<string>();
  for (const file of files ?? []) {
    const parts = file.split('/').filter(Boolean);
    if (parts[0] === 'packages' && parts[1]) tags.add(parts[1]);
    const filename = parts.at(-1);
    if (filename) tags.add(filename.replace(/\.[^.]+$/, ''));
  }
  return [...tags];
}

function review(items: MemoryItem[]): ExternalMemoryReviewCandidate[] {
  const now = Date.now();
  return items.map((item) => {
    const issues: string[] = [];
    if (!/\bSource:/i.test(item.text)) issues.push('missing-source');
    if (item.text.length > 1200 || item.text.split(/\r?\n/).length > 8) issues.push('too-long');
    if (/\b(?:tests?|build|typecheck|lint)\s+(?:passed|green|ok)\b/i.test(item.text)) issues.push('routine-status');
    if (/\b(?:AGENTS\.md|CLAUDE\.md)\b/i.test(item.text)) issues.push('instruction-file-reference');
    if (item.tags.length === 0) issues.push('missing-tags');
    if (now - Date.parse(item.createdAt) > 90 * 24 * 60 * 60 * 1000) issues.push('older-than-90d');
    return { memoryId: item.memoryId, label: item.label, issues, preview: item.text.slice(0, 160) };
  }).filter((item) => item.issues.length > 0);
}

export function validateExternalMemoryParams(params: ExternalMemoryParams): void {
  if (!EXTERNAL_MEMORY_ACTIONS.includes(params.action)) throw new Error(`unknown action "${String(params.action)}".`);
  if (params.action === 'record' || params.action === 'suggest') {
    const observation = params.observation?.trim() ?? '';
    if (!observation) throw new Error(`${params.action} requires an observation.`);
    const error = validateObservation(observation);
    if (error) throw new Error(error);
  }
  if (params.action === 'record') {
    if (!params.label?.trim()) throw new Error('record requires a label (e.g. GOTCHA).');
    if (!Number.isInteger(params.importance) || params.importance! < 1 || params.importance! > 10) throw new Error('record requires importance 1-10.');
  }
  if (params.action === 'recall' && (params.mode ?? 'lexical') !== 'recent') {
    const query = params.query ?? ((params.mode ?? 'lexical') === 'tagged' ? params.tags?.[0] : '');
    if (!query?.trim()) throw new Error(`recall mode ${params.mode ?? 'lexical'} requires a query${params.mode === 'tagged' ? ' or at least one tag' : ''}.`);
  }
  if (params.action === 'forget' && !params.memoryId?.trim()) throw new Error('forget requires a memoryId.');
}

/** Apply an external agent's durable-memory workflow directly to the shared Awareness store. */
export function executeExternalMemoryAction(input: { workspace: string; params: ExternalMemoryParams }): ExternalMemoryResult {
  const params = input.params;
  validateExternalMemoryParams(params);
  if (params.action === 'suggest') {
    const candidate: Omit<ExternalMemoryParams, 'changedFiles'> = {
      action: 'record',
      label: params.label?.trim() || 'EXPERIENCE',
      observation: params.observation!.trim(),
      importance: Number.isInteger(params.importance) ? params.importance : 5,
      ...(params.taskContext?.trim() ? { taskContext: params.taskContext.trim() } : {}),
      ...(params.source?.trim() ? { source: params.source.trim() } : {}),
      tags: [...new Set([...(params.tags ?? []), ...changedFileTags(params.changedFiles)].filter(Boolean))],
    };
    return { action: 'suggest', summary: 'Suggested memory candidate (not recorded).', candidate };
  }

  const aw = openAwarenessStore({ workspace: input.workspace });
  try {
    if (params.action === 'record') {
      const text = `${params.taskContext?.trim() ? `${params.taskContext.trim()}: ` : ''}${params.observation!.trim()}${params.source?.trim() ? `\nSource: ${params.source.trim()}` : ''}`;
      const tags = [...new Set([
        `importance:${params.importance}`,
        ...(params.tags ?? []).map((tag) => tag.trim()).filter((tag) => tag && !/[\r\n,]/.test(tag)),
      ])];
      const memory = aw.storeMemory({ label: params.label!.trim(), text, tags });
      return { action: 'record', summary: `Recorded ${memory.label} memory ${memory.memoryId}.`, result: memory, count: 1, memoryId: memory.memoryId };
    }
    if (params.action === 'forget') {
      const result = aw.forgetMemory({ memoryId: params.memoryId!.trim() });
      const deleted = result.forgotten ? 1 : 0;
      return { action: 'forget', summary: `Forgot ${deleted} memor${deleted === 1 ? 'y' : 'ies'}.`, result, deleted };
    }
    const mode = params.mode ?? 'lexical';
    const query = params.query?.trim() || (mode === 'tagged' ? params.tags?.[0]?.trim() : undefined);
    const items = aw.recallMemory({
      query: params.action === 'review' ? params.query?.trim() : mode === 'recent' ? undefined : query,
      label: params.label?.trim(),
      limit: limit(params.limit),
      semantic: params.action === 'recall' && mode === 'semantic',
    });
    if (params.action === 'review') {
      const candidates = review(items);
      return { action: 'review', summary: `Reviewed ${items.length} memor${items.length === 1 ? 'y' : 'ies'}; found ${candidates.length} candidate${candidates.length === 1 ? '' : 's'} for cleanup or rewrite.`, result: items, count: items.length, candidates };
    }
    const scope = mode === 'recent' ? ' recent' : query ? ` for "${query}"` : '';
    return { action: 'recall', summary: `Recalled ${items.length}${scope} memor${items.length === 1 ? 'y' : 'ies'}.`, result: items, count: items.length };
  } finally {
    aw.close();
  }
}
