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

export { DOM_ACTIONS as actions } from './dom-config.mjs';
export { planOperations, validatePlan } from './plan-validate.mjs';

const arg = flag => {
  const i = process.argv.indexOf(flag);
  return i < 0 ? null : process.argv[i + 1];
};
const ops = { ...pageOps, ...protocolOps };
const log = line => console.log(line);

export async function run(connection, input) {
  const plan = validatePlan(
    input ??
      JSON.parse(
        arg('--plan')
          ? readFileSync(arg('--plan'), 'utf8')
          : process.env.BROWSER_PLAN || 'null'
      )
  );
  const planArtifact = connection.saveArtifact('browser-plan.json', plan).file;
  const journalArtifact = join(connection.outputDir, 'browser-steps.jsonl');
  writeFileSync(journalArtifact, '', { mode: 0o600, flag: 'wx' });
  console.log(`[ARTIFACT] STEPS ${journalArtifact}`);
  const journal = row =>
    appendFileSync(journalArtifact, JSON.stringify(row) + '\n');
  let stepDeadline = null;
  const cdp = {
    ...connection,
    foreground() {
      return cdp.send('Page.bringToFront');
    },
    send(method, params, sessionId) {
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
  const results = [],
    scopes = new Map(),
    sessionScopes = new Map(),
    streams = new Map(),
    artifacts = new Map();
  let failure = null;
  const started = Date.now();
  const resolveParams = value => {
    if (!value || typeof value !== 'object') return value;
    if (value.$step !== undefined || value.$event !== undefined) {
      const source =
        value.$step !== undefined
          ? JSON.parse(readFileSync(artifacts.get(value.$step), 'utf8'))
          : streams.get(value.$event)?.last;
      const selected = jsonPointer(source, value.pointer ?? '');
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
  const state = {
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
        row: Record<string, any> = {
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
        let target = step.frame
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
          if (!sessionScopes.has(sessionId))
            sessionScopes.set(
              sessionId,
              scopedSession(cdp, { targetId: null, type: 'session' }, sessionId)
            );
          target = sessionScopes.get(sessionId);
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
        if (error.protocolError)
          row.artifact = cdp.saveArtifact(
            `protocol-error-${index + 1}.json`,
            error.protocolError
          ).file;
        row.status = 'failed';
        row.error = error.message;
        failure = {
          step: index + 1,
          error: error.message,
          ...(/CDP timeout|step deadline/.test(error.message)
            ? {
                outcomeUncertain: ['act', 'cdp', 'goto', 'readStream'].includes(
                  step.op
                ),
                instruction:
                  'Inspect state before any retry; an acknowledged outcome is unavailable.',
              }
            : {}),
        };
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
    failure ??= { error: error.message };
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
      eventCoverage: [...streams.entries()].map(([id, stream]) => ({
        id,
        event: stream.event,
        observed: stream.observed,
        matched: stream.matched,
        artifact: stream.file,
        boundary: 'listener registration to flow completion',
      })),
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
