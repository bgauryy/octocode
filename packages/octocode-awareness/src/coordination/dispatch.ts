/**
 * Single command dispatcher shared by the CLI and in-process hosts.
 * Callers own DB/I/O lifecycles; optional values remain `undefined` because
 * library methods can distinguish omission from explicit null.
 */

import { type AgentStatus } from '@octocodeai/octocode-shared/entities';
import type { AwarenessStore } from './coordination-continuity.js';
import type { MemoryEvaluationCorpusV1,MemoryRecallModeV1 } from '../memory-hardening.js';

export interface AwarenessCommandRequest {
  command: string;
  action?: string;
  params?: Record<string, unknown>;
}

export interface AwarenessCommandOutcome {
  /** The JSON value the CLI would print. */
  result: unknown;
  /** Process/branch exit code: 0 success, 2 for a still-held `lock wait`. */
  exitCode: number;
}

type Params = Record<string, unknown>;

function str(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  const s = String(value).trim();
  return s.length ? s : undefined;
}

function reqStr(p: Params, key: string, ctx: string): string {
  const s = str(p[key]);
  if (!s) throw new Error(`${ctx} requires ${key}`);
  return s;
}

function num(value: unknown): number | undefined {
  if (value === undefined || value === null || value === '') return undefined;
  const n = Number(value);
  return Number.isFinite(n) ? n : undefined;
}

function bool(value: unknown): boolean {
  return value === true || value === 'true' || value === 1 || value === '1';
}

function corpus(value: unknown): MemoryEvaluationCorpusV1 | undefined {
  if (value === undefined || value === null || value === '') return undefined;
  const parsed = typeof value === 'string' ? JSON.parse(value) as unknown : value;
  if (!parsed || typeof parsed !== 'object') throw new Error('memory evaluate corpus-json must be a JSON object');
  return parsed as MemoryEvaluationCorpusV1;
}

/** Accept a string[], a comma string, or a single string; the library splits either. */
function list(value: unknown): string | string[] | undefined {
  if (Array.isArray(value)) {
    const arr = value.map((v) => String(v).trim()).filter(Boolean);
    return arr.length ? arr : undefined;
  }
  return str(value);
}


/**
 * Execute one structured Awareness command against an already-open library
 * instance. Throws `Error` on unknown command/action or a missing required
 * param (the CLI maps that to exit 1; hosts catch it as a tool error).
 */
export function dispatchAwarenessCommand(
  aw: AwarenessStore,
  req: AwarenessCommandRequest,
): AwarenessCommandOutcome {
  const p: Params = req.params ?? {};
  const action = req.action;
  const done = (result: unknown, exitCode = 0): AwarenessCommandOutcome => ({ result, exitCode });

  switch (req.command) {
    case 'status':
      return done(aw.status({ staleAfterMs: num(p['staleAfterMs']) }));

    case 'handoff': {
      switch (action) {
        case 'add':
          return done(aw.addHandoff({
            agentId: reqStr(p, 'agentId', 'handoff add'),
            summary: reqStr(p, 'summary', 'handoff add'),
            files: list(p['files']),
          }));
        case 'list':
          return done(aw.listHandoffs({ includeCleared: bool(p['includeCleared']) }));
        case 'clear':
          return done(aw.clearHandoff({ handoffId: reqStr(p, 'handoffId', 'handoff clear') }));
        default:
          throw new Error('handoff action must be add, list, or clear');
      }
    }

    case 'agent': {
      switch (action) {
        case 'touch':
          return done(aw.touchAgent({
            agentId: reqStr(p, 'agentId', 'agent touch'),
            status: (str(p['status']) as AgentStatus | undefined) ?? 'ACTIVE',
          }));
        case 'leave':
          return done(aw.leaveAgent({ agentId: reqStr(p, 'agentId', 'agent leave') }));
        default:
          throw new Error('agent action must be touch or leave');
      }
    }

    case 'memory': {
      switch (action) {
        case 'store-verified':
          return done(aw.storeVerifiedMemory({
            label: reqStr(p, 'label', 'memory store-verified'), text: reqStr(p, 'text', 'memory store-verified'),
            sourceDigest: reqStr(p, 'sourceDigest', 'memory store-verified'),
            scope: str(p['scope']) as 'project' | 'artifact' | undefined,
            verifiedAt: str(p['verifiedAt']), validUntil: str(p['validUntil']),
            importance: num(p['importance']), tags: list(p['tags']),
          }));
        case 'recall-verified':
          return done(aw.recallVerifiedMemory({
            query: str(p['query']), label: str(p['label']), sourceDigest: str(p['sourceDigest']),
            scope: str(p['scope']) as 'project' | 'artifact' | undefined,
            mode: str(p['mode']) as MemoryRecallModeV1 | undefined,
            limit: num(p['limit']), now: str(p['now']), minSimilarity: num(p['minSimilarity']),
          }));
        case 'evaluate':
          return done(aw.evaluateVerifiedMemory({
            corpus: corpus(p['corpusJson']), now: str(p['now']), limit: num(p['limit']), minSimilarity: num(p['minSimilarity']),
          }));
        case 'reindex':
          return done(aw.reindexMemories({ force: bool(p['force']), limit: num(p['limit']) }));
        case 'prune':
          return done(aw.pruneMemories({
            olderThanMs: num(p['olderThanMs']) ?? 0,
            label: str(p['label']),
            dryRun: p['dryRun'] === undefined ? true : bool(p['dryRun']),
          }));
        default:
          throw new Error('memory action must be store-verified, recall-verified, evaluate, reindex, or prune');
      }
    }

    default:
      throw new Error(`unknown command: ${req.command}`);
  }
}
