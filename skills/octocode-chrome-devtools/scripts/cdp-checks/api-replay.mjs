#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const argv = process.argv.slice(2);
const arg = (flag, fallback = '') => argv.includes(flag) ? argv[argv.indexOf(flag) + 1] : fallback;
if (argv.includes('--help') || argv.includes('-h')) {
  console.log('Usage: api-replay.mjs --url <url> [--method GET] [--headers <json>] [--body <text>] [--timeout-ms 30000] [--max-chars 4000]\n       api-replay.mjs --response-file <artifact> [--page 1] [--max-chars 4000]\nRequests once; continuation reads the saved response without repeating the request.');
  process.exit(0);
}
try {
  const page = Number(arg('--page', '1'));
  const maxChars = Number(arg('--max-chars', '4000'));
  const timeout = Number(arg('--timeout-ms', '30000'));
  if (!Number.isSafeInteger(page) || page < 1 || !Number.isSafeInteger(maxChars) || maxChars < 1 || maxChars > 20000 || !Number.isSafeInteger(timeout) || timeout < 1) throw new Error('Invalid page, max-chars, or timeout-ms');
  let responseFile = arg('--response-file');
  let payload;
  if (responseFile) {
    if (argv.includes('--url') || argv.includes('--body') || argv.includes('--headers') || argv.includes('--method')) throw new Error('Response-file mode cannot send a request');
    responseFile = resolve(responseFile);
    payload = JSON.parse(readFileSync(responseFile, 'utf8'));
  } else {
    if (page !== 1) throw new Error('Use the saved response continuation for pages after 1');
    const url = arg('--url');
    if (!url || !['http:', 'https:'].includes(new URL(url).protocol)) throw new Error('An HTTP(S) --url is required');
    const method = arg('--method', 'GET').toUpperCase();
    const headers = JSON.parse(arg('--headers', '{}'));
    if (!headers || typeof headers !== 'object' || Array.isArray(headers)) throw new Error('Headers must be an object');
    const body = arg('--body');
    if (body && ['GET', 'HEAD'].includes(method)) throw new Error('GET/HEAD cannot have a body');
    const response = await fetch(url, { method, headers, ...(body ? { body } : {}), signal: AbortSignal.timeout(timeout) });
    const text = await response.text();
    payload = {
      request: { url, method, headerNames: Object.keys(headers).sort(), bodyProvided: Boolean(body) },
      response: { ok: response.ok, status: response.status, statusText: response.statusText, contentType: response.headers.get('content-type') || '', headerNames: [...response.headers.keys()].sort(), bytes: Buffer.byteLength(text) },
      text,
    };
    const dir = resolve('.octocode/tmp/chrome-devtools/api-responses');
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    responseFile = join(dir, randomUUID() + '.json');
    writeFileSync(responseFile, JSON.stringify(payload), { mode: 0o600 });
  }
  if (typeof payload.text !== 'string' || !payload.response) throw new Error('Invalid response artifact');
  const start = (page - 1) * maxChars;
  const hasMore = start + maxChars < payload.text.length;
  console.log(JSON.stringify({ request: payload.request, response: payload.response, artifact: responseFile, page, maxChars, hasMore,
    contentPreview: payload.text.slice(start, start + maxChars),
    ...(hasMore ? { next: { continue: { command: process.execPath, args: [fileURLToPath(import.meta.url), '--response-file', responseFile, '--page', String(page + 1), '--max-chars', String(maxChars)] } } } : {}),
  }, null, 2));
} catch (error) {
  console.error(`[API_REPLAY] ${error.message}`);
  process.exitCode = 1;
}
