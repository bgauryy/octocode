import { basename, dirname, resolve } from 'node:path';
import { realpathSync, statSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { defaults } from '../pipeline.mjs';
import { freshOut } from './run-output.mjs';

const string = { type: 'string' };
const boolean = { type: 'boolean' };
const common = { out: string, jobs: string, 'ingest-jobs': string, lsp: string, exclude: string, whole: boolean };

// Evidence keys stay relative to a directory, including when only one file was requested.
export function graphInput(path) {
  const input = resolve(path);
  const stat = statSync(input);
  if (stat.isDirectory()) return { root: input };
  if (!stat.isFile()) throw new Error('Expected a file or directory');
  const file = realpathSync(input);
  return { root: dirname(file), file: basename(file) };
}

export function runOptions(argv, viewer = false) {
  const options = viewer
    ? { ...common, replay: string, port: string, 'no-open': boolean, 'exit-when-done': boolean }
    : { ...common, progress: boolean };
  const { values, positionals, tokens } = parseArgs({ args: argv, options, allowPositionals: true, tokens: true });
  const seen = new Set();
  for (const token of tokens) {
    if (token.kind !== 'option') continue;
    if (seen.has(token.name)) throw new Error(`Repeated option --${token.name}`);
    seen.add(token.name);
  }
  for (const name of ['replay', 'out']) if (values[name] === '') throw new Error(`--${name} needs a directory`);
  if (values.replay && (positionals.length || values.out)) throw new Error('--replay takes a saved directory and cannot be combined with a project root or --out');
  if (!values.replay && (positionals.length !== 1 || !positionals[0])) throw new Error('Expected one project root');
  const opts = defaults();
  for (const [name, key] of [['jobs', 'jobs'], ['ingest-jobs', 'ingestJobs']]) {
    if (values[name] === undefined) continue;
    const n = Number(values[name]);
    if (!Number.isSafeInteger(n) || n < 1) throw new Error(`--${name} requires a positive integer`);
    opts[key] = n;
  }
  if (values.lsp !== undefined && !['on', 'off'].includes(values.lsp)) throw new Error('--lsp must be on or off');
  const port = Number(values.port ?? 0);
  if (values.port !== undefined && (!/^\d+$/.test(values.port) || port > 65535)) throw new Error('--port requires an integer from 0 to 65535');
  opts.whole = Boolean(values.whole);
  opts.lsp = values.lsp ?? opts.lsp;
  opts.exclude = (values.exclude ?? '').split(',').filter(Boolean);
  const input = values.replay ? null : resolve(positionals[0]);
  const { root, file } = input ? graphInput(input) : { root: null };
  if (file) opts.file = file;
  const out = values.replay || values.out;
  return { root, out: out ? resolve(out) : freshOut(input), opts, replay: Boolean(values.replay), port,
    progress: Boolean(values.progress), open: !values['no-open'], exitWhenDone: Boolean(values['exit-when-done']) };
}
