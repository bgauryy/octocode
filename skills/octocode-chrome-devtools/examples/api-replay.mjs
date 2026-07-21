#!/usr/bin/env node

const argv = process.argv.slice(2);
const getArg = (flag, def = '') => {
  const index = argv.indexOf(flag);
  return index === -1 ? def : argv[index + 1] ?? def;
};
const hasFlag = (flag) => argv.includes(flag);

if (hasFlag('--help') || !getArg('--url')) {
  console.error(`Usage: node api-replay.mjs --url <url> [--method GET] [--headers '{"accept":"application/json"}'] [--body '<json-or-text>'] [--page 1] [--max-chars 4000]\n\nReplays a discovered HTTP/API request with caller-provided non-secret inputs and prints bounded response metadata/content.`);
  process.exit(hasFlag('--help') ? 0 : 1);
}

const url = getArg('--url');
const method = getArg('--method', 'GET').toUpperCase();
const body = getArg('--body', '');
const page = Math.max(1, Number.parseInt(getArg('--page', '1'), 10));
const maxChars = Math.max(500, Math.min(20000, Number.parseInt(getArg('--max-chars', '4000'), 10)));

function parseHeaders(raw) {
  if (!raw) return {};
  try {
    const value = JSON.parse(raw);
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('headers must be an object');
    return Object.fromEntries(Object.entries(value).map(([key, headerValue]) => [key, String(headerValue)]));
  } catch (error) {
    throw new Error(`Invalid --headers JSON: ${error.message}`);
  }
}

function safeHeaderNames(headers) {
  return [...headers.keys()].filter(Boolean).sort();
}

const headers = parseHeaders(getArg('--headers', ''));
const init = { method, headers };
if (body && !['GET', 'HEAD'].includes(method)) init.body = body;

const response = await fetch(url, init);
const contentType = response.headers.get('content-type') ?? '';
const text = await response.text();
const start = (page - 1) * maxChars;
const slice = text.slice(start, start + maxChars);

console.log(JSON.stringify({
  request: {
    url,
    method,
    headerNames: Object.keys(headers).sort(),
    bodyProvided: Boolean(body),
  },
  response: {
    ok: response.ok,
    status: response.status,
    statusText: response.statusText,
    contentType,
    headerNames: safeHeaderNames(response.headers),
    bytes: Buffer.byteLength(text),
  },
  page,
  maxChars,
  hasMore: start + maxChars < text.length,
  contentPreview: slice,
}, null, 2));
