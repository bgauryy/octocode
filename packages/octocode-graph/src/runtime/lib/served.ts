// A run a live viewer serves: serve.ts writes <out>/server.json (mode 0600) while it runs. Commands that change the
// run (refresh) or should show their answer in the viewer (reply) go through that server, so one process writes
// the run and the page follows.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

/** <out>/server.json as the running viewer writes it. */
export interface ServerInfo {
  url: string;
  token: string;
  port: number;
  pid: number;
  out: string;
  root: string | null;
}

/** A server answer: the result, or { ok: false, error, problems | candidates } when it refused. */
export interface ServerAnswer {
  ok?: boolean;
  error?: string;
  problems?: unknown;
  candidates?: unknown;
  [field: string]: unknown;
}

/** Why a served request failed: 1 the server refused it, 3 no server answers for the run. */
export class ServedError extends Error {
  readonly exitCode: 1 | 3;
  constructor(message: string, exitCode: 1 | 3) {
    super(message);
    this.exitCode = exitCode;
  }
}

export function writeServerInfo(info: ServerInfo) {
  writeFileSync(join(info.out, 'server.json'), `${JSON.stringify(info)}\n`, {
    mode: 0o600,
  });
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/** The viewer serving `out`, or null: no server.json, or its process is gone (a stale file). */
export function serverOf(out: string): ServerInfo | null {
  let info: unknown;
  try {
    info = JSON.parse(readFileSync(join(out, 'server.json'), 'utf8'));
  } catch {
    return null;
  }
  if (!isRecord(info) || typeof info.pid !== 'number') return null;
  try {
    process.kill(info.pid, 0);
  } catch (error) {
    // EPERM: alive under another user; ESRCH: gone.
    if (!(isRecord(error) && error.code === 'EPERM')) return null;
  }
  return info as unknown as ServerInfo;
}

// Why a request never reached the server: the network error's code, else its message.
function unreachable(error: unknown) {
  if (!(error instanceof Error)) return String(error);
  const code = isRecord(error.cause) ? error.cause.code : undefined;
  return typeof code === 'string' ? code : error.message;
}

/** POST a JSON body to the viewer with its token; a refusal or an unreachable server throws ServedError. */
export async function postServer(
  info: ServerInfo,
  path: string,
  body: unknown
): Promise<ServerAnswer> {
  const url = new URL(path, info.url);
  url.search = new URLSearchParams({ t: info.token }).toString();
  let res: Response;
  try {
    res = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch (error) {
    throw new ServedError(
      `cannot reach the viewer at ${url.origin}: ${unreachable(error)}`,
      3
    );
  }
  // The viewer answers every POST with a JSON object.
  const answer = (await res
    .json()
    .catch(() => ({ ok: false, error: `HTTP ${res.status}` }))) as ServerAnswer;
  if (!res.ok) {
    const detail = answer.problems ?? answer.candidates;
    throw new ServedError(
      `${path} refused (${res.status}): ${answer.error ?? 'no detail'}${detail ? `\n${JSON.stringify(detail)}` : ''}`,
      1
    );
  }
  return answer;
}
