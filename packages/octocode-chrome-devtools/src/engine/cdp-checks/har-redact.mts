#!/usr/bin/env node
/**
 * Redact secrets from a HAR 1.2 file for safer sharing.
 * Writes a new file; never prints secret values.
 *
 * Usage:
 *   node har-redact.mjs <in.har> [--out <out.har>] [--keep-bodies] [--strip-bodies]
 */
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'path';
import { validateFlags } from '../cli-flags.mjs';

const argv = process.argv.slice(2);
const getArg = (flag: string, def?: string) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
const hasFlag = flag => argv.includes(flag);

if (!argv.length || hasFlag('--help') || hasFlag('-h')) {
  console.error(
    'Usage: node har-redact.mjs <in.har> [--out <out.har>] [--keep-bodies] [--strip-bodies]'
  );
  process.exit(hasFlag('--help') || hasFlag('-h') ? 0 : 1);
}
const [inPath] = validateFlags(
  argv,
  ['--out'],
  ['--help', '-h', '--keep-bodies', '--strip-bodies'],
  1
);
if (!inPath) throw new Error('An input HAR file is required');

const workspaceOutputBase = resolve(process.cwd(), '.octocode');
const defaultName = basename(inPath).replace(/\.har$/i, '') + '.redacted.har';
const outPath = resolve(
  getArg(
    '--out',
    join(
      workspaceOutputBase,
      'tmp',
      'chrome-devtools',
      'redacted-har',
      defaultName
    )
  )
);
const outRelative = relative(workspaceOutputBase, outPath);
if (outRelative.startsWith('..') || isAbsolute(outRelative)) {
  console.error(`--out must stay under ${workspaceOutputBase}`);
  process.exit(2);
}
const stripBodies = !hasFlag('--keep-bodies') || hasFlag('--strip-bodies');
const SECRET_HEADER =
  /^(cookie|set-cookie|authorization|proxy-authorization|x-api-key|x-auth-token|x-csrf-token)$/i;
const SECRET_QUERY = /token|key|secret|session|auth|password|signature|jwt/i;

function redactUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    url.hash = '';
    for (const key of [...url.searchParams.keys()]) {
      if (SECRET_QUERY.test(key)) url.searchParams.set(key, '[REDACTED]');
    }
    return url.href;
  } catch {
    return String(raw ?? '');
  }
}

function redactHeaders(headers = []) {
  return headers.map(h => {
    const name = h.name || h.Name || '';
    if (SECRET_HEADER.test(name) || SECRET_QUERY.test(name))
      return { name, value: '[REDACTED]' };
    return {
      name,
      value: /^(location|referer)$/i.test(name)
        ? redactUrl(h.value)
        : String(h.value ?? ''),
    };
  });
}

function redactCookies(list = []) {
  return list.map(c => ({
    ...c,
    value: '[REDACTED]',
  }));
}

function redactPostData(postData) {
  if (!postData) return postData;
  if (stripBodies)
    return { mimeType: postData.mimeType, text: '', comment: 'body stripped' };
  return {
    ...postData,
    text: '[REDACTED]',
    params: postData.params?.map(p => ({
      ...p,
      value: '[REDACTED]',
      fileName: undefined,
    })),
    comment: 'request body redacted',
  };
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
    if (JSON.stringify(entry.request.headers) !== beforeH)
      redactedHeaderCount++;
    if (entry.request.cookies?.length) {
      redactedCookieCount += entry.request.cookies.length;
      entry.request.cookies = redactCookies(entry.request.cookies);
    }
    if (entry.request.queryString) {
      entry.request.queryString = entry.request.queryString.map(q =>
        SECRET_QUERY.test(q.name) ? { name: q.name, value: '[REDACTED]' } : q
      );
    }
    entry.request.postData = redactPostData(entry.request.postData);
  }
  if (entry.response) {
    if (entry.response.redirectURL)
      entry.response.redirectURL = redactUrl(entry.response.redirectURL);
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

console.log(
  `[METRIC] entries=${entries.length} headerRowsTouched=${redactedHeaderCount} cookiesRedacted=${redactedCookieCount}`
);
console.log(`[ARTIFACT] REDACTED_HAR ${outPath}`);
console.log(
  '[REASON] review before sharing; bodies stripped by default; originals stay local'
);
