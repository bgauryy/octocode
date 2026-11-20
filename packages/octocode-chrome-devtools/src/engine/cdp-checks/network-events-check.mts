/**
 * Network events: an observation window that records WebSocket, Server-Sent Events and HTTP
 * request/response events into network-events.jsonl (one event per line). Payloads are saved
 * whole in sidecar files payloads/<n>.txt|.bin that the event line names. Credential headers
 * (Authorization, Cookie, Set-Cookie, Proxy-Authorization) are redacted in the jsonl and stdout;
 * payload sidecars are raw evidence and may contain tokens.
 */
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Payload, Session } from '../cdp-connection.mjs';
import { redactHeaders } from './net-headers.mjs';

const env = process.env;
const KINDS = ['websocket', 'sse', 'http'] as const;
type Kind = (typeof KINDS)[number];
/** Event lines printed inline; the jsonl keeps every line and `query` pages the rest. */
const PAGE_ROWS = 10;

const out = (line: string) => {
  // oxlint-disable-next-line eslint/no-console -- stdout is the check's output protocol
  console.log(line);
};
const invalid = (error: string) => {
  out(`[FINDING] NETWORK_EVENTS_INVALID ${JSON.stringify({ error })}`);
  process.exitCode = 2;
};

function parseKinds(): Kind[] | string {
  if (!env.NETEVENTS_KINDS) return [...KINDS];
  let parsed: unknown;
  try {
    parsed = JSON.parse(env.NETEVENTS_KINDS);
  } catch {
    return 'NETEVENTS_KINDS must be a JSON array';
  }
  if (
    !Array.isArray(parsed) ||
    !parsed.length ||
    !parsed.every(k => KINDS.includes(k))
  )
    return `NETEVENTS_KINDS must be a non-empty array of ${KINDS.join('|')}`;
  return parsed as Kind[];
}

export async function run(cdp: Session) {
  const ms = Number(env.NETEVENTS_MS);
  if (!env.NETEVENTS_MS || !Number.isSafeInteger(ms) || ms < 1)
    return invalid('NETEVENTS_MS is required and must be a positive integer');
  const kinds = parseKinds();
  if (typeof kinds === 'string') return invalid(kinds);
  const match = env.NETEVENTS_MATCH ?? '';

  const file = join(cdp.outputDir, 'network-events.jsonl');
  const payloadDir = join(cdp.outputDir, 'payloads');
  writeFileSync(file, '', { mode: 0o600 });
  const started = Date.now();
  const counts: Record<string, number> = { websocket: 0, sse: 0, http: 0 };
  const rows: Payload[] = [];
  let n = 0;
  let unmatched = 0;
  const urls = new Map<string, string>();

  const wanted = (kind: Kind, url: string | undefined) => {
    if (!kinds.includes(kind)) return false;
    if (match && !(url ?? '').includes(match)) {
      unmatched++;
      return false;
    }
    return true;
  };
  const record = (
    kind: Kind,
    type: string,
    requestId: string,
    fields: Payload,
    payload?: { data: Buffer; ext: 'txt' | 'bin' }
  ) => {
    const index = n++;
    const row: Payload = {
      n: index,
      t: Date.now() - started,
      kind,
      type,
      requestId,
      url: urls.get(requestId),
      ...fields,
    };
    if (payload) {
      mkdirSync(payloadDir, { recursive: true, mode: 0o700 });
      const name = `payloads/${index}.${payload.ext}`;
      writeFileSync(join(cdp.outputDir, name), payload.data, { mode: 0o600 });
      row.payloadFile = name;
      row.payloadBytes = payload.data.length;
    }
    counts[kind]++;
    rows.push(row);
    appendFileSync(file, `${JSON.stringify(row)}\n`);
  };
  const text = (value: string) => ({
    data: Buffer.from(value, 'utf8'),
    ext: 'txt' as const,
  });

  await cdp.send('Network.enable');

  cdp.on('Network.webSocketCreated', (p: Payload) => {
    urls.set(p.requestId, p.url);
    if (wanted('websocket', p.url))
      record('websocket', 'created', p.requestId, {
        initiator: p.initiator?.type,
      });
  });
  cdp.on('Network.webSocketWillSendHandshakeRequest', (p: Payload) => {
    if (wanted('websocket', urls.get(p.requestId)))
      record('websocket', 'handshakeRequest', p.requestId, {
        headers: redactHeaders(p.request?.headers),
      });
  });
  cdp.on('Network.webSocketHandshakeResponseReceived', (p: Payload) => {
    if (wanted('websocket', urls.get(p.requestId)))
      record('websocket', 'handshakeResponse', p.requestId, {
        status: p.response?.status,
        headers: redactHeaders(p.response?.headers),
      });
  });
  for (const [event, direction] of [
    ['webSocketFrameSent', 'sent'],
    ['webSocketFrameReceived', 'received'],
  ] as const)
    cdp.on(`Network.${event}`, (p: Payload) => {
      if (!wanted('websocket', urls.get(p.requestId))) return;
      const binary = p.response.opcode === 2;
      record(
        'websocket',
        `frame-${direction}`,
        p.requestId,
        { direction, opcode: p.response.opcode },
        binary
          ? { data: Buffer.from(p.response.payloadData, 'base64'), ext: 'bin' }
          : text(p.response.payloadData)
      );
    });
  cdp.on('Network.webSocketFrameError', (p: Payload) => {
    if (wanted('websocket', urls.get(p.requestId)))
      record('websocket', 'frame-error', p.requestId, {
        errorMessage: p.errorMessage,
      });
  });
  cdp.on('Network.webSocketClosed', (p: Payload) => {
    if (wanted('websocket', urls.get(p.requestId)))
      record('websocket', 'closed', p.requestId, {});
  });

  cdp.on('Network.eventSourceMessageReceived', (p: Payload) => {
    if (wanted('sse', urls.get(p.requestId)))
      record(
        'sse',
        'message',
        p.requestId,
        { eventName: p.eventName, eventId: p.eventId },
        text(p.data ?? '')
      );
  });

  cdp.on('Network.requestWillBeSent', (p: Payload) => {
    urls.set(p.requestId, p.request.url);
    if (wanted('http', p.request.url))
      record('http', 'request', p.requestId, {
        method: p.request.method,
        resourceType: p.type,
        headers: redactHeaders(p.request.headers),
        hasPostData: Boolean(p.request.hasPostData),
        initiator: p.initiator,
        redirectFrom: p.redirectResponse?.url,
        timestamp: p.timestamp,
        wallTime: p.wallTime,
      });
  });
  cdp.on('Network.requestWillBeSentExtraInfo', (p: Payload) => {
    if (wanted('http', urls.get(p.requestId)))
      record('http', 'request-extra', p.requestId, {
        headers: redactHeaders(p.headers),
      });
  });
  cdp.on('Network.responseReceived', (p: Payload) => {
    urls.set(p.requestId, p.response.url);
    if (wanted('http', p.response.url))
      record('http', 'response', p.requestId, {
        status: p.response.status,
        mimeType: p.response.mimeType,
        protocol: p.response.protocol,
        fromDiskCache: p.response.fromDiskCache,
        headers: redactHeaders(p.response.headers),
        timing: p.response.timing,
        timestamp: p.timestamp,
      });
  });
  cdp.on('Network.responseReceivedExtraInfo', (p: Payload) => {
    if (wanted('http', urls.get(p.requestId)))
      record('http', 'response-extra', p.requestId, {
        status: p.statusCode,
        headers: redactHeaders(p.headers),
      });
  });
  cdp.on('Network.loadingFinished', (p: Payload) => {
    if (wanted('http', urls.get(p.requestId)))
      record('http', 'finished', p.requestId, {
        encodedDataLength: p.encodedDataLength,
        timestamp: p.timestamp,
      });
  });
  cdp.on('Network.loadingFailed', (p: Payload) => {
    if (wanted('http', urls.get(p.requestId)))
      record('http', 'failed', p.requestId, {
        errorText: p.errorText,
        canceled: p.canceled,
        blockedReason: p.blockedReason,
        timestamp: p.timestamp,
      });
  });

  // Listeners are attached: a load now is observed from its first request.
  if (env.NETEVENTS_URL) {
    await cdp.send('Page.enable');
    const nav = await cdp.send('Page.navigate', { url: env.NETEVENTS_URL });
    if (nav.errorText)
      out(
        `[FINDING] NETWORK_EVENTS_NAVIGATION_FAILED ${JSON.stringify({ errorText: nav.errorText })}`
      );
  }
  await new Promise(resolve => setTimeout(resolve, ms));

  const total = n;
  out(
    `[METRIC] NETWORK_EVENTS windowMs=${ms} total=${total} websocket=${counts.websocket} sse=${counts.sse} http=${counts.http} filteredOut=${unmatched} kinds=${kinds.join(',')}${match ? ` match=${JSON.stringify(match)}` : ''}`
  );
  for (const row of rows.slice(0, PAGE_ROWS))
    out(
      `[EVENT] ${JSON.stringify({ n: row.n, t: row.t, kind: row.kind, type: row.type, url: row.url, status: row.status, direction: row.direction, payloadFile: row.payloadFile, payloadBytes: row.payloadBytes })}`
    );
  if (total > PAGE_ROWS) {
    out(
      `[EVENT] ${PAGE_ROWS} of ${total} events shown; every line is in the jsonl`
    );
    out(
      `[NEXT] ${JSON.stringify({
        command: process.execPath,
        args: [
          join(
            dirname(fileURLToPath(import.meta.url)),
            '..',
            'evidence-query.mjs'
          ),
          '--file',
          file,
          '--format',
          'jsonl',
          '--cursor',
          String(PAGE_ROWS),
          '--limit',
          String(PAGE_ROWS),
        ],
      })}`
    );
  }
  out(`[ARTIFACT] NETWORK_EVENTS ${file}`);
  if (existsPayloads(rows))
    out(
      `[ARTIFACT] NETWORK_EVENTS_PAYLOADS ${payloadDir} (raw frame and message payloads; may contain tokens, treat like HAR bodies)`
    );
}

const existsPayloads = (rows: Payload[]) => rows.some(r => r.payloadFile);
