import { basename, dirname, resolve } from 'node:path';
import { realpathSync, statSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { defaults } from '../pipeline.ts';
import { freshOut } from './run-output.ts';
import type { LspMode } from '../types.ts';

const string = { type: 'string' } as const;
const boolean = { type: 'boolean' } as const;
const common = {
  out: string,
  jobs: string,
  'ingest-jobs': string,
  lsp: string,
  exclude: string,
  whole: boolean,
  'no-cache': boolean,
  'explain-cache': boolean,
};

export const lspModes = ['smart', 'all', 'off'];
// A validator: any value but a mode name (absent: the default) is refused.
export function lspMode(value: unknown = defaults().lsp): LspMode {
  if (typeof value !== 'string' || !lspModes.includes(value))
    throw new Error('--lsp must be smart, all or off');
  return value as LspMode;
}

// Evidence keys stay relative to a directory, including when only one file was requested.
export function graphInput(path: string): { root: string; file?: string } {
  const input = resolve(path);
  const stat = statSync(input);
  if (stat.isDirectory()) return { root: input };
  if (!stat.isFile()) throw new Error('Expected a file or directory');
  const file = realpathSync(input);
  return { root: dirname(file), file: basename(file) };
}

/** The flags of a build or a viewer run, as parseArgs reads them. */
interface RunFlags {
  out?: string;
  jobs?: string;
  'ingest-jobs'?: string;
  lsp?: string;
  exclude?: string;
  whole?: boolean;
  replay?: string;
  port?: string;
  'no-open'?: boolean;
  'exit-when-done'?: boolean;
  progress?: boolean;
  'no-cache'?: boolean;
  'explain-cache'?: boolean;
}

export function runOptions(argv: string[], viewer = false) {
  const options = viewer
    ? {
        ...common,
        replay: string,
        port: string,
        'no-open': boolean,
        'exit-when-done': boolean,
      }
    : { ...common, progress: boolean };
  const {
    values: parsed,
    positionals,
    tokens,
  } = parseArgs({ args: argv, options, allowPositionals: true, tokens: true });
  // parseArgs (strict) has checked each flag against its declared type; the two option sets type as a union.
  const values = parsed as RunFlags;
  const seen = new Set<string>();
  for (const token of tokens) {
    if (token.kind !== 'option') continue;
    if (seen.has(token.name))
      throw new Error(`Repeated option --${token.name}`);
    seen.add(token.name);
  }
  for (const name of ['replay', 'out'] as const)
    if (values[name] === '') throw new Error(`--${name} needs a directory`);
  if (values.replay && (positionals.length || values.out))
    throw new Error(
      '--replay takes a saved directory and cannot be combined with a project root or --out'
    );
  if (!values.replay && (positionals.length !== 1 || !positionals[0]))
    throw new Error('Expected one project root');
  const opts = defaults();
  for (const [name, key] of [
    ['jobs', 'jobs'],
    ['ingest-jobs', 'ingestJobs'],
  ] as const) {
    if (values[name] === undefined) continue;
    const n = Number(values[name]);
    if (!Number.isSafeInteger(n) || n < 1)
      throw new Error(`--${name} requires a positive integer`);
    opts[key] = n;
  }
  const port = Number(values.port ?? 0);
  if (values.port !== undefined && (!/^\d+$/.test(values.port) || port > 65535))
    throw new Error('--port requires an integer from 0 to 65535');
  opts.whole = Boolean(values.whole);
  opts.lsp = lspMode(values.lsp);
  opts.exclude = (values.exclude ?? '').split(',').filter(Boolean);
  const input = values.replay ? null : resolve(positionals[0]);
  const { root, file } = input
    ? graphInput(input)
    : { root: null, file: undefined };
  if (file) opts.file = file;
  const out = values.replay || values.out;
  // Without --replay the project root is set; with it, --replay names the output directory.
  return {
    root,
    out: out ? resolve(out) : freshOut(input!),
    opts,
    replay: Boolean(values.replay),
    port,
    progress: Boolean(values.progress),
    /** The shared graph cache (graph-cache.ts): `--no-cache` bypasses it, `--explain-cache` prints each lookup. */
    cache: {
      enabled: !values['no-cache'],
      explain: Boolean(values['explain-cache']),
    },
    open: !values['no-open'],
    exitWhenDone: Boolean(values['exit-when-done']),
  };
}
