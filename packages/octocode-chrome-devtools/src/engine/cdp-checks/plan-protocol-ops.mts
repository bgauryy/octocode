// Protocol-level plan steps: raw CDP commands, protocol inventory, event listeners and IO streams.
// Each op takes the plan run state and the current step { step, index, timeout, row, target }.
import { openSync, writeSync, closeSync } from 'node:fs';
import { stageSave } from './plan-save.mjs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { jsonPointer } from './plan-validate.mjs';
import { exceptionMessage, waitCondition } from './plan-condition.mjs';
import { setTimeout as sleep } from 'node:timers/promises';
import type { Handler, Payload } from '../cdp-connection.mjs';
import { gateCdpCall, trySchema } from '../cdp-schema.mjs';
import { BrowserHub, CdpRouter } from '../cdp-scope.mjs';
import { connectCDP } from '../cdp-connection.mjs';
import { cdpHttp } from '../engine-io.mjs';
import { errorMessage } from '../error-message.mjs';
import type { CheckSession } from '../cdp-runner-session.mjs';
import type {
  EventStream,
  PlanOp,
  PlanScope,
  PlanState,
  StepContext,
} from './plan-types.mjs';

/** Prints an artifact line and its lossless artifact-query continuation. */
export function logQueryContinuation(
  log: (line: string) => void,
  label: string,
  file: string
) {
  log(`[ARTIFACT] ${label} ${file}`);
  log(
    `[NEXT] ${JSON.stringify({
      continue: {
        command: process.execPath,
        args: [
          fileURLToPath(new URL('../artifact-query.mjs', import.meta.url)),
          '--file',
          file,
          '--format',
          'text',
        ],
      },
    })}`
  );
}

function writeAll(fd: number, bytes: Buffer) {
  for (let at = 0; at < bytes.length;)
    at += writeSync(fd, bytes, at, bytes.length - at);
}

async function readChunks(
  fd: number,
  handle: string,
  { step, row, target }: StepContext
) {
  for (;;) {
    const chunk = await target.send('IO.read', {
      handle,
      size: step.size ?? 65536,
    });
    const index = row.chunks ?? 0; // readStream starts the count at 0
    row.chunks = index + 1;
    writeAll(fd, Buffer.from(`${JSON.stringify({ index, ...chunk })}\n`));
    if (chunk.eof) {
      row.complete = true;
      return;
    }
  }
}

// Cleanup has its own bounded deadline, even if the step timed out.
async function closeStream(
  connection: CheckSession,
  handle: string,
  { step, row, target }: StepContext
) {
  if (step.close === false) return null;
  try {
    await connection.send('IO.close', { handle }, target.targetInfo.sessionId, {
      timeoutMs: 1000,
    });
    return null;
  } catch (error) {
    row.cleanupError = errorMessage(error);
    return error;
  }
}

async function readStream(state: PlanState, current: StepContext) {
  const { index, row } = current;
  const handle = state.resolveParams(current.step.handle);
  if (typeof handle !== 'string')
    throw new Error('Stream reference is not a handle');
  const file = join(state.cdp.outputDir, `stream-${index + 1}.jsonl`),
    fd = openSync(file, 'wx', 0o600);
  row.artifact = file;
  row.chunks = 0;
  row.complete = false;
  let readFailed = false,
    readError: unknown;
  try {
    await readChunks(fd, handle, current);
  } catch (error) {
    readFailed = true;
    readError = error;
  }
  const cleanupError = await closeStream(state.connection, handle, current);
  closeSync(fd);
  logQueryContinuation(state.log, 'STREAM', file);
  if (readFailed) throw readError;
  if (cleanupError) throw cleanupError;
}

/** The runner's --port: the browser whose installed schema validates raw calls. */
const portArgument = () => {
  const at = process.argv.indexOf('--port');
  return at < 0 ? undefined : process.argv[at + 1];
};

/** The page-method hint when a raw cdp call fell back to the browser because no single tab was chosen. */
function fallbackHint(pages: unknown) {
  if (typeof pages !== 'number') return undefined;
  const why = pages
    ? `${pages} tabs are open and none is selected`
    : 'no tab is open';
  return `${why}, so this ran on the browser target; page methods need connection.target (see targets) or connection.newTab`;
}

const hubs = new WeakMap<PlanState, BrowserHub>();
const routers = new WeakMap<PlanScope, CdpRouter>();

/** The router of one target: page lane per target, one browser connection per run (opened on first use). */
async function routerFor(state: PlanState, target: PlanScope) {
  const known = routers.get(target);
  if (known) return known;
  const port = portArgument(),
    portNumber = port ?? '9222';
  const commandMs = state.plan.commandMs ?? state.plan.waitMs ?? 8000;
  let hub = hubs.get(state);
  if (!hub) {
    hub = new BrowserHub(async () => {
      const version = await cdpHttp(Number(portNumber), '/json/version');
      const url: unknown = version?.webSocketDebuggerUrl;
      if (typeof url !== 'string')
        throw new Error(
          `Chrome on port ${portNumber} gave no browser WebSocket URL`
        );
      const connection = await connectCDP(url, {
        targetInfo: { type: 'browser' },
        timeoutMs: commandMs,
      });
      return {
        send: (method, params) =>
          connection.send(method, params, undefined, { timeoutMs: commandMs }),
        close: () => connection.close(),
      };
    });
    hubs.set(state, hub);
  }
  const { schema } = await trySchema(port);
  const router = new CdpRouter({
    schema,
    page: (method, params) => target.send(method, params),
    pageIsBrowser: target.targetInfo.type === 'browser',
    pageHint: fallbackHint(target.targetInfo.ambiguousPages),
    hub,
    version: async () => {
      const info = await cdpHttp(Number(portNumber), '/json/version').catch(
        () => undefined
      );
      return typeof info?.Browser === 'string' ? info.Browser : undefined;
    },
    log: state.log,
  });
  routers.set(target, router);
  return router;
}

/** Closes the browser connection a run opened for scope routing. */
export function closeRouting(state: PlanState) {
  hubs.get(state)?.close();
  hubs.delete(state);
}

/** Plan-level `enable`: the domains every step may rely on, enabled before step 1. */
export async function enablePlanDomains(state: PlanState) {
  if (!state.plan.enable?.length) return;
  const router = await routerFor(state, state.cdp);
  const { enabled, notes } = await router.enable(state.plan.enable);
  state.log(`[NOTE] plan enabled: ${enabled.join(', ') || 'nothing new'}`);
  for (const note of notes) state.log(`[NOTE] ${note}`);
}

async function listen(state: PlanState, { step, row, target }: StepContext) {
  const { id, event } = step;
  if (id === undefined || event === undefined)
    throw new Error('listen needs id and event'); // validatePlan requires both
  const events = await (await routerFor(state, target)).ensureEvents(event);
  if (events.enabled.length) row.enabled = events.enabled;
  if (events.note) row.note = events.note;
  const file = join(state.cdp.outputDir, `events-${id}.jsonl`),
    fd = openSync(file, 'wx', 0o600);
  const handler: Handler = (params, meta: { sessionId?: string } = {}) => {
    writeAll(
      fd,
      Buffer.from(
        `${JSON.stringify({
          at: new Date().toISOString(),
          params,
          sessionId: meta.sessionId ?? null,
        })}\n`
      )
    );
    stream.observed++;
    if (
      Object.entries(step.where || {}).every(
        ([path, expected]) => jsonPointer(params, path) === expected
      )
    ) {
      stream.matched++;
      stream.last = params;
    }
  };
  const stream: EventStream = {
    target,
    file,
    fd,
    observed: 0,
    matched: 0,
    last: undefined,
    event,
    handler,
  };
  target.on(event, handler);
  state.streams.set(id, stream);
  row.artifact = file;
}

async function waitEvent(
  state: PlanState,
  { step, timeout, row }: StepContext
) {
  const stream =
      step.listener === undefined
        ? undefined
        : state.streams.get(step.listener),
    begin = Date.now();
  // validatePlan only accepts a listener registered by an earlier listen step.
  if (!stream) throw new Error('waitEvent needs an earlier listener');
  await (await routerFor(state, stream.target)).ensureEvents(stream.event);
  while (stream.matched < (step.count ?? 1)) {
    if (Date.now() - begin >= timeout)
      throw new Error('Event condition timed out');
    await sleep(50);
  }
  row.observed = stream.observed;
  row.matched = stream.matched;
}

async function cdpCommand(state: PlanState, current: StepContext) {
  const { step, index, timeout, row, target } = current;
  if (step.method === undefined) throw new Error('cdp needs Domain.method'); // validatePlan requires it
  const params = state.resolveParams(step.params || {});
  if (typeof params !== 'object' || params === null)
    throw new Error('CDP params must resolve to an object');
  const refusal = await gateCdpCall(
    { method: step.method, params, acknowledge: step.acknowledge },
    portArgument(),
    state.log
  );
  if (refusal) throw new Error(refusal);
  const router = await routerFor(state, target),
    scope = step.scope ?? 'auto';
  const explicit = step.enable?.length
    ? await router.enable(step.enable, scope)
    : { enabled: [], notes: [] };
  const outcome = await router.route(step.method, params, scope);
  const result = outcome.result;
  const enabled = [...explicit.enabled, ...outcome.enabled];
  if (outcome.scope === 'browser') row.scope = 'browser';
  if (enabled.length) row.enabled = enabled;
  if (explicit.notes.length) row.note = explicit.notes.join('; ');
  const artifact = state.cdp.saveArtifact(`cdp-${index + 1}.json`, result).file;
  row.artifact = artifact;
  state.artifacts.set(index + 1, artifact);
  if (result.exceptionDetails)
    throw new Error(exceptionMessage(result.exceptionDetails));
  if (step.after)
    row.condition = await waitCondition(target, step.after, timeout, state.log);
  stageSave(current, result);
}

type ProtocolEntry = { name: string };

function protocolMember(domain: Payload, member: string) {
  return {
    domain: domain.domain,
    commands:
      domain.commands?.filter((c: ProtocolEntry) => c.name === member) || [],
    events:
      domain.events?.filter((e: ProtocolEntry) => e.name === member) || [],
    types: domain.types || [],
  };
}

async function protocol(state: PlanState, { step, index, row }: StepContext) {
  if (!state.protocol) {
    state.protocol = await state.cdp.protocol();
    state.cdp.saveArtifact('installed-protocol.json', state.protocol);
  }
  const domain = state.protocol.domains.find(
    (d: Payload) => d.domain === step.domain
  );
  if (!domain) throw new Error('Protocol domain not found');
  const selected = step.member ? protocolMember(domain, step.member) : domain;
  if (step.member && !selected.commands.length && !selected.events.length)
    throw new Error('Protocol member not found');
  row.artifact = state.cdp.saveArtifact(
    `protocol-${index + 1}.json`,
    selected
  ).file;
}

/** Typed page-state read: Runtime.evaluate by value in the step's target, frame or session. */
async function evaluate(state: PlanState, current: StepContext) {
  const { step, index, timeout, row, target } = current;
  const evaluated = await target.send('Runtime.evaluate', {
    expression: step.expression,
    returnByValue: true,
    awaitPromise: step.awaitPromise === true,
  });
  if (evaluated.exceptionDetails) {
    row.artifact = state.cdp.saveArtifact(
      `evaluate-error-${index + 1}.json`,
      evaluated
    ).file;
    throw new Error(exceptionMessage(evaluated.exceptionDetails));
  }
  row.data = { result: evaluated.result?.value };
  if (step.after)
    row.condition = await waitCondition(target, step.after, timeout, state.log);
  stageSave(current, evaluated.result?.value);
}

export const protocolOps: Record<string, PlanOp> = {
  readStream,
  evaluate,
  listen,
  waitEvent,
  cdp: cdpCommand,
  protocol,
};
