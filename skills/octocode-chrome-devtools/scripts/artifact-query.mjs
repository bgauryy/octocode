#!/usr/bin/env node
import { readFileSync, openSync, readSync, closeSync, fstatSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const get = (flag, fallback) => args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
function fitText(content) {
  if (Buffer.byteLength(JSON.stringify(content)) <= 20000) return content;
  let low = 0, high = content.length;
  while (low < high) {
    const mid = Math.ceil((low + high) / 2);
    if (Buffer.byteLength(JSON.stringify(content.slice(0, mid))) <= 20000) low = mid; else high = mid - 1;
  }
  if (low && /[\uD800-\uDBFF]/u.test(content[low - 1]) && /[\uDC00-\uDFFF]/u.test(content[low] || '')) low--;
  return content.slice(0, low);
}
if (args.includes('--help')) {
  console.log('Usage: artifact-query.mjs --file <capture> [--format text|json|binary] [--pointer /JSON/pointer] [--offset 0] [--length 8000] [--sha256 <digest>]\nLossless bounded pages for any saved source. Whole-file offsets use source bytes; JSON-pointer offsets use UTF-16 units. Pages respect encoding boundaries and a 20000-byte encoded-content budget. Decode each binary page from base64 before joining bytes. Continuations pin the original file digest.');
  process.exit(0);
}
try {
  if (!get('--file')) throw new Error('--file is required');
  const file = resolve(get('--file')), format = get('--format', 'text'), pointer = get('--pointer', '');
  const offset = Number(get('--offset', '0')), length = Number(get('--length', '8000'));
  if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 || length > 20000) throw new Error('Invalid offset or length');
  if (!['text', 'json', 'binary'].includes(format)) throw new Error('Invalid format');
  // Hash with bounded memory, including captures too large for readFileSync.
  const fd = openSync(file, 'r'), hash = createHash('sha256'), block = Buffer.alloc(1024 * 1024);
  let sourceBytes, sha256, raw;
  try {
    const before = fstatSync(fd); sourceBytes = before.size;
    for (let at = 0; at < sourceBytes;) {
      const n = readSync(fd, block, 0, Math.min(block.length, sourceBytes - at), at);
      if (!n) throw new Error('Capture changed while reading');
      hash.update(block.subarray(0, n)); at += n;
    }
    sha256 = hash.digest('hex');
    if (!pointer) {
      if (offset > sourceBytes) throw new Error('Offset exceeds source size');
      raw = Buffer.alloc(Math.min(length + 3, Math.max(0, sourceBytes - offset)));
      let done = 0;
      while (done < raw.length) { const n = readSync(fd, raw, done, raw.length - done, offset + done); if (!n) throw new Error('Capture changed while reading'); done += n; }
    }
    const after = fstatSync(fd);
    if (before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs) throw new Error('Capture changed while reading');
  } finally { closeSync(fd); }
  if (get('--sha256') && get('--sha256') !== sha256) throw new Error('Capture changed; restart from offset 0');
  if (!pointer) {
    let content, used = Math.min(length, raw.length);
    if (format === 'binary') { used = Math.min(used, 14997); content = raw.subarray(0, used).toString('base64'); }
    else {
      // Keep UTF-8 sequences on one page. A tiny page still returns one whole code point.
      if (offset + used < sourceBytes && used) {
        let start = used - 1;
        while (start >= 0 && (raw[start] & 0xC0) === 0x80) start--;
        if (start < 0) throw new Error('Offset splits a UTF-8 sequence');
        const lead = raw[start], width = lead < 0x80 ? 1 : lead < 0xE0 ? 2 : lead < 0xF0 ? 3 : 4;
        if (start + width > used) {
          if (start) used = start;
          else used = width;
        }
      }
      content = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(raw.subarray(0, used));
      content = fitText(content); used = Buffer.byteLength(content);
    }
    const nextOffset = offset + used;
    console.log(JSON.stringify({ file, format, pointer, sha256, encoding: format === 'binary' ? 'base64' : 'utf8', unit: 'source-byte', offset, length: used, totalUnits: sourceBytes, sourceBytes, content,
      ...(nextOffset < sourceBytes ? { next: { continue: { command: process.execPath, args: [fileURLToPath(import.meta.url), '--file', file, '--format', format, '--offset', String(nextOffset), '--length', String(length), '--sha256', sha256] } } } : {}),
    }));
    process.exit(0);
  }
  let value;
  if (format === 'json') {
    const bytes = readFileSync(file);
    if (createHash('sha256').update(bytes).digest('hex') !== sha256) throw new Error('Capture changed while reading');
    value = JSON.parse(bytes.toString('utf8'), (_key, item, context) => typeof item === 'number' && context?.source ? JSON.rawJSON(context.source) : item);
    if (pointer && !pointer.startsWith('/')) throw new Error('JSON pointer must start with /');
    for (const part of pointer ? pointer.slice(1).split('/') : []) {
      if (/~(?![01])/u.test(part)) throw new Error('Invalid JSON pointer escape');
      const key = part.replace(/~1/g, '/').replace(/~0/g, '~');
      if (value === null || typeof value !== 'object' || JSON.isRawJSON(value) || !Object.hasOwn(value, key)) throw new Error('JSON pointer not found');
      value = value[key];
    }
    value = JSON.stringify(value, null, 2);
  } else {
    throw new Error('--pointer requires --format json');
  }
  if (offset > value.length) throw new Error('Offset exceeds selected value');
  // Keep surrogate pairs together without allocating an array for the whole capture.
  if (offset > 0 && /[\uDC00-\uDFFF]/u.test(value[offset] || '') && /[\uD800-\uDBFF]/u.test(value[offset - 1])) throw new Error('Offset splits a Unicode surrogate pair');
  let end = Math.min(offset + length, value.length);
  if (end > 0 && /[\uD800-\uDBFF]/u.test(value[end - 1]) && /[\uDC00-\uDFFF]/u.test(value[end] || '')) end++;
  const content = fitText(value.slice(offset, end)), nextOffset = offset + content.length;
  const nextArgs = [fileURLToPath(import.meta.url), '--file', file, '--format', format, '--offset', String(nextOffset), '--length', String(length), '--sha256', sha256, ...(pointer ? ['--pointer', pointer] : [])];
  console.log(JSON.stringify({ file, format, pointer, sha256, encoding: 'utf8', unit: 'utf16-code-unit', offset, length: content.length, totalUnits: value.length, sourceBytes, content,
    ...(nextOffset < value.length ? { next: { continue: { command: process.execPath, args: nextArgs } } } : {}),
  }));
} catch (error) { console.error('[ARTIFACT_QUERY] ' + error.message); process.exitCode = 1; }
