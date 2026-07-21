#!/usr/bin/env node
/**
 * Redact secrets from a HAR 1.2 file for safer sharing.
 * Writes a new file; never prints secret values.
 *
 * Usage:
 *   node har-redact.mjs <in.har> [--out <out.har>] [--strip-bodies]
 */
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, resolve } from 'path';

const argv = process.argv.slice(2);
const getArg = (flag, def) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
const hasFlag = (flag) => argv.includes(flag);
const inPath = argv.find(a => !a.startsWith('--'));

if (!inPath || hasFlag('--help')) {
  console.error('Usage: node har-redact.mjs <in.har> [--out <out.har>] [--strip-bodies]');
  process.exit(inPath ? 0 : 1);
}

const outPath = resolve(getArg('--out', inPath.replace(/\.har$/i, '') + '.redacted.har'));
const stripBodies = hasFlag('--strip-bodies');
const SECRET_HEADER = /^(cookie|set-cookie|authorization|proxy-authorization|x-api-key|x-auth-token|x-csrf-token)$/i;
const SECRET_QUERY = /token|key|secret|session|auth|password|signature|jwt/i;

function redactUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    for (const key of [...url.searchParams.keys()]) {
      if (SECRET_QUERY.test(key)) url.searchParams.set(key, '[REDACTED]');
    }
    return url.href;
  } catch {
    return String(raw ?? '');
  }
}

function redactHeaders(headers = []) {
  return headers.map((h) => {
    const name = h.name || h.Name || '';
    if (SECRET_HEADER.test(name)) return { name, value: '[REDACTED]' };
    return { name, value: String(h.value ?? '') };
  });
}

function redactCookies(list = []) {
  return list.map((c) => ({
    ...c,
    value: '[REDACTED]',
  }));
}

function redactPostData(postData) {
  if (!postData) return postData;
  const text = String(postData.text ?? '');
  if (!text) return { ...postData, text: '' };
  if (/password|token|secret|authorization/i.test(text) || text.length > 4000) {
    return { ...postData, text: '[REDACTED]', comment: 'body redacted' };
  }
  return postData;
}

const har = JSON.parse(readFileSync(inPath, 'utf8'));
const entries = har?.log?.entries || [];
let redactedHeaderCount = 0;
let redactedCookieCount = 0;

for (const entry of entries) {
  if (entry.request) {
    entry.request.url = redactUrl(entry.request.url);
    const beforeH = JSON.stringify(entry.request.headers || []);
    entry.request.headers = redactHeaders(entry.request.headers || []);
    if (JSON.stringify(entry.request.headers) !== beforeH) redactedHeaderCount++;
    if (entry.request.cookies?.length) {
      redactedCookieCount += entry.request.cookies.length;
      entry.request.cookies = redactCookies(entry.request.cookies);
    }
    if (entry.request.queryString) {
      entry.request.queryString = entry.request.queryString.map((q) => (
        SECRET_QUERY.test(q.name) ? { name: q.name, value: '[REDACTED]' } : q
      ));
    }
    entry.request.postData = redactPostData(entry.request.postData);
  }
  if (entry.response) {
    entry.response.headers = redactHeaders(entry.response.headers || []);
    if (entry.response.cookies?.length) {
      redactedCookieCount += entry.response.cookies.length;
      entry.response.cookies = redactCookies(entry.response.cookies);
    }
    if (stripBodies && entry.response.content) {
      entry.response.content = {
        ...entry.response.content,
        text: '',
        encoding: undefined,
        comment: 'body stripped',
      };
    }
  }
}

mkdirSync(dirname(outPath), { recursive: true, mode: 0o700 });
writeFileSync(outPath, `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });

console.log(`[METRIC] entries=${entries.length} headerRowsTouched=${redactedHeaderCount} cookiesRedacted=${redactedCookieCount}`);
console.log(`[ARTIFACT] REDACTED_HAR ${outPath}`);
console.log('[REASON] share only redacted HAR; keep originals local');
