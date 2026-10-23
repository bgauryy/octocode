// Protocol-level plan steps: raw CDP commands, protocol inventory, event listeners and IO streams.
// Each op takes the plan run state and the current step { step, index, timeout, row, target }.
import { openSync, writeSync, closeSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { jsonPointer } from './plan-validate.mjs';
import { exceptionMessage, waitCondition } from './plan-condition.mjs';
import { setTimeout as sleep } from 'node:timers/promises';

/** Prints an artifact line and its lossless artifact-query continuation. */
export function logQueryContinuation(log, label, file) {
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

function writeAll(fd, bytes) {
  for (let at = 0; at < bytes.length;)
    at += writeSync(fd, bytes, at, bytes.length - at);
}

async function readChunks(fd, handle, { step, row, target }) {
  for (;;) {
    const chunk = await target.send('IO.read', {
      handle,
      size: step.size ?? 65536,
    });
    writeAll(
      fd,
      Buffer.from(JSON.stringify({ index: row.chunks++, ...chunk }) + '\n')
    );
    if (chunk.eof) {
      row.complete = true;
      return;
    }
  }
}

// Cleanup has its own bounded deadline, even if the step timed out.
async function closeStream(connection, handle, { step, row, target }) {
  if (step.close === false) return null;
  try {
    await connection.send('IO.close', { handle }, target.targetInfo.sessionId, {
      timeoutMs: 1000,
    });
    return null;
  } catch (error) {
    row.cleanupError = error.message;
    return error;
  }
}

async function readStream(state, current) {
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
    readError;
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

function listen(state, { step, row, target }) {
  const file = join(state.cdp.outputDir, `events-${step.id}.jsonl`),
    fd = openSync(file, 'wx', 0o600);
  const stream = {
    target,
    file,
    fd,
    observed: 0,
    matched: 0,
    last: undefined,
    event: step.event,
    handler: undefined,
  };
  stream.handler = (params, meta: { sessionId?: string } = {}) => {
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
  target.on(step.event, stream.handler);
  state.streams.set(step.id, stream);
  row.artifact = file;
}

async function waitEvent(state, { step, timeout, row }) {
  const stream = state.streams.get(step.listener),
    begin = Date.now();
  while (stream.matched < (step.count ?? 1)) {
    if (Date.now() - begin >= timeout)
      throw new Error('Event condition timed out');
    await sleep(50);
  }
  row.observed = stream.observed;
  row.matched = stream.matched;
}

async function cdpCommand(state, { step, index, timeout, row, target }) {
  const result = await target.send(
    step.method,
    state.resolveParams(step.params || {})
  );
  row.artifact = state.cdp.saveArtifact(`cdp-${index + 1}.json`, result).file;
  state.artifacts.set(index + 1, row.artifact);
  if (result.exceptionDetails)
    throw new Error(exceptionMessage(result.exceptionDetails));
  if (step.after)
    row.condition = await waitCondition(target, step.after, timeout, state.log);
}

function protocolMember(domain, member) {
  return {
    domain: domain.domain,
    commands: domain.commands?.filter(c => c.name === member) || [],
    events: domain.events?.filter(e => e.name === member) || [],
    types: domain.types || [],
  };
}

async function protocol(state, { step, index, row }) {
  if (!state.protocol) {
    state.protocol = await state.cdp.protocol();
    state.cdp.saveArtifact('installed-protocol.json', state.protocol);
  }
  const domain = state.protocol.domains.find(d => d.domain === step.domain);
  if (!domain) throw new Error('Protocol domain not found');
  const selected = step.member ? protocolMember(domain, step.member) : domain;
  if (step.member && !selected.commands.length && !selected.events.length)
    throw new Error('Protocol member not found');
  row.artifact = state.cdp.saveArtifact(
    `protocol-${index + 1}.json`,
    selected
  ).file;
}

export const protocolOps = {
  readStream,
  listen,
  waitEvent,
  cdp: cdpCommand,
  protocol,
};
