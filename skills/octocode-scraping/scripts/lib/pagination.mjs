import { createReadStream, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fail } from './fail.mjs';

const flagValue = (args, flag, fallback) => args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;

// One hash over every file the answer reads, so a continuation can tell the corpus changed underneath it.
async function corpusSnapshot(files) {
  const hash = createHash('sha256');
  for (const file of files) { hash.update(file); if (existsSync(file)) for await (const chunk of createReadStream(file)) hash.update(chunk); }
  return hash.digest('hex');
}

// A row too large to page inline is cached under indexes/query-values and replaced by a pointer to it.
function oversizedPointer(dir, at, encoded, bytes) {
  const cache = join(dir, 'indexes', 'query-values'); mkdirSync(cache, { recursive: true });
  const rowSnapshot = createHash('sha256').update(encoded).digest('hex');
  const source = join(cache, rowSnapshot + '.json');
  if (!existsSync(source)) writeFileSync(source, encoded, { mode: 0o600 });
  return { sourceIndex: at, oversized: true, bytes, next: { continue: { command: process.execPath, args: [fileURLToPath(new URL('../source-query.mjs', import.meta.url)), '--file', source, '--snapshot', rowSnapshot] } } };
}

// Rows from `at` until the limit or the shared byte budget runs out.
function pageRows(rows, at, { limit, dir, budget }) {
  const page = [];
  while (at < rows.length && page.length < limit) {
    let row = rows[at];
    const encoded = JSON.stringify(row);
    let bytes = Buffer.byteLength(encoded);
    if (bytes > 14000) { row = oversizedPointer(dir, at, encoded, bytes); bytes = Buffer.byteLength(JSON.stringify(row)); }
    if (bytes > budget) break;
    page.push(row); budget -= bytes; at++;
  }
  return { page, at, budget };
}

function continuation(args, script, { field, at, snapshot }) {
  const owned = ['--snapshot', '--view', '--cursor-' + field];
  const filtered = args.filter((_, i) => !owned.includes(args[i]) && !owned.includes(args[i - 1]));
  return { command: process.execPath, args: [script, ...filtered, '--view', field, '--cursor-' + field, String(at), '--snapshot', snapshot] };
}

function pageOptions(args, lists, defaultLimit) {
  const limit = Number(flagValue(args, '--limit', defaultLimit)), view = flagValue(args, '--view');
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 300 || view && !Object.hasOwn(lists, view)) throw new Error('Invalid limit or view');
  return { limit, view };
}

async function paginateLists({ lists, files, dir, args, script, defaultLimit }) {
  const { limit, view } = pageOptions(args, lists, defaultLimit);
  const snapshot = await corpusSnapshot(files);
  if (flagValue(args, '--snapshot') && flagValue(args, '--snapshot') !== snapshot) throw new Error('Corpus changed; restart the query');
  const next = {}, output = {}, pagination = {}; let budget = 16000;
  for (const [field, rows] of Object.entries(lists)) {
    output[field] = []; if (view && field !== view) continue;
    const cursor = Number(flagValue(args, '--cursor-' + field, 0)); if (!Number.isSafeInteger(cursor) || cursor < 0 || cursor > rows.length) throw new Error('Invalid cursor');
    const paged = pageRows(rows, cursor, { limit, dir, budget });
    output[field] = paged.page; budget = paged.budget;
    pagination[field] = { cursor, returned: output[field].length, total: rows.length };
    if (paged.at < rows.length) next[field] = continuation(args, script, { field, at: paged.at, snapshot });
  }
  return { ...output, pagination, snapshot, ...(Object.keys(next).length ? { next } : {}) };
}

export async function paginate({ lists, files, dir, args, script, defaultLimit = 20 }) {
  let result;
  try { result = await paginateLists({ lists, files, dir, args, script, defaultLimit }); }
  catch (error) { fail(/changed/i.test(error.message) ? 'sourceChanged' : 'invalidArguments', error.message, 2); }
  return result;
}
