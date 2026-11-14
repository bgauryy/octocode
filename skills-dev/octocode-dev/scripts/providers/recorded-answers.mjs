// The recorded-answer store: real provider answers for public packages and
// repositories, one JSON file per tool call under test-utils/provider-answers.
// An exchange carries the fields of the transport's ProviderAnswer (status,
// headers as pairs, body, next), with `body` parsed when it is JSON so a review
// reads it, and `request` as what the wire showed.
// The Rust tests read the same files (crates/runtime/src/providers/artifact/
// test_http/recorded.rs). Refresh: `dev.mjs answers:record`.
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from '../lib/workspaces.mjs';

export const ANSWERS = path.join(ROOT, 'test-utils/provider-answers');

const sorted = value => {
  if (Array.isArray(value)) return value.map(sorted);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.keys(value).toSorted((a, b) => a.localeCompare(b)).map(key => [key, sorted(value[key])]));
};

/** One request's identity: method, URL, what it accepts, and a JSON body whatever its key order. */
export function requestKey({ method, url, accept, body }) {
  const head = `${method} ${url} ${accept ?? ''}`.trimEnd();
  if (body === undefined || body === '') return head;
  let parsed = body;
  if (typeof body === 'string') { try { parsed = JSON.parse(body); } catch { /* A text body keys as itself. */ } }
  return `${head} ${JSON.stringify(sorted(parsed))}`;
}

export const casePath = id => path.join(ANSWERS, `${id}.json`);
export const readCase = id => JSON.parse(fs.readFileSync(casePath(id), 'utf8'));
export const hasCase = id => fs.existsSync(casePath(id));

export function writeCase(id, recording) {
  fs.mkdirSync(path.dirname(casePath(id)), { recursive: true });
  fs.writeFileSync(casePath(id), `${JSON.stringify(recording, null, 1)}\n`);
}

/** The exchange as the registry proxy serves it. */
export const served = exchange => ({ status: exchange.status, headers: Object.fromEntries(exchange.headers), body: exchange.base64 === undefined ? exchange.body ?? '' : Buffer.from(exchange.base64, 'base64') });

/** A case's JSON answer at `urlPart`, for a test that builds on a real answer. */
export function recordedJson(id, urlPart) {
  const exchange = readCase(id).exchanges.find(({ request }) => request.url.includes(urlPart));
  if (!exchange?.body || typeof exchange.body !== 'object') throw new Error(`${id} holds no JSON answer for ${urlPart}`);
  return exchange.body;
}

/** Replay of `recording`: answers by request identity, and lists what was asked and what it does not hold. */
export function replayer(recording) {
  const byKey = new Map(recording.exchanges.map(exchange => [requestKey(exchange.request), exchange]));
  const unrecorded = [], asked = new Set();
  const answer = (req, url, sent) => {
    const key = requestKey({ method: req.method, url: url.href, accept: req.headers.accept, body: sent });
    asked.add(key);
    const exchange = byKey.get(key);
    if (exchange) return served(exchange);
    unrecorded.push(key.slice(0, 300));
    return { status: 404, body: { message: 'Not in the recording: refresh it with dev.mjs answers:record' } };
  };
  return { answer, unrecorded, asked };
}

export const hostsOf = recording => [...new Set(recording.exchanges.map(({ request }) => new URL(request.url).hostname))];
