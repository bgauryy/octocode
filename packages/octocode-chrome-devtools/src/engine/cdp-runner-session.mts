// The runner's workspace state: the run folder, per-port session metadata, run history,
// the resource map scripts share, and complete artifact writes with query continuations.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { basename, join } from 'path';
import type { Payload, Session } from './cdp-connection.mjs';
import { errorMessage } from './error-message.mjs';

type Json = Record<string, unknown>;
type Log = (line: string) => void;
/** A file the runner wrote plus the command that pages through it. */
export type SavedArtifact = {
  file: string;
  next: { continue: { command: string; args: string[] } };
};
/** The session folder and metadata accessors attachSessionStore adds to a connection. */
export type SessionStore = {
  sessionMetaDir: string;
  sessionMetaFile: string;
  targetSnapshotFile: string;
  resourcesFile: string;
  upsertResourceMap(resourceKey: string, details: Json): unknown;
  readSessionMetadata(): unknown;
  writeSessionMetadata(patch: Json): Json;
};
/** The connection a runner script's run(cdp) receives. */
export type RunnerSession = Session &
  SessionStore & {
    foreground(): Promise<unknown>;
    protocol(): Promise<Payload>;
    skillScriptsDir: string;
    saveArtifact(name: string, data: unknown, format?: string): SavedArtifact;
  };
/** What a cdp-checks module's run(cdp) needs from the runner session; the store fields stay optional. */
export type CheckSession = Session &
  Pick<RunnerSession, 'saveArtifact' | 'protocol' | 'foreground'> &
  Partial<Pick<RunnerSession, 'resourcesFile' | 'upsertResourceMap'>>;

export function readJson(filePath: string, fallback: unknown = null): unknown {
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch {
    return fallback;
  }
}

/** A JSON object file's fields; anything else (missing, unparsable, not an object) reads as `{}`. */
export function readJsonObject(filePath: string): Json {
  const value = readJson(filePath);
  return isJsonObject(value) ? value : {};
}

const isJsonObject = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

export function writeJson(filePath: string, value: unknown) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, {
    mode: 0o600,
  });
}

/** The per-port session folder (created): envDir, or <base>/octocode-chrome-devtools/tmp/session-meta/port-<port>. */
export function sessionMetaDir(
  base: string,
  envDir: string | null,
  port: string
) {
  const dir =
    envDir ||
    join(
      base,
      'octocode-chrome-devtools',
      'tmp',
      'session-meta',
      `port-${port}`
    );
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  return dir;
}

type TargetRow = { id?: string; type?: string; url?: string; title?: string };

const targetRow = (t: TargetRow) => ({
  id: t.id ?? null,
  type: t.type ?? null,
  url: t.url ?? null,
  title: t.title ?? null,
});

/** Records the target inventory; with attachedId each row says whether this run attached to it. */
export function writeTargetSnapshot(
  file: string,
  {
    capturedAt,
    port,
    targets,
    attachedId,
  }: {
    capturedAt: string;
    port: string;
    targets: TargetRow[];
    attachedId?: string;
  }
) {
  writeJson(file, {
    capturedAt,
    port,
    targets: targets.map(t =>
      attachedId === undefined
        ? targetRow(t)
        : { ...targetRow(t), attached: t.id === attachedId }
    ),
  });
}

/** A new run folder under <base>/octocode-chrome-devtools/tmp, named by its start second. */
export function createRunDir(base: string) {
  const ts = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
  const runs = join(base, 'octocode-chrome-devtools', 'tmp');
  mkdirSync(runs, { recursive: true });
  for (let n = 1; ; n++) {
    const dir = join(runs, n === 1 ? ts : `${ts}-${n}`);
    try {
      mkdirSync(dir);
      return dir;
    } catch (e) {
      if (!(e instanceof Error && 'code' in e && e.code === 'EEXIST')) throw e;
    }
  }
}

type RunHistory = Json & { runs: unknown[] };

const readRuns = (file: string): RunHistory => {
  const history = readJsonObject(file);
  return {
    ...history,
    runs: Array.isArray(history.runs) ? history.runs : [],
  };
};

type FinalizeRun = ReturnType<typeof startRunHistory>;

/** Appends this run to the history as running; returns finalize(status, extra) for its outcome. */
export function startRunHistory(runLogFile: string, entry: Json) {
  const runHistory = readRuns(runLogFile);
  const runId = `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  runHistory.runs.push({ id: runId, ...entry, status: 'running' });
  writeJson(runLogFile, runHistory);
  return (status: string, extra: Json = {}) => {
    const current = readRuns(runLogFile);
    const run = current.runs.find(
      (r): r is Json => isJsonObject(r) && r.id === runId
    );
    if (!run) return;
    current.runs[current.runs.indexOf(run)] = {
      ...run,
      status,
      finishedAt: new Date().toISOString(),
      ...extra,
    };
    writeJson(runLogFile, current);
  };
}

/** Gives scripts the session folder: its files, the shared resource map and session metadata. */
export function attachSessionStore<T extends Session>(
  cdp: T,
  {
    sessionMetaDir: dir,
    sessionMetaFile,
    targetSnapshotFile,
  }: {
    sessionMetaDir: string;
    sessionMetaFile: string;
    targetSnapshotFile: string;
  }
): T & SessionStore {
  // The accessors read the paths from the session, so a script that repoints them is honoured.
  const store: T & SessionStore = Object.assign(cdp, {
    sessionMetaDir: dir,
    sessionMetaFile,
    targetSnapshotFile,
    resourcesFile: join(dir, 'resource-map.json'),
    upsertResourceMap(resourceKey: string, details: Json) {
      const payload = readJson(store.resourcesFile, {
        updatedAt: null,
        resources: {},
      });
      return upsertResource(store.resourcesFile, payload, resourceKey, details);
    },
    readSessionMetadata: () => readJson(store.sessionMetaFile, {}),
    writeSessionMetadata(patch: Json) {
      const next = {
        ...readJsonObject(store.sessionMetaFile),
        ...patch,
        updatedAt: new Date().toISOString(),
      };
      writeJson(store.sessionMetaFile, next);
      return next;
    },
  });
  return store;
}

/** Merges details into one resource-map entry and writes the map back; returns the entry. */
function upsertResource(
  file: string,
  stored: unknown,
  resourceKey: string,
  details: Json
) {
  const payload = isJsonObject(stored) ? stored : {};
  const resources = isJsonObject(payload.resources) ? payload.resources : {};
  const previous = resources[resourceKey];
  resources[resourceKey] = {
    ...(isJsonObject(previous) ? previous : {}),
    ...details,
    updatedAt: new Date().toISOString(),
  };
  writeJson(file, {
    ...payload,
    resources,
    updatedAt: new Date().toISOString(),
  });
  return resources[resourceKey];
}

/** ArrayBuffer.isView is true exactly for TypedArrays and DataViews, the views writeFileSync takes. */
const isByteView = (value: unknown): value is NodeJS.ArrayBufferView =>
  ArrayBuffer.isView(value);

function writeArtifact(file: string, data: unknown, format: string) {
  if (existsSync(file))
    throw new Error(
      'Artifact exists; use a new name to preserve continuations'
    );
  const body = format === 'json' ? JSON.stringify(data, null, 2) + '\n' : data;
  // writeFileSync rejects any other value with the same TypeError before creating the file.
  if (typeof body !== 'string' && !isByteView(body))
    throw new TypeError(
      'A text or binary artifact must be a string, Buffer, TypedArray or DataView'
    );
  writeFileSync(file, body, { mode: 0o600 });
}

/** cdp.saveArtifact(name, data, format): a new file in the run folder plus its artifact-query continuation. */
export function attachSaveArtifact<
  T extends Session & { skillScriptsDir: string },
>(
  cdp: T,
  log: (line: string) => void
): T & Pick<RunnerSession, 'saveArtifact'> {
  const saveArtifact = (
    name: string,
    data: unknown,
    format = 'json'
  ): SavedArtifact => {
    if (!name || basename(name) !== name || name === '.' || name === '..')
      throw new Error('Artifact name must be a filename');
    if (!['json', 'text', 'binary'].includes(format))
      throw new Error('Invalid artifact format');
    const file = join(cdp.outputDir, name);
    writeArtifact(file, data, format);
    const next = {
      continue: {
        command: process.execPath,
        args: [
          join(cdp.skillScriptsDir, 'artifact-query.mjs'),
          '--file',
          file,
          '--format',
          format,
        ],
      },
    };
    log(`[ARTIFACT] ${name} ${file}`);
    log(`[NEXT] ${JSON.stringify(next)}`);
    return { file, next };
  };
  return Object.assign(cdp, { saveArtifact });
}

/** Records a failed run(cdp): a CDP failure asks for a retry (exit 2), anything else is an error (exit 1). */
export function reportRunError(
  cdp: SessionStore,
  finalizeRun: FinalizeRun,
  e: unknown,
  out: { log: Log; error: Log }
) {
  const message = errorMessage(e);
  if (/CDP error \[|CDP timeout/.test(message)) {
    const methodMatch =
      message.match(/for:\s*(\S+)/) ??
      message.match(/'([A-Z][a-zA-Z]+\.[a-zA-Z]+)'/);
    const method = methodMatch ? methodMatch[1] : 'unknown';
    out.log(`[CDP_RETRY_NEEDED] method=${method} error="${message}"`);
    out.log(
      `[CDP_RETRY_NEEDED] Inspect "${method}", its parameters, enabled domains and target readiness before retrying.`
    );
    cdp.writeSessionMetadata({
      lastRunStatus: 'retry-needed',
      lastError: message,
      lastErrorMethod: method,
    });
    finalizeRun('retry-needed', { error: message, errorMethod: method });
    return 2;
  }
  out.error(`[CDP_RUNNER] Script error: ${message}`);
  if (e instanceof Error && e.stack) out.error(e.stack);
  cdp.writeSessionMetadata({ lastRunStatus: 'error', lastError: message });
  finalizeRun('error', { error: message });
  return 1;
}
