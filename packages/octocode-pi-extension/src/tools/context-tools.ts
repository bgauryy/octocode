/** Model-aware proactive compaction built on Pi's public extension API. */
import type { PiInstance, NotifyFn } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { createSessionArtifactContext } from './session-artifacts.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export const OCTOCODE_COMPACTION_THRESHOLD = 0.8;

function finiteNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function compactionMetrics(result: unknown): {
  tokensBefore: number;
  estimatedTokensAfter: number;
  reclaimedTokens: number;
  reclaimedPercent: number;
} | undefined {
  if (!result || typeof result !== 'object' || Array.isArray(result)) return undefined;
  const record = result as Record<string, unknown>;
  const tokensBefore = finiteNumber(record['tokensBefore']);
  const estimatedTokensAfter = finiteNumber(record['estimatedTokensAfter']);
  if (tokensBefore === undefined || estimatedTokensAfter === undefined) return undefined;
  const reclaimedTokens = Math.max(0, tokensBefore - estimatedTokensAfter);
  return {
    tokensBefore,
    estimatedTokensAfter,
    reclaimedTokens,
    reclaimedPercent: tokensBefore > 0 ? Math.round((reclaimedTokens / tokensBefore) * 100) : 0,
  };
}

function sessionKey(ctx: Parameters<NotifyFn>[0]): string {
  return ctx?.sessionManager?.getSessionId?.()
    ?? ctx?.sessionManager?.getSessionFile?.()
    ?? `${ctx?.cwd ?? process.cwd()}:active`;
}

export function registerContextTools(
  pi: PiInstance,
  _Type: TypeBoxBuilder,
  _registeredToolNames: Set<string>,
  _registerFn: RegisterFn,
  _notify: NotifyFn,
): void {
  // Pi's own post-run threshold/overflow path runs before agent_settled. Waiting
  // for this idle boundary avoids calling ctx.compact() from turn_end, where its
  // abort-and-wait implementation can race the run that is still settling.
  const inFlight = new Set<string>();
  pi.on('agent_settled', async (_event, ctx) => {
    const usage = ctx.getContextUsage?.();
    const tokens = usage?.tokens;
    const contextWindow = usage?.contextWindow;
    if (tokens === null || tokens === undefined || !contextWindow || contextWindow <= 0 || !ctx.compact) return;
    if (tokens / contextWindow < OCTOCODE_COMPACTION_THRESHOLD) return;
    const key = sessionKey(ctx);
    if (inFlight.has(key)) return;
    inFlight.add(key);
    _notify(ctx, `Context reached ${Math.round((tokens / contextWindow) * 100)}%; compacting at the 80% policy boundary.`, 'info');
    try {
      ctx.compact({
        customInstructions: 'Compact at the Octocode 80% context boundary. Preserve the active objective, current plan, decisions, changed files, verification state, and unresolved blockers.',
        onComplete: (result) => {
          inFlight.delete(key);
          const metrics = compactionMetrics(result);
          if (metrics) {
            try {
              const artifacts = createSessionArtifactContext(ctx);
              artifacts.writeJson('compaction/metrics-latest.json', {
                version: 1,
                recordedAt: new Date().toISOString(),
                ...metrics,
              });
              artifacts.registerProducer('compaction', 'compaction/metrics-latest.json');
            } catch { /* metrics persistence is best-effort */ }
            _notify(
              ctx,
              `Context compaction completed: ${metrics.tokensBefore} → ~${metrics.estimatedTokensAfter} tokens; reclaimed ~${metrics.reclaimedTokens} (${metrics.reclaimedPercent}%).`,
              'info',
            );
          } else {
            _notify(ctx, 'Context compaction completed.', 'info');
          }
        },
        onError: (error) => {
          inFlight.delete(key);
          _notify(ctx, `Context compaction failed: ${error.message}`, 'warning');
        },
      });
    } catch (error) {
      inFlight.delete(key);
      _notify(ctx, `Context compaction failed: ${error instanceof Error ? error.message : String(error)}`, 'warning');
    }
  });
  pi.on('session_compact', async (_event, ctx) => {
    inFlight.delete(sessionKey(ctx));
  });
  pi.on('session_compact_failed', async (_event, ctx) => {
    inFlight.delete(sessionKey(ctx));
  });
}
