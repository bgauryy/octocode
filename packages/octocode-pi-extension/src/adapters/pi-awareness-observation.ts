import { createHash } from 'node:crypto';
import {
  createAwarenessClient,
  type AwarenessOperationResult,
  type ContextObservation,
} from '@octocodeai/octocode-awareness';
import type { PiRuntimeObservation } from '@octocodeai/agent-contracts/physiology';
import { isPersistentStorageEnabledForExtension } from '@octocodeai/config';
import type { PiContext } from '../types.js';
import { buildAwarenessContext } from '../tools/awareness-context.js';

interface SessionProjection {
  tools?: PiRuntimeObservation['tools'];
  contextFingerprint?: string;
}

const MAX_SESSION_PROJECTIONS = 8;
const MAX_PERSISTENCE_ATTEMPTS = 3;

export interface PiAwarenessObservationSinkOptions {
  enabled?: () => boolean;
  observe?: (ctx: PiContext, input: ContextObservation) => Promise<AwarenessOperationResult>;
  onError?: (error: unknown) => void;
}

function digest(value: unknown): string {
  return createHash('sha256').update(JSON.stringify(value)).digest('hex');
}

function toolOutcome(current: PiRuntimeObservation['tools'], prior: PiRuntimeObservation['tools']): 'success' | 'failure' | undefined {
  if (!current || current.total_observed <= (prior?.total_observed ?? 0)) return undefined;
  // Awareness's outcome sensor describes execution, not permission or cancellation.
  // Pi retains those distinct terminal states in its runtime observation.
  if (current.latest_outcome === 'succeeded') return 'success';
  if (current.latest_outcome === 'failed') return 'failure';
  return undefined;
}

function commitProjection(sessions: Map<string, SessionProjection>, key: string, projection: SessionProjection): void {
  sessions.delete(key);
  sessions.set(key, projection);
  while (sessions.size > MAX_SESSION_PROJECTIONS) {
    const oldest = sessions.keys().next().value as string | undefined;
    if (oldest === undefined) break;
    sessions.delete(oldest);
  }
}

/** Persist only changed, host-observed sensors; Pi execution remains fail-open. */
export function createPiAwarenessObservationSink(options: PiAwarenessObservationSinkOptions = {}) {
  const enabled = options.enabled ?? isPersistentStorageEnabledForExtension;
  const sessions = new Map<string, SessionProjection>();
  let queue = Promise.resolve();
  const reportError = (error: unknown): void => {
    try { options.onError?.(error); } catch { /* diagnostics cannot poison the observation queue */ }
  };

  return (observation: PiRuntimeObservation, ctx: PiContext): Promise<void> => {
    if (!enabled()) return Promise.resolve();
    let observe: NonNullable<PiAwarenessObservationSinkOptions['observe']>;
    try {
      if (options.observe) observe = options.observe;
      else {
        // Pi context getters and process bindings can change while this queue drains.
        const client = createAwarenessClient({
          ...buildAwarenessContext(ctx),
          sessionId: observation.session.session_id,
          signal: ctx.signal,
        });
        observe = (_ctx, input) => client.observe(input);
      }
    } catch (error) {
      reportError(error);
      return Promise.resolve();
    }
    queue = queue
      .then(async () => {
        const sessionKey = `${observation.session.session_id}:${observation.session.generation}`;
        const prior = sessions.get(sessionKey);
        const tools = observation.tools ? { ...observation.tools } : undefined;
        const outcome = toolOutcome(tools, prior?.tools);
        const context = observation.context?.input_limit_tokens
          ? { used: observation.context.current_tokens, limit: observation.context.input_limit_tokens }
          : undefined;
        const contextFingerprint = context ? digest(context) : undefined;
        const contextChanged = contextFingerprint !== undefined && prior?.contextFingerprint !== contextFingerprint;
        const projection = { ...prior, ...(tools ? { tools } : {}), ...(contextFingerprint ? { contextFingerprint } : {}) };
        if (!contextChanged && !outcome) {
          commitProjection(sessions, sessionKey, projection);
          return;
        }

        const observedAt = new Date(observation.session.observed_at).toISOString();
        const fingerprint = digest({ context, outcome });
        const report: ContextObservation = {
          observation_id: `pi_${digest([sessionKey, observation.session.observed_at, fingerprint, outcome ? tools?.total_observed : undefined]).slice(0, 40)}`,
          observed_at: observedAt,
          source: 'host',
          acquisition: 'passive',
          ...(context ? { context } : {}),
          ...(outcome ? { action_fingerprint: 'pi-runtime-tool', outcome } : {}),
        };
        let lastError: unknown;
        for (let attempt = 1; attempt <= MAX_PERSISTENCE_ATTEMPTS; attempt++) {
          try {
            const result = await observe(ctx, report);
            if (result.exitCode !== 0) throw new Error(`context.observe failed: ${JSON.stringify(result.payload)}`);
            commitProjection(sessions, sessionKey, projection);
            return;
          } catch (error) {
            lastError = error;
          }
        }
        throw lastError;
      })
      .catch(reportError);
    return queue;
  };
}
