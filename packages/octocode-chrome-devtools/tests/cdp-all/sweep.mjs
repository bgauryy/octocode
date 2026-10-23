// Sends a queue of CDP commands to one target scope (page, browser, tab or worker) and
// classifies every reply; a lost context is replayed to blame the command that caused it.
// The entry assigns `options` before the first sweep.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { artifact, engine, ensureBrowser, suite } from './browser.mjs';
import { classify } from './schema.mjs';

export const options = { via: 'script', batchSize: 120, commandMs: 3000 };
const lossClasses = new Set(['closed', 'engine']);
// A command is sent to the next target only while the previous one did not know it.
// tab and worker are flattened sessions attached from the browser connection: neither
// has its own /json entry the engine could select (tab) or a page that owns it (worker).
export const scopes = {
  page: { select: ['--new-tab', 'about:blank', '--close-tab'] },
  browser: { select: ['--browser'] },
  tab: { select: ['--browser'], attach: 'tab' },
  // A worker answers from its own thread and drops a method it lacks without any reply,
  // so its silence says nothing: it keeps the earlier target's -32601 and is not waited on.
  worker: {
    select: ['--browser'],
    attach: 'worker',
    commandMs: 500,
    silentUnknown: true,
  },
};

export const selectionFor = scope => scopes[scope].select;
const msFor = scope => scopes[scope].commandMs ?? options.commandMs;
const deadline = (count, scope) => String(count * msFor(scope) + 30000);

// Runs queue[from..] as one plan of cdp steps; a stop ends the batch.
// Steps that attach the session a tab or worker scope sends through; they head each plan.
// $step counts from 1.
const attachSteps = type =>
  type
    ? [
        {
          op: 'cdp',
          method: 'Target.getTargets',
          params: { filter: [{ type }] },
        },
        {
          op: 'cdp',
          method: 'Target.attachToTarget',
          params: {
            targetId: { $step: 1, pointer: '/targetInfos/0/targetId' },
            flatten: true,
          },
        },
      ]
    : [];
async function attemptRun(queue, from, scope) {
  const batch = queue.slice(from, from + options.batchSize);
  const file = join(suite.work, 'plan.json');
  const prelude = attachSteps(scopes[scope].attach);
  const session = prelude.length
    ? { session: { $step: 2, pointer: '/sessionId' } }
    : {};
  writeFileSync(
    file,
    JSON.stringify({
      commandMs: msFor(scope),
      waitMs: msFor(scope),
      steps: [
        ...prelude,
        ...batch.map(({ method, params }) => ({
          op: 'cdp',
          method,
          params,
          ...session,
        })),
      ],
    })
  );
  const r = await engine(
    [
      'run',
      ...selectionFor(scope),
      '--port',
      suite.port,
      '--script-timeout',
      deadline(batch.length, scope),
      '--plan',
      file,
    ],
    Number(deadline(batch.length, scope)) + 30000
  );
  const resultFile = artifact(r.out, 'browser-result.json');
  if (!resultFile)
    return [{ index: from, ...classify(r.out.slice(-600), false) }];
  const steps = JSON.parse(readFileSync(resultFile, 'utf8')).steps;
  // A target that cannot be attached is a suite failure, not an answer from the target.
  if (steps.slice(0, prelude.length).some(step => step.status !== 'complete'))
    throw Error(
      `Cannot attach a ${scopes[scope].attach} target: ${JSON.stringify(steps.slice(0, prelude.length))}`
    );
  return steps.slice(prelude.length).map((step, offset) =>
    step.status === 'complete'
      ? { index: from + offset, class: 'ok' }
      : {
          index: from + offset,
          // A saved cdp-N.json means Chrome replied and the engine rejected exceptionDetails.
          ...classify(
            step.error ?? 'missing step error',
            /\/cdp-\d+\.json$/.test(step.artifact ?? '')
          ),
        }
  );
}

// Sends queue[from..] over one long-lived sandboxed connection; the queue is inlined in a
// generated module and replies stream back as [CDPALL] lines. Context loss ends the run.
async function attemptScript(queue, from, scope) {
  const batch = queue.slice(from);
  const file = join(suite.work, 'cdp-all-worker.mjs');
  writeFileSync(
    file,
    `const queue = ${JSON.stringify(batch.map(({ method, params }) => ({ method, params })))};
const attach = ${JSON.stringify(scopes[scope].attach ?? null)};
export async function run(cdp) {
  let sessionId;
  if (attach) {
    const { targetInfos } = await cdp.send('Target.getTargets', { filter: [{ type: attach }] });
    if (!targetInfos.length) {
      console.log('[CDPALL] ' + JSON.stringify({ setup: 'No ' + attach + ' target to attach' }));
      return;
    }
    ({ sessionId } = await cdp.send('Target.attachToTarget', { targetId: targetInfos[0].targetId, flatten: true }));
  }
  for (const [offset, row] of queue.entries()) {
    let reply;
    try {
      const result = await cdp.send(row.method, row.params, sessionId, { timeoutMs: ${msFor(scope)} });
      reply = result?.exceptionDetails ? { offset, replied: true, error: String(result.exceptionDetails.text) } : { offset, ok: true };
    } catch (error) {
      reply = { offset, error: error.message };
    }
    console.log('[CDPALL] ' + JSON.stringify(reply));
    if (reply.error && !reply.replied && !/^CDP error \\[/.test(reply.error)) return;
  }
}
`
  );
  const r = await engine(
    [
      'script',
      file,
      ...selectionFor(scope),
      '--port',
      suite.port,
      '--script-timeout',
      deadline(batch.length, scope),
    ],
    Number(deadline(batch.length, scope)) + 30000
  );
  const replies = r.out
    .split('\n')
    .filter(line => line.startsWith('[CDPALL] '))
    .map(line => JSON.parse(line.slice(9)));
  const setup = replies.find(row => row.setup);
  if (setup) throw Error(setup.setup);
  const rows = replies.map(row =>
    row.ok
      ? { index: from + row.offset, class: 'ok' }
      : { index: from + row.offset, ...classify(row.error, row.replied) }
  );
  return rows.length
    ? rows
    : [{ index: from, ...classify(r.out.slice(-600), false) }];
}
const attempt = (queue, from, scope) =>
  (options.via === 'run' ? attemptRun : attemptScript)(queue, from, scope);

// Replays items on a fresh target; reports whether the context was lost at the last item.
async function replay(items, scope) {
  const rows = await attempt(items, 0, scope),
    died = await ensureBrowser(),
    last = rows.at(-1);
  return {
    rows,
    died,
    lost:
      died || (lossClasses.has(last.class) && last.index === items.length - 1),
  };
}

// A lost context is blamed on the command that reproduces it: the lost command alone,
// or the command before it on the same connection (whose reply may precede the loss,
// e.g. Page.close or Browser.close). An unreproduced loss is a transport failure.
const destroyed = died => (died ? 'browser-destroyed' : 'target-destroyed');
async function attribute(queue, outcome, loss, { from, scope }) {
  const index = loss.index,
    alone = await replay([queue[index]], scope);
  let blamed = false;
  if (index > from) {
    const previous = index - 1,
      before = await replay([queue[previous]], scope);
    const pair =
      before.died || alone.lost
        ? before
        : await replay([queue[previous], queue[index]], scope);
    if (pair.lost) {
      outcome[previous] = {
        ...outcome[previous],
        class: destroyed(pair.died),
        reply: outcome[previous].class,
      };
      blamed = true;
    }
  }
  if (alone.lost)
    return { index, class: destroyed(alone.died), error: loss.error };
  const retried = { ...alone.rows[0], index };
  return blamed ? retried : { ...retried, class: 'transport', first: loss };
}

export async function sweep(queue, scope) {
  const outcome = Array.from({ length: queue.length });
  let from = 0;
  while (from < queue.length) {
    const rows = await attempt(queue, from, scope);
    for (const row of rows) outcome[row.index] = row;
    const last = rows.at(-1);
    from = last.index + 1;
    // Chrome-routed stops just resume; a hung or lost context gets a fresh target first.
    if (!['timeout', ...lossClasses].includes(last.class)) continue;
    const died = await ensureBrowser();
    if (last.class === 'timeout') continue;
    outcome[last.index] = await attribute(
      queue,
      outcome,
      { ...last, browserDied: died },
      { from: rows[0].index, scope }
    );
  }
  return outcome;
}
