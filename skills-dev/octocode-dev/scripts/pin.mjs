#!/usr/bin/env node
// The round's pin (`dev.mjs pin`): a frozen copy of the built native binary that every retest runs, never the
// moving tree. Copies the host's staged binary into .octocode/octocode-dev/pins/<version>-<sha12>/, prints its
// path, sha256 and version, and keeps only the current and previous pin.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { ROOT } from './lib/workspaces.mjs';

const require = createRequire(import.meta.url);
const NATIVE = path.join(ROOT, 'packages', 'octocode-native');
const { executableName, getPlatformSuffix } = require(path.join(NATIVE, 'bin', 'platform.cjs'));
const { SMOKE_TIMEOUT_MS } = require(path.join(NATIVE, 'scripts', 'stage-binary.cjs'));

export const PINS = path.join(ROOT, '.octocode', 'octocode-dev', 'pins');
/** The current pin and the one before it: a retest of the previous round still runs. */
export const KEEP = 2;
const RECORD = 'pin.json';

const HELP = `Usage: node skills-dev/octocode-dev/scripts/pin.mjs [--from <binary>] [--dir <pins dir>]

Freezes the built native binary as the round's pin: copies it to <dir>/<version>-<sha12>/octocode (default dir
.octocode/octocode-dev/pins/ under the repo, gitignored), runs the copy's --version, and prints
{ok, path, sha256, version, source, reused, previous, removed}. Retests run "path" (the launchers take it as
NODE_ENV=development OCTOCODE_NATIVE_BIN=<path>). Pinning bytes already pinned reuses that pin and makes it current.
Keeps the current and previous pin and deletes older ones (listed in "removed").

  --from <binary>   pin this binary instead of the host's staged one (packages/octocode-native/npm/<host>/octocode,
                    which \`dev build:dev\` writes)
  --dir <dir>       the pins folder

Errors print {ok:false,error:{code,message}}: noBinary (nothing built at the source; run \`dev build:dev\`),
binaryFailed (the copy does not run --version), invalidArguments.
Exit codes: 0 pinned; 1 no binary or the copy failed; 2 invalid arguments.`;

class PinError extends Error {
  constructor(code, message, exitCode = 1) {
    super(message);
    this.code = code;
    this.exitCode = exitCode;
  }
}

/** The binary `dev build:dev` stages for this host. */
export function stagedBinary() {
  const suffix = getPlatformSuffix();
  if (!suffix) throw new PinError('noBinary', `no native platform package for ${process.platform}-${process.arch}`);
  return path.join(NATIVE, 'npm', suffix, executableName('octocode'));
}

function sha256(file) {
  const hash = createHash('sha256');
  const fd = fs.openSync(file, 'r');
  try {
    const buffer = Buffer.allocUnsafe(1 << 20);
    for (let read; (read = fs.readSync(fd, buffer, 0, buffer.length, null)) > 0;) hash.update(buffer.subarray(0, read));
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest('hex');
}

/** `octocode 20.2.0+54a1f44b` → `20.2.0+54a1f44b`; the copy must run on this machine. */
function versionOf(binary) {
  const ran = spawnSync(binary, ['--version'], { encoding: 'utf8', timeout: SMOKE_TIMEOUT_MS });
  const version = /^octocode (\S+)\s*$/.exec(ran.stdout ?? '')?.[1];
  if (ran.status !== 0 || !version) {
    const why = ran.error?.message ?? (`${ran.stderr ?? ''}${ran.stdout ?? ''}`.trim() || `exit ${ran.status}`);
    throw new PinError('binaryFailed', `${binary} --version failed: ${why}`);
  }
  return version;
}

/** Every pin in `dir` with its record, newest first. */
export function listPins(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    if (!entry.isDirectory()) return [];
    try {
      const record = JSON.parse(fs.readFileSync(path.join(dir, entry.name, RECORD), 'utf8'));
      return [{ ...record, dir: path.join(dir, entry.name) }];
    } catch {
      return [];
    }
  }).toSorted((a, b) => b.pinnedAt.localeCompare(a.pinnedAt) || b.dir.localeCompare(a.dir));
}

const summary = pin => pin && { path: pin.path, sha256: pin.sha256, version: pin.version };

/** Pins `source` into `dir`; `now` orders pins (an ISO time). */
export function pin({ source = stagedBinary(), dir = PINS, now = new Date().toISOString() } = {}) {
  if (!fs.statSync(source, { throwIfNoEntry: false })?.isFile()) {
    throw new PinError('noBinary', `no built binary at ${source}; run \`dev build:dev\` first`);
  }
  const before = listPins(dir);
  const digest = sha256(source);
  const existing = before.find(item => item.sha256 === digest && fs.existsSync(item.path));
  let record;
  if (existing) {
    record = { ...existing, pinnedAt: now };
    delete record.dir;
    fs.writeFileSync(path.join(existing.dir, RECORD), `${JSON.stringify(record, null, 2)}\n`);
  } else {
    const staging = path.join(dir, `.staging-${process.pid}-${Date.now()}`);
    fs.mkdirSync(staging, { recursive: true });
    try {
      const copy = path.join(staging, path.basename(source));
      fs.copyFileSync(source, copy);
      if (process.platform !== 'win32') fs.chmodSync(copy, 0o755);
      if (sha256(copy) !== digest) throw new PinError('binaryFailed', `${source} changed while it was copied; build again or wait for the build`);
      const version = versionOf(copy);
      const name = `${version.replace(/[^\w.+-]/g, '_')}-${digest.slice(0, 12)}`;
      const final = path.join(dir, name);
      record = { path: path.join(final, path.basename(source)), sha256: digest, version, source, pinnedAt: now };
      fs.writeFileSync(path.join(staging, RECORD), `${JSON.stringify(record, null, 2)}\n`);
      fs.rmSync(final, { recursive: true, force: true });
      fs.renameSync(staging, final);
    } finally {
      fs.rmSync(staging, { recursive: true, force: true });
    }
  }
  const pins = listPins(dir);
  const removed = pins.slice(KEEP).map(item => {
    fs.rmSync(item.dir, { recursive: true, force: true });
    return item.dir;
  });
  const previous = pins.find(item => item.sha256 !== digest);
  return { ok: true, ...summary(record), source, reused: Boolean(existing), previous: summary(previous) ?? null, removed };
}

function parse(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if ((arg === '--from' || arg === '--dir') && argv[index + 1] && !argv[index + 1].startsWith('--')) {
      options[arg.slice(2)] = path.resolve(argv[++index]);
    } else {
      throw new PinError('invalidArguments', `unexpected argument ${arg} (see --help)`, 2);
    }
  }
  return { source: options.from, dir: options.dir };
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    return 0;
  }
  try {
    const { source, dir } = parse(argv);
    console.log(JSON.stringify(pin({ ...(source && { source }), ...(dir && { dir }) }), null, 2));
    return 0;
  } catch (error) {
    const code = error instanceof PinError ? error.code : 'pinFailed';
    console.log(JSON.stringify({ ok: false, error: { code, message: error.message } }));
    return error instanceof PinError ? error.exitCode : 1;
  }
}

// exitCode, not exit(): a piped stdout flushes before the process ends.
if (isMainModule(import.meta.url)) process.exitCode = main(process.argv.slice(2));
