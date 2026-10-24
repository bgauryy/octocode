#!/usr/bin/env node
// Transparent JEV transport accounting, including availability probes and failures.
// No source bodies, credentials, answers, or request headers are persisted.
import { createServer } from 'node:http';
import { appendFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { isMainModule } from '../../../skills/common/scripts/lib/entrypoint.mjs';
import { hash } from '../local/e2e/harness/util.mjs';

const reportedCount = value => Number.isSafeInteger(value) && value >= 0 ? value : null;

export async function startProxy({ endpoint, key, log, timeoutMs = 30000, allowLoopback = false, maxRequests = Infinity }) {
  const upstream = new URL(endpoint);
  const loopback = ['127.0.0.1', 'localhost', '[::1]'].includes(upstream.hostname);
  if (upstream.protocol !== 'https:' && !(allowLoopback && loopback && upstream.protocol === 'http:')) {
    throw new Error('Classification upstream must use HTTPS.');
  }
  if (upstream.username || upstream.password) throw new Error('Credentials in provider URL are forbidden.');
  if (!key?.trim()) throw new Error('Classification credential is required.');
  mkdirSync(dirname(log), { recursive: true });
  let sequence = 0;
  const pending = new Set();
  const record = value => appendFileSync(log, `${JSON.stringify(value)}\n`, { mode: 0o600 });
  const server = createServer(async (request, response) => {
    if (!((request.method === 'POST' && request.url === '/v1/systemone') ||
        (request.method === 'GET' && request.url === '/v1/models'))) {
      response.writeHead(404).end();
      return;
    }
    if (sequence >= maxRequests) {
      record({ event: 'rejected', reason: 'benchmarkRequestCap', limit: maxRequests });
      response.writeHead(402, { 'content-type': 'application/json' })
        .end('{"error":"benchmark request cap"}');
      return;
    }
    const id = ++sequence;
    const started = performance.now();
    const body = [];
    let bytes = 0;
    // Bound instrumentation allocation; supported ranking/clasify payloads fit this ceiling.
    try {
      for await (const chunk of request) {
        bytes += chunk.length;
        if (bytes > 16 * 1024 * 1024) {
          record({ event: 'rejected', id, reason: 'bodyLimit', bytes });
          response.writeHead(413).end();
          return;
        }
        body.push(chunk);
      }
    } catch {
      record({ event: 'rejected', id, reason: 'clientRead', bytes });
      response.destroy();
      return;
    }
    const payload = Buffer.concat(body);
    let requestedModel = null;
    try { requestedModel = JSON.parse(payload).model ?? null; } catch { /* Forward exact malformed requests too. */ }
    record({ event: 'started', id, time: new Date().toISOString(), bytes, method: request.method, path: request.url, requestHash: hash(payload), requestedModel });
    const abort = new AbortController();
    const timeout = setTimeout(() => abort.abort(), timeoutMs);
    const disconnected = () => { if (!response.writableEnded) abort.abort(); };
    response.on('close', disconnected);
    let finish;
    const done = new Promise(resolve => { finish = resolve; });
    pending.add(done);
    try {
      const requestHeaders = { 'content-type': 'application/json', authorization: `Bearer ${key}` };
      for (const name of ['typesafe-danger-zone-images', 'typesafe-danger-zone-long-context']) {
        const value = request.headers[name];
        if (typeof value === 'string') requestHeaders[name] = value;
      }
      const target = request.method === 'GET' ? new URL('/v1/models', upstream) : upstream;
      const result = await fetch(target, {
        method: request.method, headers: requestHeaders,
        body: request.method === 'POST' ? payload : undefined, signal: abort.signal, redirect: 'error',
      });
      const raw = await result.arrayBuffer();
      let parsed;
      try { parsed = JSON.parse(Buffer.from(raw).toString('utf8')); } catch { /* Unknown usage stays unknown. */ }
      record({ event: 'finished', id, httpStatus: result.status, ms: performance.now() - started,
        responseContentType: result.headers.get('content-type'), responseBytes: raw.byteLength, responseHash: hash(Buffer.from(raw)),
        resolvedModel: parsed?.model ?? null,
        inputTokens: reportedCount(parsed?.usage?.input_tokens),
        outputTokens: reportedCount(parsed?.usage?.output_tokens) });
      const headers = { 'content-type': result.headers.get('content-type') ?? 'application/json' };
      for (const name of ['retry-after', 'x-ratelimit-limit', 'x-ratelimit-remaining', 'x-ratelimit-reset']) {
        if (result.headers.has(name)) headers[name] = result.headers.get(name);
      }
      if (!response.destroyed) response.writeHead(result.status, headers).end(Buffer.from(raw));
    } catch (error) {
      record({ event: 'finished', id, httpStatus: null, ms: performance.now() - started,
        errorType: error.name, inputTokens: null, outputTokens: null });
      if (!response.destroyed) response.writeHead(502, { 'content-type': 'application/json' })
        .end('{"error":"benchmark upstream request failed"}');
    } finally {
      clearTimeout(timeout);
      response.off('close', disconnected);
      pending.delete(done);
      finish();
    }
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  return {
    url: `http://127.0.0.1:${server.address().port}`,
    close: async () => {
      const closed = new Promise(resolve => server.close(resolve));
      await Promise.allSettled([...pending]);
      server.closeAllConnections();
      await closed;
    },
  };
}

// argv[1] keeps a symlinked path while import.meta.url is the real path, so compare real paths.
if (isMainModule(import.meta.url)) {
  const index = process.argv.indexOf('--log');
  if (index < 0 || !process.argv[index + 1]) throw new Error('Usage: classification_proxy.mjs --log <path>');
  const host = process.env.OCTOCODE_CLASSIFICATION_API_HOST ?? 'https://api.typesafe.ai';
  const capIndex = process.argv.indexOf('--max-requests');
  const maxRequests = capIndex < 0 ? Infinity : Number(process.argv[capIndex + 1]);
  if (capIndex >= 0 && (!Number.isSafeInteger(maxRequests) || maxRequests < 1)) {
    throw new Error('--max-requests must be a positive integer.');
  }
  const proxy = await startProxy({
    endpoint: new URL('v1/systemone', `${host.replace(/\/$/, '')}/`).href,
    key: process.env.OCTOCODE_CLASSIFICATION_API, log: process.argv[index + 1], maxRequests,
  });
  process.stdout.write(`${JSON.stringify({ url: proxy.url })}\n`);
  let stopping = false;
  for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, async () => {
    if (stopping) return;
    stopping = true;
    await proxy.close();
    process.exitCode = 0;
  });
}
