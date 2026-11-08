// A per-run snapshot of the build under test. `build:dev` rewrites the native binary, the CLI
// launcher and the MCP server in place, so a run that reads them live can meet a half-written
// file (CLI exit 1 `/ must be object`, an empty JSON frame). The run copies them once into a
// private temp folder and every CLI and MCP call executes the copies:
//   native binary  -> OCTOCODE_NATIVE_BIN (the launchers' own override) pointing at the copy
//   CLI launcher   -> <snapshot>/cli  (a copy of packages/octocode/out)
//   MCP server     -> <snapshot>/mcp  (a copy of packages/octocode-mcp/dist)
// The process that creates the snapshot owns it and removes it on exit; child suites inherit it
// through SNAPSHOT_ENV and only read it.
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { devOverridesAllowed } from '@octocodeai/config';
import { CLI_SOURCE_DIR, MCP_SOURCE_DIR, ROOT, SNAPSHOT_ENV, useBuildPaths } from './paths.mjs';
import { nativeBinaryPath } from './native-binary.mjs';

/** A source still being rewritten changes under the copy; retry until it holds still across one copy. */
const ATTEMPTS = 120;
const RETRY_MS = 1000;

const sha256 = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const sleep = ms => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
const identity = file => { const s = fs.statSync(file); return `${s.size}:${s.mtimeMs}:${s.ino}`; };
const filesOf = dir => fs.readdirSync(dir, { recursive: true, withFileTypes: true }).filter(e => e.isFile()).map(e => path.relative(dir, path.join(e.parentPath, e.name))).sort();

/** Copy `source` (a file or folder) to `dest` once it is stable and non-empty; returns the copied files. */
function stableCopy(source, dest, label) {
  const isDir = fs.statSync(source).isDirectory();
  const names = isDir ? filesOf(source) : [''];
  for (let attempt = 1; attempt <= ATTEMPTS; attempt++) {
    const before = names.map(name => identity(path.join(source, name)));
    fs.rmSync(dest, { recursive: true, force: true });
    if (isDir) fs.cpSync(source, dest, { recursive: true });
    else fs.copyFileSync(source, dest, fs.constants.COPYFILE_FICLONE);
    const after = names.map(name => identity(path.join(source, name)));
    const empty = names.some(name => fs.statSync(path.join(dest, name)).size === 0);
    if (!empty && before.every((id, i) => id === after[i])) return names;
    sleep(RETRY_MS);
  }
  throw new Error(`build snapshot: ${label} (${source}) kept changing or stayed empty for ${ATTEMPTS} attempts; a rebuild is still running`);
}

const describe = (source, copy, names) => {
  const files = names.map(name => {
    const file = path.join(copy, name), original = path.join(source, name);
    return { name: name || path.basename(copy), bytes: fs.statSync(file).size, sha256: sha256(file), sourceMtime: fs.statSync(original).mtime.toISOString() };
  });
  return { source: path.relative(ROOT, source), copy, files };
};

const manifestFile = dir => path.join(dir, 'manifest.json');
let owned;

/**
 * Snapshot the build under test for this run (idempotent), point the harness at the copies and
 * return the manifest. A suite started by a run that already holds a snapshot reuses it.
 */
export function snapshotBuild() {
  const inherited = process.env[SNAPSHOT_ENV];
  if (inherited && fs.existsSync(manifestFile(inherited))) return JSON.parse(fs.readFileSync(manifestFile(inherited), 'utf8'));
  if (inherited) throw new Error(`${SNAPSHOT_ENV}=${inherited} holds no manifest.json`);
  if (!devOverridesAllowed(process.env, { bundled: true }) && process.env.NODE_ENV !== 'production') process.env.NODE_ENV = 'test';
  if (process.env.NODE_ENV === 'production') throw new Error('build snapshot: NODE_ENV=production ignores OCTOCODE_NATIVE_BIN, so the copied binary could not be used');
  // Inside the repo's node_modules: a launcher copy resolves the installed platform package the way the original does
  // (the gates where OCTOCODE_NATIVE_BIN is ignored still run the installed binary).
  const parent = path.join(ROOT, 'node_modules/.cache/octocode-e2e');
  fs.mkdirSync(parent, { recursive: true });
  const dir = fs.mkdtempSync(path.join(parent, 'build-'));
  try {
    const nativeSource = nativeBinaryPath();
    const nativeCopy = path.join(dir, 'native', path.basename(nativeSource));
    fs.mkdirSync(path.dirname(nativeCopy));
    const native = describe(nativeSource, nativeCopy, stableCopy(nativeSource, nativeCopy, 'native binary'));
    const cli = describe(CLI_SOURCE_DIR, path.join(dir, 'cli'), stableCopy(CLI_SOURCE_DIR, path.join(dir, 'cli'), 'CLI launcher'));
    const mcp = describe(MCP_SOURCE_DIR, path.join(dir, 'mcp'), stableCopy(MCP_SOURCE_DIR, path.join(dir, 'mcp'), 'MCP server'));
    const manifest = { dir, createdAt: new Date().toISOString(), native, cli, mcp };
    fs.writeFileSync(manifestFile(dir), JSON.stringify(manifest, null, 2));
    process.env[SNAPSHOT_ENV] = dir;
    process.env.OCTOCODE_NATIVE_BIN = nativeCopy;
    useBuildPaths({ cli: path.join(dir, 'cli', 'octocode.js'), mcp: path.join(dir, 'mcp', 'index.js') });
    owned = dir;
    process.once('exit', () => { fs.rmSync(dir, { recursive: true, force: true }); });
    return manifest;
  } catch (error) {
    fs.rmSync(dir, { recursive: true, force: true });
    throw error;
  }
}

/** The folder this process created and will remove at exit, if any. */
export const ownedSnapshot = () => owned;
