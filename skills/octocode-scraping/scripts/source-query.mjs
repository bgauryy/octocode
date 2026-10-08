#!/usr/bin/env node
import { createReadStream, openSync, readSync, closeSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const args = process.argv.slice(2), get = (key, fallback) => args.includes(key) ? args[args.indexOf(key) + 1] : fallback;
if (args.includes('--help')) { console.log('Usage: source-query.mjs --file <path> [--offset <byte>] [--length 6000] [--snapshot <sha256>]\nLossless source bytes as independently decodable base64 pages; follows every byte, including JSON/text/binary.'); process.exit(0); }
try {
  if (!get('--file')) throw new Error('--file is required');
  const file = resolve(get('--file')), offset = Number(get('--offset', 0)), length = Number(get('--length', 6000));
  if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 || length > 12000) throw new Error('Invalid offset or length');
  const hash = createHash('sha256'); for await (const chunk of createReadStream(file)) hash.update(chunk); const snapshot = hash.digest('hex');
  if (get('--snapshot') && get('--snapshot') !== snapshot) throw new Error('Source changed; restart paging');
  const total = statSync(file).size; if (offset > total) throw new Error('Offset exceeds source');
  const fd = openSync(file, 'r'), bytes = Buffer.alloc(Math.min(length, total - offset));
  try { let at = 0; while (at < bytes.length) { const n = readSync(fd, bytes, at, bytes.length - at, offset + at); if (!n) throw new Error('Incomplete source'); at += n; } } finally { closeSync(fd); }
  const end = offset + bytes.length;
  console.log(JSON.stringify({ file, snapshot, totalBytes: total, offset, bytes: bytes.length, encoding: 'base64', content: bytes.toString('base64'), ...(end < total ? { next: { continue: { command: process.execPath, args: [fileURLToPath(import.meta.url), '--file', file, '--offset', String(end), '--length', String(length), '--snapshot', snapshot] } } } : {}) }));
} catch (error) { console.error(JSON.stringify({ ok: false, error: error.message })); process.exitCode = 2; }
