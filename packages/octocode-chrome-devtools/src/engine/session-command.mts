// `session start|list|show|close`: durable research sessions in the caller's
// workspace (see session-store.mts). Output is one JSON object on stdout.
import { chromeSessionStore } from './chrome-contract.mjs';
import { flagArgs, validateFlags } from './cli-flags.mjs';
import { cdpHttp } from './engine-io.mjs';
import { errorMessage } from './error-message.mjs';
import {
  acquireLease,
  assertId,
  createSession,
  isRecord,
  listSessions,
  openSession,
  readLease,
  releaseLease,
  updateSession,
} from './session-store.mjs';

const { errors } = chromeSessionStore;
const ACTIONS = ['start', 'list', 'show', 'close'];

/** The browser identity Chrome reports on `port`, which a session binds. */
async function browserIdentity(port: number): Promise<string> {
  let version: unknown;
  try {
    version = await cdpHttp(port, '/json/version', { timeoutMs: 3000 });
  } catch (error) {
    throw Error(
      `${errors.unreachable} (port ${port}: ${errorMessage(error)})`,
      {
        cause: error,
      }
    );
  }
  const socket = isRecord(version) ? version.webSocketDebuggerUrl : undefined;
  if (typeof socket !== 'string' || !socket.includes('/devtools/browser/'))
    throw Error(`${errors.unreachable} (port ${port} reported no browser id)`);
  return socket.slice(socket.lastIndexOf('/') + 1);
}

function known(workspace: string, id: string) {
  assertId(id, 's');
  if (!listSessions(workspace).includes(id))
    throw Error(`${errors.unknown} (${id})`);
  try {
    return openSession(workspace, id);
  } catch (error) {
    throw Error(`${errors.malformed} (${errorMessage(error)})`, {
      cause: error,
    });
  }
}

/** The action and its flags, each checked against what the action takes. */
function parse(args: string[]) {
  validateFlags(args, ['--action', '--id', '--port']);
  const { getArg } = flagArgs(args);
  const action = getArg('--action');
  if (action === undefined || !ACTIONS.includes(action))
    throw Error(`session needs one of: ${ACTIONS.join(', ')}`);
  const id = getArg('--id'),
    port = getArg('--port');
  const needsId = action === 'show' || action === 'close';
  if (needsId !== (id !== undefined))
    throw Error(
      needsId ? `${action} requires --id` : `${action} does not take --id`
    );
  if (port !== undefined && action !== 'start')
    throw Error('--port applies only to start');
  return { action, id: id ?? '', port: Number(port ?? '9222') };
}

async function start(workspace: string, port: number) {
  if (!Number.isInteger(port) || port < 1 || port > 65535)
    throw Error('--port needs a port number');
  const incarnation = await browserIdentity(port);
  return {
    session: createSession(workspace, {
      incarnation,
      endpoint: `http://127.0.0.1:${port}`,
    }),
  };
}

function list(workspace: string) {
  return {
    sessions: listSessions(workspace).map(session => {
      try {
        const manifest = openSession(workspace, session);
        return {
          id: session,
          lifecycle: manifest.lifecycle,
          pages: manifest.pages.length,
          updatedAt: manifest.updatedAt,
        };
      } catch (error) {
        return { id: session, error: errorMessage(error) };
      }
    }),
  };
}

/** Marks a session closed under its writer lease. */
function close(workspace: string, id: string) {
  const manifest = known(workspace, id);
  let lease;
  try {
    lease = acquireLease(workspace, manifest.id, manifest.browser.incarnation);
  } catch (error) {
    throw Error(`${errors.leased} (${errorMessage(error)})`, { cause: error });
  }
  try {
    return {
      session: updateSession(workspace, manifest.id, value => {
        value.lifecycle = 'closed';
      }),
    };
  } finally {
    releaseLease(workspace, manifest.id, lease);
  }
}

export async function sessionCommand(
  args: string[],
  workspace = process.cwd()
): Promise<unknown> {
  const { action, id, port } = parse(args);
  if (action === 'start') return start(workspace, port);
  if (action === 'list') return list(workspace);
  if (action === 'close') return close(workspace, id);
  const manifest = known(workspace, id);
  return { session: manifest, lease: readLease(workspace, manifest.id) };
}
