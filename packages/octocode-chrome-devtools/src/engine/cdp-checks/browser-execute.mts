import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  closeSync,
} from 'node:fs';
import { join } from 'node:path';
import { run as monitor } from './live-har-monitor.mjs';
import { jsonPointer, validatePlan } from './plan-validate.mjs';
import { frameScope, scopedSession } from './plan-scopes.mjs';
import { pageOps } from './plan-page-ops.mjs';
import { logQueryContinuation, protocolOps } from './plan-protocol-ops.mjs';
import type { Payload } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import type { CheckSession } from '../cdp-runner-session.mjs';
import type {
  DisposableScope,
  EventStream,
  FrameScopes,
  PlanScope,
  PlanState,
  PlanStep,
  StepRow,
} from './plan-types.mjs';

/** Why the plan stopped: the failed step, or a failure outside any step. */
type Failure = {
  step?: number;
  error: string;
  outcomeUncertain?: boolean;
  instruction?: string;
};

export { DOM_ACTIONS } from './dom-config.mjs';
export { validatePlan } from './plan-validate.mjs';

const arg = (flag: string) => {
  const i = process.argv.indexOf(flag);
  return i < 0 ? null : process.argv[i + 1];
};
const ops = { ...pageOps, ...protocolOps };
const log = (line: string) => console.log(line);
const readPlanInput = () => {
  const file = arg('--plan');
  return JSON.parse(
    file ? readFileSync(file, 'utf8') : process.env.BROWSER_PLAN || 'null'
  );
};
const protocolErrorOf = (error: unknown) =>
  error instanceof Error && 'protocolError' in error
    ? error.protocolError
    : undefined;

/** What each plan event listener saw, for the run result. */
const eventCoverage = (streams: Map<string, EventStream>) =>
  [...streams.entries()].map(([id, stream]) => ({
    id,
    event: stream.event,
    observed: stream.observed,
    matched: stream.matched,
    artifact: stream.file,
    boundary: 'listener registration to flow completion',
  }));

/** The run failure a step error records; a timeout leaves the step's outcome unknown. */
function stepFailure(message: string, step: PlanStep, index: number): Failure {
  return {
    step: index + 1,
    error: message,
    ...(/CDP timeout|step deadline/.test(message)
      ? {
          outcomeUncertain: ['act', 'cdp', 'goto', 'readStream'].includes(
            step.op
          ),
          instruction:
            'Inspect state before any retry; an acknowledged outcome is unavailable.',
        }
      : {}),
  };
}

export async function run(connection: CheckSession, input?: unknown) {
  const plan = validatePlan(input ?? readPlanInput());
  const planArtifact = connection.saveArtifact('browser-plan.json', plan).file;
  const journalArtifact = join(connection.outputDir, 'browser-steps.jsonl');
  writeFileSync(journalArtifact, '', { mode: 0o600, flag: 'wx' });
  console.log(`[ARTIFACT] STEPS ${journalArtifact}`);
  const journal = (row: StepRow) =>
    appendFileSync(journalArtifact, JSON.stringify(row) + '\n');
  let stepDeadline: number | null = null;
  const cdp: CheckSession = {
    ...connection,
    foreground() {
      return cdp.send('Page.bringToFront');
    },
    send(method: string, params?: Payload, sessionId?: string) {
      const remaining =
        stepDeadline === null ? Infinity : stepDeadline - Date.now();
      if (remaining <= 0)
        return Promise.reject(
          new Error('Browser step deadline exceeded before ' + method)
        );
      return connection.send(method, params, sessionId, {
        timeoutMs: Math.min(plan.commandMs ?? plan.waitMs ?? 8000, remaining),
      });
    },
  };
  const results: StepRow[] = [],
    scopes: FrameScopes = new Map(),
    sessionScopes = new Map<string, DisposableScope>(),
    streams = new Map<string, EventStream>(),
    artifacts = new Map<unknown, string>();
  let failure: Failure | null = null;
  const started = Date.now();
  // A { $step | $event, pointer } reference resolves to the value it selects.
  const resolveReference = (
    step: unknown,
    event: unknown,
    pointer: unknown
  ) => {
    const artifact = step !== undefined ? artifacts.get(step) : undefined;
    if (step !== undefined && artifact === undefined)
      throw new Error('Result reference is unavailable');
    const source =
      artifact !== undefined
        ? JSON.parse(readFileSync(artifact, 'utf8'))
        : typeof event === 'string'
          ? streams.get(event)?.last
          : undefined;
    return jsonPointer(source, pointer ?? '');
  };
  const resolveParams = (value: unknown): unknown => {
    if (!value || typeof value !== 'object') return value;
    const step: unknown = Reflect.get(value, '$step'),
      event: unknown = Reflect.get(value, '$event');
    if (step !== undefined || event !== undefined) {
      const selected = resolveReference(
        step,
        event,
        Reflect.get(value, 'pointer')
      );
      if (selected === undefined)
        throw new Error('Result reference is unavailable');
      return selected;
    }
    return Array.isArray(value)
      ? value.map(resolveParams)
      : Object.fromEntries(
          Object.entries(value).map(([k, v]) => [k, resolveParams(v)])
        );
  };
  const state: PlanState = {
    plan,
    connection,
    cdp,
    streams,
    artifacts,
    protocol: null,
    resolveParams,
    log,
  };
  const execute = async () => {
    for (const [index, step] of plan.steps.entries()) {
      const timeout = step.timeoutMs ?? plan.waitMs ?? 8000,
        row: StepRow = {
          index: index + 1,
          op: step.op,
          status: 'running',
          startedAt: new Date().toISOString(),
        };
      journal(row);
      results.push(row);
      console.log(
        `[PROGRESS] BROWSER step=${index + 1}/${plan.steps.length} op=${step.op}`
      );
      const stepStarted = Date.now();
      stepDeadline = stepStarted + timeout;
      const progress = setInterval(
        () =>
          console.log(
            `[PROGRESS] BROWSER step=${index + 1} op=${step.op} elapsedMs=${Date.now() - stepStarted}`
          ),
        1000
      );
      try {
        let target: PlanScope = step.frame
          ? await frameScope(cdp, step.frame, {
              timeoutMs: timeout,
              scopes,
              log,
            })
          : cdp;
        if (step.session !== undefined) {
          const sessionId = resolveParams(step.session);
          if (typeof sessionId !== 'string' || !sessionId)
            throw new Error('Session reference is not an id');
          let scope = sessionScopes.get(sessionId);
          if (!scope) {
            scope = scopedSession(
              cdp,
              { targetId: null, type: 'session' },
              sessionId
            );
            sessionScopes.set(sessionId, scope);
          }
          target = scope;
        }
        row.targetId = target.targetInfo.id;
        row.source = {
          selectedUrl: target.targetInfo.url ?? null,
          frameId: target.targetInfo.frameId ?? null,
        };
        if (target.targetInfo.sessionId)
          row.sessionId = target.targetInfo.sessionId;
        await (ops[step.op] ?? protocolOps.protocol)(state, {
          step,
          index,
          timeout,
          row,
          target,
        });
        row.status = 'complete';
      } catch (error) {
        const protocolError = protocolErrorOf(error);
        if (protocolError)
          row.artifact = cdp.saveArtifact(
            `protocol-error-${index + 1}.json`,
            protocolError
          ).file;
        row.status = 'failed';
        row.error = errorMessage(error);
        failure = stepFailure(row.error, step, index);
        break;
      } finally {
        stepDeadline = null;
        clearInterval(progress);
        row.finishedAt = new Date().toISOString();
        row.source = {
          ...row.source,
          ...(row.condition?.url ? { url: row.condition.url } : {}),
          capturedAt: row.finishedAt,
        };
        journal(row);
      }
    }
  };
  try {
    if (plan.observe?.network)
      await monitor(cdp, {
        onReady: execute,
        monitorMs: plan.observe.afterMs ?? 0,
        url: '',
      });
    else await execute();
  } catch (error) {
    failure ??= { error: errorMessage(error) };
  } finally {
    for (const stream of streams.values()) {
      stream.target.off(stream.event, stream.handler);
      closeSync(stream.fd);
      logQueryContinuation(log, 'EVENTS', stream.file);
    }
    for (const scope of sessionScopes.values()) scope.dispose();
    for (const { scope, sessionId } of scopes.values()) {
      scope.dispose();
      if (sessionId)
        await cdp
          .send('Target.detachFromTarget', { sessionId })
          .catch(() => {});
    }
    const result = {
      ok: !failure,
      failure,
      planArtifact,
      journalArtifact,
      elapsedMs: Date.now() - started,
      requestedSteps: plan.steps.length,
      completedSteps: results.filter(r => r.status === 'complete').length,
      steps: results,
      coverage: {
        executedSteps: results.length,
        unvisitedSteps: plan.steps.length - results.length,
        boundary:
          'selected targets and frames during this plan; no site-wide coverage',
      },
      eventCoverage: eventCoverage(streams),
    };
    cdp.saveArtifact('browser-result.json', result);
    if (failure) {
      process.exitCode = 1;
      console.log(`[FINDING] BROWSER_STOPPED ${JSON.stringify(failure)}`);
    } else
      console.log(
        `[METRIC] BROWSER completed=${result.completedSteps}/${result.requestedSteps} elapsedMs=${result.elapsedMs}`
      );
  }
  return { ok: !failure, results };
}
