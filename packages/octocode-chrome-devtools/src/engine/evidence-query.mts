#!/usr/bin/env node
import {
  createReadStream,
  readFileSync,
  openSync,
  readSync,
  writeSync,
  closeSync,
  writeFileSync,
  statSync,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
} from 'node:fs';
import { createInterface } from 'node:readline';
import { arrayRanges } from './json-array-stream.mjs';
import { createHash, randomUUID } from 'node:crypto';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromeOutputLimits } from './chrome-contract.mjs';
import { validateFlags } from './cli-flags.mjs';
import { compareNumeric } from './evidence-numeric.mjs';

const args = process.argv.slice(2),
  get = (flag: string, fallback?: string) =>
    args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
const parse = text =>
  JSON.parse(text, (_key, item, context) =>
    typeof item === 'number' && context?.source
      ? JSON.rawJSON(context.source)
      : item
  );
const digest = value => createHash('sha256').update(value).digest('hex');
async function sourceDigest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
function pointer(value, path) {
  if (!path) return value;
  if (!path.startsWith('/')) throw new Error('JSON pointer must start with /');
  for (const part of path.slice(1).split('/')) {
    if (/~(?![01])/u.test(part)) throw new Error('Invalid JSON pointer escape');
    const key = part.replace(/~1/g, '/').replace(/~0/g, '~');
    if (
      value === null ||
      typeof value !== 'object' ||
      JSON.isRawJSON(value) ||
      !Object.hasOwn(value, key)
    )
      return undefined;
    value = value[key];
  }
  return value;
}
function matches(row, filters) {
  return filters.every(filter => {
    const actual = pointer(row, filter.path);
    let expected = filter.value;
    if (filter.op === 'exists') return (actual !== undefined) === expected;
    if (filter.op === 'contains')
      return typeof actual === 'string' && actual.includes(expected);
    // Integer strings are the existing exact-large-integer query form.
    if (
      typeof expected === 'string' &&
      /^-?\d+$/.test(expected) &&
      (['gte', 'lte'].includes(filter.op) ||
        (JSON.isRawJSON(actual) &&
          /^-?\d+$/.test(actual.rawJSON) &&
          (BigInt(actual.rawJSON) > BigInt(Number.MAX_SAFE_INTEGER) ||
            BigInt(actual.rawJSON) < BigInt(Number.MIN_SAFE_INTEGER))))
    )
      expected = BigInt(expected);
    const order = compareNumeric(actual, expected);
    if (filter.op === 'eq')
      return order === null ? actual === expected : order === 0;
    if (order === null) return false;
    if (filter.op === 'gte') return order >= 0;
    if (filter.op === 'lte') return order <= 0;
    return false;
  });
}

if (args.includes('--help') || args.includes('-h')) {
  console.log(
    'Usage: evidence-query.mjs --file <JSON|JSONL> [--format json|jsonl] [--pointer /rows] [--where <JSON predicates>] [--select <JSON pointer array>] [--cursor 0] [--limit 50] [--sha256 <digest>]\nFilters before pagination; builds a reusable lossless offset index. Predicates: {path:"/status",op:"eq|contains|gte|lte|exists",value:...}. Select is opt-in projection with original source pointers. Oversized rows have executable full-value continuations.'
  );
  process.exit(0);
}
let temporary;
try {
  validateFlags(args, [
    '--file',
    '--format',
    '--pointer',
    '--where',
    '--select',
    '--cursor',
    '--limit',
    '--sha256',
  ]);
  if (!get('--file')) throw new Error('--file is required');
  const file = resolve(get('--file')),
    format = get('--format', 'json'),
    path = get('--pointer', '');
  const cursor = Number(get('--cursor', '0')),
    limit = Number(get('--limit', String(chromeOutputLimits.continuationRows)));
  if (
    !['json', 'jsonl'].includes(format) ||
    !Number.isSafeInteger(cursor) ||
    cursor < 0 ||
    !Number.isSafeInteger(limit) ||
    limit < 1 ||
    limit > 300
  )
    throw new Error('Invalid format, cursor or limit');
  if (format === 'jsonl' && path)
    throw new Error('JSONL rows are the root; omit --pointer');
  const filters = parse(get('--where', '[]')),
    select = get('--select') ? JSON.parse(get('--select')) : null;
  if (
    !Array.isArray(filters) ||
    filters.some(
      f =>
        !f ||
        typeof f.path !== 'string' ||
        !['eq', 'contains', 'gte', 'lte', 'exists'].includes(f.op) ||
        !Object.hasOwn(f, 'value')
    )
  )
    throw new Error('Invalid predicates');
  for (const f of filters) {
    if (Object.keys(f).some(key => !['path', 'op', 'value'].includes(key)))
      throw new Error('Unknown predicate field');
    if (
      f.op === 'eq' &&
      f.value !== null &&
      !JSON.isRawJSON(f.value) &&
      !['string', 'number', 'boolean'].includes(typeof f.value)
    )
      throw new Error('eq needs a scalar value');
    pointer({}, f.path);
    if (f.op === 'exists' && typeof f.value !== 'boolean')
      throw new Error('exists needs a boolean');
    if (f.op === 'contains' && typeof f.value !== 'string')
      throw new Error('contains needs text');
    if (
      ['gte', 'lte'].includes(f.op) &&
      typeof f.value !== 'number' &&
      !JSON.isRawJSON(f.value) &&
      !/^-?\d+$/.test(String(f.value))
    )
      throw new Error('Numeric predicate needs a number or integer string');
  }
  if (
    select &&
    (!Array.isArray(select) ||
      !select.length ||
      select.some(p => typeof p !== 'string'))
  )
    throw new Error('Invalid projection');
  for (const p of select || []) pointer({}, p);
  const sha256 = await sourceDigest(file);
  if (get('--sha256') && get('--sha256') !== sha256)
    throw new Error('Capture changed; restart the query');
  const identity = digest(
    JSON.stringify({ version: 7, file, sha256, format, path, filters, select })
  );
  const indexDir = resolve(
      '.octocode/tmp/chrome-devtools/evidence-index',
      identity
    ),
    manifestFile = join(indexDir, 'index.json');
  let index,
    reused = existsSync(manifestFile);
  if (reused) index = JSON.parse(readFileSync(manifestFile, 'utf8'));
  else {
    mkdirSync(dirname(indexDir), { recursive: true, mode: 0o700 });
    temporary = indexDir + '-' + randomUUID();
    mkdirSync(temporary, { mode: 0o700 });
    const out = openSync(join(temporary, 'rows.jsonl'), 'wx', 0o600),
      offsets = openSync(join(temporary, 'offsets.bin'), 'wx', 0o600);
    let at = 0,
      scanned = 0,
      matched = 0;
    const add = (row, sourcePointer) => {
      const sourceIndex = scanned++;
      if (!matches(row, filters)) return;
      const value = select
        ? Object.fromEntries(select.map(p => [p, pointer(row, p) ?? null]))
        : row;
      const record = { sourceIndex, sourcePointer, value },
        encoded = Buffer.from(JSON.stringify(record) + '\n');
      let done = 0;
      while (done < encoded.length)
        done += writeSync(out, encoded, done, encoded.length - done);
      const entry = Buffer.alloc(64);
      entry.writeDoubleLE(at, 0);
      entry.writeDoubleLE(encoded.length, 8);
      entry.writeDoubleLE(sourceIndex, 16);
      entry.writeDoubleLE(
        format === 'jsonl' ? Number(sourcePointer.slice(5)) : sourceIndex,
        24
      );
      Buffer.from(digest(encoded), 'hex').copy(entry, 32);
      for (let done = 0; done < entry.length;)
        done += writeSync(offsets, entry, done, entry.length - done);
      matched++;
      at += encoded.length;
    };
    try {
      if (format === 'jsonl') {
        let line = 0;
        const lines = createInterface({
          input: createReadStream(file),
          crlfDelay: Infinity,
        });
        for await (const text of lines) {
          line++;
          if (text.trim()) add(parse(text), 'line:' + line);
        }
      } else {
        const input = openSync(file, 'r');
        try {
          for await (const range of arrayRanges(file, path)) {
            const raw = Buffer.alloc(range.length);
            let done = 0;
            while (done < raw.length) {
              const n = readSync(
                input,
                raw,
                done,
                raw.length - done,
                range.offset + done
              );
              if (!n) throw new Error('Incomplete source row');
              done += n;
            }
            add(parse(raw.toString('utf8')), path + '/' + range.index);
          }
        } finally {
          closeSync(input);
        }
      }
    } finally {
      closeSync(out);
      closeSync(offsets);
    }
    if ((await sourceDigest(file)) !== sha256)
      throw new Error('Capture changed while indexing');
    index = {
      file,
      sha256,
      format,
      pointer: path,
      filters,
      select,
      scanned,
      matched,
      offsetsSha256: await sourceDigest(join(temporary, 'offsets.bin')),
      rowsBytes: at,
    };
    writeFileSync(join(temporary, 'index.json'), JSON.stringify(index), {
      mode: 0o600,
    });
    try {
      renameSync(temporary, indexDir);
      temporary = null;
    } catch (error) {
      if (!existsSync(manifestFile)) throw error;
      rmSync(temporary, { recursive: true, force: true });
      temporary = null;
      index = JSON.parse(readFileSync(manifestFile, 'utf8'));
      reused = true;
    }
  }
  if (cursor > index.matched) throw new Error('Cursor exceeds matched rows');
  const rowsFile = join(indexDir, 'rows.jsonl');
  // The trusted offset table pins each row digest. Verify selected rows, not the
  // entire copied evidence on every page; source and offsets remain digest-pinned.
  if (statSync(rowsFile).size !== index.rowsBytes)
    throw new Error('Evidence index changed; rebuild it');
  const offsetsFile = join(indexDir, 'offsets.bin');
  if ((await sourceDigest(offsetsFile)) !== index.offsetsSha256)
    throw new Error('Evidence offset index changed; rebuild it');
  const fd = openSync(rowsFile, 'r'),
    offsets = openSync(offsetsFile, 'r'),
    rows = [];
  let nextCursor = cursor,
    bytes = 0;
  try {
    while (nextCursor < index.matched && rows.length < limit) {
      const encodedEntry = Buffer.alloc(64);
      if (readSync(offsets, encodedEntry, 0, 64, nextCursor * 64) !== 64)
        throw new Error('Incomplete offset index');
      const sourceIndex = encodedEntry.readDoubleLE(16),
        sourceLine = encodedEntry.readDoubleLE(24);
      const entry = {
        offset: encodedEntry.readDoubleLE(0),
        length: encodedEntry.readDoubleLE(8),
        sourceIndex,
        sourcePointer:
          format === 'jsonl' ? 'line:' + sourceLine : path + '/' + sourceIndex,
        sha256: encodedEntry.subarray(32).toString('hex'),
      };
      let record;
      if (entry.length > 16000) {
        const oversized = join(indexDir, `row-${nextCursor}.json`);
        if (!existsSync(oversized)) {
          const destination = openSync(oversized, 'wx', 0o600),
            buffer = Buffer.alloc(Math.min(entry.length, 1024 * 1024));
          try {
            for (let at = 0; at < entry.length;) {
              const n = readSync(
                fd,
                buffer,
                0,
                Math.min(buffer.length, entry.length - at),
                entry.offset + at
              );
              if (!n) throw new Error('Incomplete evidence index');
              let written = 0;
              while (written < n)
                written += writeSync(destination, buffer, written, n - written);
              at += n;
            }
          } finally {
            closeSync(destination);
          }
        }
        if ((await sourceDigest(oversized)) !== entry.sha256)
          throw new Error('Oversized evidence row changed; rebuild the index');
        record = {
          sourceIndex: entry.sourceIndex,
          sourcePointer: entry.sourcePointer,
          oversized: true,
          sourceBytes: entry.length,
          next: {
            continue: {
              command: process.execPath,
              args: [
                join(
                  dirname(fileURLToPath(import.meta.url)),
                  'artifact-query.mjs'
                ),
                '--file',
                oversized,
                '--format',
                'json',
                '--sha256',
                entry.sha256,
              ],
            },
          },
        };
      } else {
        const raw = Buffer.alloc(entry.length);
        let done = 0;
        while (done < raw.length) {
          const n = readSync(
            fd,
            raw,
            done,
            raw.length - done,
            entry.offset + done
          );
          if (!n) throw new Error('Incomplete evidence index');
          done += n;
        }
        if (digest(raw) !== entry.sha256)
          throw new Error('Evidence index row changed; rebuild it');
        record = parse(raw.toString('utf8'));
      }
      const size = Buffer.byteLength(JSON.stringify(record));
      if (rows.length && bytes + size > 18000) break;
      rows.push(record);
      bytes += size;
      nextCursor++;
    }
  } finally {
    closeSync(fd);
    closeSync(offsets);
  }
  const nextArgs = [
    fileURLToPath(import.meta.url),
    '--file',
    file,
    '--format',
    format,
    '--cursor',
    String(nextCursor),
    '--limit',
    String(limit),
    '--sha256',
    sha256,
    ...(path ? ['--pointer', path] : []),
    '--where',
    JSON.stringify(filters),
    ...(select ? ['--select', JSON.stringify(select)] : []),
  ];
  console.log(
    JSON.stringify({
      file,
      sha256,
      index: manifestFile,
      indexReused: reused,
      scanned: index.scanned,
      matched: index.matched,
      cursor,
      returned: rows.length,
      rows,
      ...(nextCursor < index.matched
        ? { next: { continue: { command: process.execPath, args: nextArgs } } }
        : {}),
    })
  );
} catch (error) {
  console.error('[EVIDENCE_QUERY] ' + error.message);
  process.exitCode = 1;
} finally {
  if (temporary) rmSync(temporary, { recursive: true, force: true });
}
