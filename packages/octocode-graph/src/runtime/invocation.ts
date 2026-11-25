// How a person or agent runs this package's CLI here. Every hint, usage line and error that names an
// `octocode-graph …` command takes its first words from graphCommand: the bare `octocode-graph` when the first
// executable of that name on PATH runs this package, else `npx -y @octocodeai/octocode-graph`, which needs no install.
// A `node_modules/.bin` on PATH is a package runner's (npm run, npx, pnpm exec), added for this process only: the
// shell that reads the hint does not have it, so it never counts.
import { existsSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, delimiter, dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

export const PACKAGE = '@octocodeai/octocode-graph';
const NAME = 'octocode-graph';
/** The no-install invocation: npx runs the installed copy of the project, else fetches the package once. */
export const NPX = `npx -y ${PACKAGE}`;

// This package's directory: the nearest package.json above this module that names it (src/runtime in a checkout,
// dist/runtime or dist/shared once bundled).
function packageDir() {
  for (
    let dir = dirname(fileURLToPath(import.meta.url));
    dir !== dirname(dir);
    dir = dirname(dir)
  ) {
    const manifest = join(dir, 'package.json');
    if (!existsSync(manifest)) continue;
    try {
      if (
        (JSON.parse(readFileSync(manifest, 'utf8')) as { name?: unknown })
          .name === PACKAGE
      )
        return dir;
    } catch {
      // An unreadable manifest names no package: keep climbing.
    }
  }
  throw new Error(`no ${PACKAGE} package.json above ${import.meta.url}`);
}

/** This package's version, from its package.json: what the MCP server reports as its own. */
export function packageVersion() {
  const manifest = join(packageDir(), 'package.json');
  const { version } = JSON.parse(readFileSync(manifest, 'utf8')) as {
    version?: unknown;
  };
  if (typeof version !== 'string')
    throw new Error(`${manifest} names no version`);
  return version;
}

/** The CLI file an `octocode-graph` on PATH must run for it to be this package. */
export const packageBin = () => join(packageDir(), 'bin', `${NAME}.mjs`);

const real = (path: string) => {
  try {
    return realpathSync(path);
  } catch {
    return null;
  }
};

// Package-manager shims (npm and pnpm on POSIX, `.cmd` and `.ps1` on Windows) name the CLI they run in quotes,
// relative to their own directory ($basedir, %dp0%, $PSScriptRoot) or absolute.
const SHIM_TARGET =
  /"(\$basedir|%~?dp0%?|\$PSScriptRoot)?([^"]*?bin[\\/]octocode-graph\.mjs)"/g;
function shimTargets(file: string) {
  if ((statSync(file, { throwIfNoEntry: false })?.size ?? Infinity) > 65536)
    return [];
  let text: string;
  try {
    text = readFileSync(file, 'utf8');
  } catch {
    return [];
  }
  return [...text.matchAll(SHIM_TARGET)].map(([, base, path]) => {
    const local = path.replace(/[\\/]+/g, sep);
    return base ? resolve(dirname(file), `.${sep}${local}`) : resolve(local);
  });
}

// The file a shell runs for `octocode-graph`: the first match on PATH, with Windows' executable extensions.
function onPath(env: NodeJS.ProcessEnv) {
  const names =
    process.platform === 'win32'
      ? (env.PATHEXT ?? '.COM;.EXE;.BAT;.CMD')
          .split(';')
          .filter(Boolean)
          .map(ext => `${NAME}${ext.toLowerCase()}`)
      : [NAME];
  for (const dir of (env.PATH ?? env.Path ?? '').split(delimiter)) {
    if (!dir) continue;
    if (basename(dir) === '.bin' && basename(dirname(dir)) === 'node_modules')
      continue;
    for (const name of names) {
      const file = join(dir, name);
      if (statSync(file, { throwIfNoEntry: false })?.isFile()) return file;
    }
  }
  return null;
}

/** The words that run this package's CLI here: `octocode-graph` when PATH runs this package with it, else npx. */
export function graphCommand(env: NodeJS.ProcessEnv = process.env): string {
  const file = onPath(env);
  if (!file) return NPX;
  const bin = real(packageBin());
  const runs = [file, ...shimTargets(file)].some(p => real(p) === bin);
  return bin && runs ? NAME : NPX;
}

/** A command line of this package's CLI: `<graphCommand> <args>`. */
export const graphCli = (args: string, env?: NodeJS.ProcessEnv) =>
  `${graphCommand(env)} ${args}`;

/** One argument as a shell reads it: as is when no shell would split or expand it, else quoted. */
export function shellArg(value: string) {
  if (/^[\w./:@%+=,-]+$/.test(value)) return value;
  return process.platform === 'win32'
    ? `"${value.replaceAll('"', '""')}"`
    : `'${value.replaceAll("'", "'\\''")}'`;
}
