#!/usr/bin/env node
import { createReadStream, openSync, readSync, closeSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fail } from './lib/fail.mjs';

const HELP = `Usage: source-query.mjs --file <path> [--offset <byte>] [--length 6000] [--encoding auto|utf8|base64] [--snapshot <sha256>]
Pages every byte of one source file. Prints JSON: file, snapshot (sha256 of the whole file), totalBytes, offset, bytes,
encoding, content, and next.continue {command,args} while bytes remain; run it unchanged until a page has no next.
--encoding utf8: content is decoded text; each page ends on a character boundary, so joining every page's content in
  order rebuilds the file exactly. Fails with notUtf8 (exit 2) when the file is not valid UTF-8.
--encoding base64: content is base64 of the raw bytes; decode each page and join the bytes before reading them as text.
--encoding auto (default): utf8 when the whole file is valid UTF-8, else base64; the output's encoding field says which.
--length is a byte count from 1 to 12000; a utf8 page can run up to 3 bytes past it only to finish one character.
Exit 2 on invalid input (invalidArguments, notUtf8) or a changed source (sourceChanged); no file is written.`;

const args = process.argv.slice(2);
const get = (key, fallback) => args.includes(key) ? args[args.indexOf(key) + 1] : fallback;
if (args.includes('--help') || args.includes('-h')) { console.log(HELP); process.exit(0); }

// A UTF-8 continuation byte (10xxxxxx) never starts a character.
const continuation = (byte) => (byte & 0xc0) === 0x80;

// Hash the file and check, in the same pass, that every byte sequence is valid UTF-8.
async function scan(file) {
  const hash = createHash('sha256'), decoder = new TextDecoder('utf-8', { fatal: true });
  let utf8 = true;
  for await (const chunk of createReadStream(file)) {
    hash.update(chunk);
    if (utf8) try { decoder.decode(chunk, { stream: true }); } catch { utf8 = false; }
  }
  if (utf8) try { decoder.decode(); } catch { utf8 = false; }
  return { snapshot: hash.digest('hex'), utf8 };
}

function readRange(file, offset, size) {
  const fd = openSync(file, 'r'), bytes = Buffer.alloc(size);
  try {
    let at = 0;
    while (at < size) { const n = readSync(fd, bytes, at, size - at, offset + at); if (!n) throw new Error('Incomplete source'); at += n; }
  } finally { closeSync(fd); }
  return bytes;
}

// The page's byte count: at most `length`, cut back to a character boundary; one whole character when it is longer.
function textCut(bytes, length) {
  if (bytes.length <= length) return bytes.length;
  let cut = length;
  while (cut > 0 && continuation(bytes[cut])) cut -= 1;
  if (cut > 0) return cut;
  cut = 1;
  while (cut < bytes.length && continuation(bytes[cut])) cut += 1;
  return cut;
}

try {
  if (!get('--file')) throw new Error('--file is required');
  const file = resolve(get('--file')), offset = Number(get('--offset', 0)), length = Number(get('--length', 6000));
  const requested = get('--encoding', 'auto');
  if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 || length > 12000) throw new Error('Invalid offset or length');
  if (!['auto', 'utf8', 'base64'].includes(requested)) throw new Error('--encoding must be auto, utf8 or base64');
  const { snapshot, utf8 } = await scan(file);
  if (get('--snapshot') && get('--snapshot') !== snapshot) throw new Error('Source changed; restart paging');
  if (requested === 'utf8' && !utf8) fail('notUtf8', `${file} is not valid UTF-8; page it with --encoding base64`, 2);
  const encoding = requested === 'auto' ? (utf8 ? 'utf8' : 'base64') : requested;
  const total = statSync(file).size; if (offset > total) throw new Error('Offset exceeds source');
  const available = total - offset;
  let bytes;
  if (encoding === 'utf8') {
    const window = readRange(file, offset, Math.min(length + 3, available));
    if (window.length && continuation(window[0])) throw new Error(`Offset ${offset} is inside a UTF-8 character; start at a page boundary`);
    bytes = window.subarray(0, textCut(window, length));
  } else bytes = readRange(file, offset, Math.min(length, available));
  const content = encoding === 'utf8' ? new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes) : bytes.toString('base64');
  const end = offset + bytes.length;
  const next = end < total ? { continue: { command: process.execPath, args: [fileURLToPath(import.meta.url), '--file', file, '--offset', String(end), '--length', String(length), '--encoding', encoding, '--snapshot', snapshot] } } : null;
  console.log(JSON.stringify({ file, snapshot, totalBytes: total, offset, bytes: bytes.length, encoding, content, ...(next ? { next } : {}) }));
} catch (error) { fail(/changed/.test(error.message) ? 'sourceChanged' : 'invalidArguments', error.message, 2); }
