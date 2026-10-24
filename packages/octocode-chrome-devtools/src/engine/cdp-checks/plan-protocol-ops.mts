// Protocol-level plan steps: raw CDP commands, protocol inventory, event listeners and IO streams.
// Each op takes the plan run state and the current step { step, index, timeout, row, target }.
import { openSync, writeSync, closeSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { jsonPointer } from './plan-validate.mjs';
import { exceptionMessage, waitCondition } from './plan-condition.mjs';
import { setTimeout as sleep } from 'node:timers/promises';
import type { Handler, Payload } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import type { CheckSession } from '../cdp-runner-session.mjs';
import type {
  EventStream,
  PlanOp,
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
    '[NEXT] ' +
      JSON.stringify({
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
      })
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
    writeAll(fd, Buffer.from(JSON.stringify({ index, ...chunk }) + '\n'));
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

function listen(state: PlanState, { step, row, target }: StepContext) {
  const { id, event } = step;
  if (id === undefined || event === undefined)
    throw new Error('listen needs id and event'); // validatePlan requires both
  const file = join(state.cdp.outputDir, `events-${id}.jsonl`),
    fd = openSync(file, 'wx', 0o600);
  const handler: Handler = (params, meta: { sessionId?: string } = {}) => {
    writeAll(
      fd,
      Buffer.from(
        JSON.stringify({
          at: new Date().toISOString(),
          params,
          sessionId: meta.sessionId ?? null,
        }) + '\n'
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
  while (stream.matched < (step.count ?? 1)) {
    if (Date.now() - begin >= timeout)
      throw new Error('Event condition timed out');
    await sleep(50);
  }
  row.observed = stream.observed;
  row.matched = stream.matched;
}

async function cdpCommand(
  state: PlanState,
  { step, index, timeout, row, target }: StepContext
) {
  if (step.method === undefined) throw new Error('cdp needs Domain.method'); // validatePlan requires it
  const params = state.resolveParams(step.params || {});
  if (typeof params !== 'object' || params === null)
    throw new Error('CDP params must resolve to an object');
  const result = await target.send(step.method, params);
  const artifact = state.cdp.saveArtifact(`cdp-${index + 1}.json`, result).file;
  row.artifact = artifact;
  state.artifacts.set(index + 1, artifact);
  if (result.exceptionDetails)
    throw new Error(exceptionMessage(result.exceptionDetails));
  if (step.after)
    row.condition = await waitCondition(target, step.after, timeout, state.log);
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

export const protocolOps: Record<string, PlanOp> = {
  readStream,
  listen,
  waitEvent,
  cdp: cdpCommand,
  protocol,
};
