// The runner's workspace state: the run folder, per-port session metadata, run history,
// the resource map scripts share, and complete artifact writes with query continuations.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { basename, join } from 'path';

export function readJson(filePath, fallback = null) {
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch {
    return fallback;
  }
}

export function writeJson(filePath, value) {
  writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, {
    mode: 0o600,
  });
}

/** The per-port session folder (created): envDir, or <base>/octocode-chrome-devtools/tmp/session-meta/port-<port>. */
export function sessionMetaDir(base, envDir, port) {
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

const targetRow = t => ({
  id: t.id ?? null,
  type: t.type ?? null,
  url: t.url ?? null,
  title: t.title ?? null,
});

/** Records the target inventory; with attachedId each row says whether this run attached to it. */
export function writeTargetSnapshot(
  file,
  {
    capturedAt,
    port,
    targets,
    attachedId,
  }: {
    capturedAt: string;
    port: string;
    targets: { id?: string; type?: string; url?: string; title?: string }[];
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
export function createRunDir(base) {
  const ts = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
  const runs = join(base, 'octocode-chrome-devtools', 'tmp');
  mkdirSync(runs, { recursive: true });
  for (let n = 1; ; n++) {
    const dir = join(runs, n === 1 ? ts : `${ts}-${n}`);
    try {
      mkdirSync(dir);
      return dir;
    } catch (e) {
      if (e.code !== 'EEXIST') throw e;
    }
  }
}

const readRuns = file => {
  const history = readJson(file, { runs: [] }) ?? { runs: [] };
  if (!Array.isArray(history.runs)) history.runs = [];
  return history;
};

/** Appends this run to the history as running; returns finalize(status, extra) for its outcome. */
export function startRunHistory(runLogFile, entry) {
  const runHistory = readRuns(runLogFile);
  const runId = `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  runHistory.runs.push({ id: runId, ...entry, status: 'running' });
  writeJson(runLogFile, runHistory);
  return (status, extra = {}) => {
    const current = readRuns(runLogFile);
    const idx = current.runs.findIndex(r => r.id === runId);
    if (idx === -1) return;
    current.runs[idx] = {
      ...current.runs[idx],
      status,
      finishedAt: new Date().toISOString(),
      ...extra,
    };
    writeJson(runLogFile, current);
  };
}

/** Gives scripts the session folder: its files, the shared resource map and session metadata. */
export function attachSessionStore(
  cdp,
  { sessionMetaDir: dir, sessionMetaFile, targetSnapshotFile }
) {
  cdp.sessionMetaDir = dir;
  cdp.sessionMetaFile = sessionMetaFile;
  cdp.targetSnapshotFile = targetSnapshotFile;
  cdp.resourcesFile = join(dir, 'resource-map.json');
  cdp.upsertResourceMap = (resourceKey, details) => {
    const payload = readJson(cdp.resourcesFile, {
      updatedAt: null,
      resources: {},
    }) ?? { updatedAt: null, resources: {} };
    if (!payload.resources || typeof payload.resources !== 'object')
      payload.resources = {};
    payload.resources[resourceKey] = {
      ...payload.resources[resourceKey],
      ...details,
      updatedAt: new Date().toISOString(),
    };
    payload.updatedAt = new Date().toISOString();
    writeJson(cdp.resourcesFile, payload);
    return payload.resources[resourceKey];
  };
  cdp.readSessionMetadata = () => readJson(cdp.sessionMetaFile, {});
  cdp.writeSessionMetadata = patch => {
    const current = readJson(cdp.sessionMetaFile, {}) ?? {};
    const next = {
      ...current,
      ...patch,
      updatedAt: new Date().toISOString(),
    };
    writeJson(cdp.sessionMetaFile, next);
    return next;
  };
}

function writeArtifact(file, data, format) {
  if (existsSync(file))
    throw new Error(
      'Artifact exists; use a new name to preserve continuations'
    );
  writeFileSync(
    file,
    format === 'json' ? JSON.stringify(data, null, 2) + '\n' : data,
    { mode: 0o600 }
  );
}

/** cdp.saveArtifact(name, data, format): a new file in the run folder plus its artifact-query continuation. */
export function attachSaveArtifact(cdp, log) {
  cdp.saveArtifact = (name, data, format = 'json') => {
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
}
