#!/usr/bin/env node
// Merges generated host hook event arrays into an existing hook or settings file without touching unrelated settings.

import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Merge generated hook event arrays into a host hook or settings file.

Usage:
  node scripts/hook-merge.mjs --file <settings.json> --add <hooks.json | -> [--replace <regex>] [--dry-run]

Inputs:
  --file     The host file to update (for example .claude/settings.local.json, .codex/hooks.json,
             .cursor/hooks.json). A missing file starts empty; its folder is created on write.
  --add      Generated hooks {"hooks":{"<Event>":[...]}, "version"?}, such as host-config --json output;
             "-" reads standard input, so no preview file lands in a hook folder.
  --replace  Before merging, remove existing handlers whose "command" matches this regex: this installation's
             obsolete handlers. Nested groups {hooks:[...]} left empty, and events left empty, are removed.
  --dry-run  Print the merged file as "settings" and write nothing.

Merge: every key other than hooks is kept as is; each added entry is appended to its event array unless an
identical entry is already there, so rerunning the same merge changes nothing. A "version" in --add is set
when the file has none and must equal the file's when it has one.

Output (stdout JSON): {ok, file, written, dryRun, added, skipped, removed, events}; settings on --dry-run.
The file is rewritten atomically (temp file + rename) only when the merge changes it.
Exit codes: 0 merged or already current; 1 version conflict or write failure (file unchanged);
2 invalid input (malformed JSON, hooks that are not event arrays, bad regex, missing option).`;

const isObject = (value) => Boolean(value) && typeof value === 'object' && !Array.isArray(value);

function deepEqual(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((item, i) => deepEqual(item, b[i]));
  }
  if (!isObject(a) || !isObject(b)) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((key) => Object.hasOwn(b, key) && deepEqual(a[key], b[key]));
}

const invalid = (message) => Object.assign(new Error(message), { code: 'invalidInput', exitCode: 2 });

function checkHooks(hooks, where) {
  if (!isObject(hooks) || !Object.values(hooks).every(Array.isArray)) throw invalid(`${where}: hooks must map each event to an array`);
}

/** Merges add into settings; returns {settings, added, skipped, removed, events} without mutating its inputs. */
export function mergeHooks(settings, add, replace) {
  if (!isObject(settings)) throw invalid('the hook file must hold a JSON object');
  if (!isObject(add)) throw invalid('--add must hold a JSON object');
  checkHooks(add.hooks, '--add');
  if (settings.hooks !== undefined) checkHooks(settings.hooks, '--file');
  const result = structuredClone(settings);
  if (add.version !== undefined) {
    if (result.version === undefined) result.version = add.version;
    else if (!deepEqual(result.version, add.version)) {
      throw Object.assign(new Error(`file version ${JSON.stringify(result.version)} differs from --add version ${JSON.stringify(add.version)}`), { code: 'versionConflict', exitCode: 1 });
    }
  }
  result.hooks ??= {};
  let removed = 0;
  const events = new Set();
  if (replace) {
    const obsolete = (handler) => isObject(handler) && typeof handler.command === 'string' && replace.test(handler.command);
    for (const [event, entries] of Object.entries(result.hooks)) {
      const kept = entries.flatMap((entry) => {
        if (isObject(entry) && Array.isArray(entry.hooks)) {
          const handlers = entry.hooks.filter((handler) => !obsolete(handler));
          removed += entry.hooks.length - handlers.length;
          if (handlers.length === entry.hooks.length) return [entry];
          return handlers.length ? [{ ...entry, hooks: handlers }] : [];
        }
        if (obsolete(entry)) { removed++; return []; }
        return [entry];
      });
      if (kept.length === entries.length && kept.every((entry, i) => entry === entries[i])) continue;
      events.add(event);
      if (kept.length) result.hooks[event] = kept;
      else delete result.hooks[event];
    }
  }
  let added = 0;
  let skipped = 0;
  for (const [event, entries] of Object.entries(add.hooks)) {
    const target = (result.hooks[event] ??= []);
    for (const entry of entries) {
      if (target.some((existing) => deepEqual(existing, entry))) { skipped++; continue; }
      target.push(structuredClone(entry));
      added++;
      events.add(event);
    }
  }
  return { settings: result, added, skipped, removed, events: [...events] };
}

function readJson(text, where) {
  try { return JSON.parse(text); }
  catch (error) { throw invalid(`${where}: ${errorMessage(error)}`); }
}

const indentOf = (text) => /\n([ \t]+)\S/.exec(text)?.[1] ?? '  ';

function main(argv) {
  if (!argv.length || argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    if (!argv.length) process.exitCode = 2;
    return;
  }
  const options = { dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--dry-run') options.dryRun = true;
    else if (['--file', '--add', '--replace'].includes(arg)) {
      const value = argv[++i];
      if (value === undefined || (value.startsWith('--') && value !== '-')) fail('invalidArguments', `${arg} requires a value`, 2);
      options[arg.slice(2)] = value;
    } else fail('invalidArguments', `Unknown argument: ${arg} (see --help)`, 2);
  }
  if (!options.file || !options.add) fail('invalidArguments', 'give --file and --add', 2);
  let replace;
  try { replace = options.replace === undefined ? undefined : new RegExp(options.replace); }
  catch (error) { fail('invalidArguments', `--replace: ${errorMessage(error)}`, 2); }
  let merged;
  let original = '';
  let settings = {};
  const existed = existsSync(options.file);
  try {
    original = existed ? readFileSync(options.file, 'utf8') : '';
    settings = original.trim() ? readJson(original, options.file) : {};
    const add = readJson(readFileSync(options.add === '-' ? 0 : options.add, 'utf8'), options.add === '-' ? 'stdin' : options.add);
    merged = mergeHooks(settings, add, replace);
  } catch (error) {
    fail(error.code ?? 'invalidInput', errorMessage(error), error.exitCode ?? 2);
  }
  const text = `${JSON.stringify(merged.settings, null, indentOf(original))}\n`;
  const changed = !existed || !deepEqual(merged.settings, settings);
  const report = { ok: true, file: options.file, written: false, dryRun: options.dryRun, added: merged.added, skipped: merged.skipped, removed: merged.removed, events: merged.events };
  if (options.dryRun) {
    console.log(JSON.stringify({ ...report, settings: merged.settings }, null, 1));
    return;
  }
  if (changed) {
    const temp = `${options.file}.${process.pid}.tmp`;
    try {
      mkdirSync(dirname(options.file), { recursive: true });
      writeFileSync(temp, text);
      renameSync(temp, options.file);
    } catch (error) {
      rmSync(temp, { force: true });
      fail('writeFailed', errorMessage(error), 1);
    }
    report.written = true;
  }
  console.log(JSON.stringify(report, null, 1));
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
