/**
 * Page resources: RES_URL, RES_TYPES (JSON array of resource types), RES_BINARY (1/0), RES_MATCH.
 * Saves every resource body of the page whole: documents, stylesheets, scripts, images, fonts, media
 * from Page.getResourceTree + Page.getResourceContent, and (with RES_URL) xhr/fetch bodies from Network.
 * Files: resources/<n>-<name> and resources-index.json. Binaries are saved only with RES_BINARY=1.
 */
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import { attachFrameSessions } from './evidence-frames.mjs';
import {
  navigateAndWait,
  out,
  sanitizeName,
  saveEvidence,
  saveIndex,
  showRows,
} from './evidence-io.mjs';
import {
  flagOption,
  sleep,
  stringsOption,
  textOption,
} from './stream-options.mjs';

const TYPES = [
  'document',
  'stylesheet',
  'script',
  'image',
  'font',
  'media',
  'xhr',
  'fetch',
  'other',
] as const;
type ResType = (typeof TYPES)[number];

const TREE_TYPE: Record<string, ResType> = {
  Document: 'document',
  Stylesheet: 'stylesheet',
  Script: 'script',
  Image: 'image',
  Font: 'font',
  Media: 'media',
  XHR: 'xhr',
  Fetch: 'fetch',
};
const EXTENSION: [RegExp, string][] = [
  [/html/, '.html'],
  [/css/, '.css'],
  [/javascript|ecmascript/, '.js'],
  [/json/, '.json'],
  [/svg/, '.svg'],
  [/png/, '.png'],
  [/jpe?g/, '.jpg'],
  [/gif/, '.gif'],
  [/webp/, '.webp'],
  [/woff2/, '.woff2'],
  [/woff/, '.woff'],
  [/^text\/plain/, '.txt'],
];

type Candidate = {
  url: string;
  type: ResType;
  mime: string;
  frame: string;
  sessionId?: string;
  source: 'resourceTree' | 'network';
  requestId?: string;
  failed?: boolean;
};
type Row = {
  n: number;
  url: string;
  type: ResType;
  mime: string;
  bytes?: number;
  sha256?: string;
  file?: string;
  source: 'resourceTree' | 'network';
  frame: string;
  error?: string;
};

function fileName(url: string, type: ResType, mime: string): string {
  let base = '';
  if (!url.startsWith('data:')) {
    try {
      base = decodeURIComponent(new URL(url).pathname.split('/').pop() ?? '');
    } catch {
      base = '';
    }
  }
  const name = sanitizeName(base, url.startsWith('data:') ? 'data-uri' : type);
  if (name.includes('.')) return name;
  const ext = EXTENSION.find(([pattern]) => pattern.test(mime))?.[1] ?? '';
  return `${name}${ext}`;
}

/** Every frame's resources in one session's tree (frames served by another session are skipped). */
function collectTree(
  tree: Payload,
  sessionId: string | undefined,
  sessions: Map<string, string>,
  rows: Candidate[]
) {
  const walk = (node: Payload, root: boolean) => {
    const frame = node.frame as Payload;
    if (!root && sessions.has(frame.id)) return;
    rows.push({
      url: frame.url + (frame.urlFragment ?? ''),
      type: 'document',
      mime: frame.mimeType ?? '',
      frame: frame.id,
      sessionId,
      source: 'resourceTree',
    });
    for (const r of node.resources ?? [])
      rows.push({
        url: r.url,
        type: TREE_TYPE[r.type] ?? 'other',
        mime: r.mimeType ?? '',
        frame: frame.id,
        sessionId,
        source: 'resourceTree',
        failed: Boolean(r.failed || r.canceled),
      });
    for (const child of node.childFrames ?? []) walk(child, false);
  };
  walk(tree, true);
}

export async function run(cdp: Session) {
  const url = textOption('RES_URL');
  const types = stringsOption('RES_TYPES');
  const binary = flagOption('RES_BINARY', false);
  const match = textOption('RES_MATCH');
  for (const t of types ?? [])
    if (!(TYPES as readonly string[]).includes(t))
      throw new Error(`RES_TYPES: ${t} is not one of ${TYPES.join(', ')}`);
  const dir = cdp.outputDir;
  const wantType = (t: ResType) => !types || types.includes(t);
  const wantUrl = (u: string) => !match || u.includes(match);
  const wantNetwork = wantType('xhr') || wantType('fetch');

  // xhr/fetch bodies are not in the resource tree: listen before the load and read them on completion.
  const requests = new Map<string, Candidate>();
  const bodies = new Map<string, Payload | { error: string }>();
  const reading = new Set<Promise<void>>();
  // A request id is unique per browser: an out-of-process iframe's document starts on the page
  // session and finishes on the frame's own session.
  const key = (id: string, _meta?: { sessionId?: string }) => id;
  const onSent = (p: Payload, meta?: { sessionId?: string }) => {
    // Documents are read too: a nested frame's document may be missing from the resource cache.
    const type =
      p.type === 'XHR'
        ? 'xhr'
        : p.type === 'Fetch'
          ? 'fetch'
          : p.type === 'Document'
            ? 'document'
            : null;
    if (!type) return;
    requests.set(key(p.requestId, meta), {
      url: p.request.url,
      type,
      mime: '',
      frame: p.frameId ?? '',
      sessionId: meta?.sessionId,
      source: 'network',
      requestId: p.requestId,
    });
  };
  const onResponse = (p: Payload, meta?: { sessionId?: string }) => {
    const row = requests.get(key(p.requestId, meta));
    if (row) row.mime = p.response?.mimeType ?? '';
  };
  const onFinished = (p: Payload, meta?: { sessionId?: string }) => {
    const k = key(p.requestId, meta);
    const row = requests.get(k);
    if (!row || !wantType(row.type) || !wantUrl(row.url)) return;
    const task = cdp
      .send(
        'Network.getResponseBody',
        { requestId: p.requestId },
        meta?.sessionId
      )
      .then(body => void bodies.set(k, body))
      .catch(error => void bodies.set(k, { error: errorMessage(error) }));
    reading.add(task);
    void task.finally(() => reading.delete(task));
  };
  const onFailed = (p: Payload, meta?: { sessionId?: string }) => {
    const row = requests.get(key(p.requestId, meta));
    if (row) {
      row.failed = true;
      bodies.set(key(p.requestId, meta), { error: String(p.errorText) });
    }
  };
  const listening = Boolean(url) && wantNetwork;
  if (listening) {
    cdp.on('Network.requestWillBeSent', onSent);
    cdp.on('Network.responseReceived', onResponse);
    cdp.on('Network.loadingFinished', onFinished);
    cdp.on('Network.loadingFailed', onFailed);
  }
  const attached = await attachFrameSessions(cdp, [
    'Page',
    ...(listening ? ['Network'] : []),
  ]);
  await cdp.send('Page.enable');
  if (listening) await cdp.send('Network.enable');
  if (url) {
    const nav = await navigateAndWait(cdp, url);
    if (nav.errorText) {
      out(
        `[FINDING] PAGE_RESOURCES_NAVIGATION_FAILED ${JSON.stringify({ url, error: nav.errorText })}`
      );
      process.exitCode = 1;
      return;
    }
    await sleep(500);
    await Promise.allSettled(reading);
  }

  await attached.settle();
  const candidates: Candidate[] = [];
  const treeErrors: string[] = [];
  const seen = new Set<string>();
  for (const sessionId of [undefined, ...attached.sessions.values()]) {
    try {
      const { frameTree } = await cdp.send(
        'Page.getResourceTree',
        {},
        sessionId
      );
      collectTree(frameTree, sessionId, attached.sessions, candidates);
    } catch (error) {
      treeErrors.push(`${sessionId ?? 'page'}: ${errorMessage(error)}`);
    }
  }
  const unique = candidates.filter(c => {
    const id = `${c.frame}|${c.url}|${c.type}`;
    if (seen.has(id)) return false;
    seen.add(id);
    return true;
  });
  const wanted = [
    ...unique,
    ...(listening
      ? [...requests.values()].filter(c => c.type !== 'document')
      : []),
  ].filter(c => wantType(c.type) && wantUrl(c.url));

  const rows: Row[] = [];
  let skippedBinary = 0;
  for (const c of wanted) {
    const row: Row = {
      n: rows.length + 1,
      url: c.url,
      type: c.type,
      mime: c.mime,
      source: c.source,
      frame: c.frame,
    };
    try {
      let content: Payload;
      if (c.source === 'network') {
        const got = bodies.get(
          key(c.requestId as string, { sessionId: c.sessionId })
        );
        if (!got || 'error' in got)
          throw new Error(got?.error ?? 'response body was never read');
        content = got;
      } else {
        if (c.failed) throw new Error('the resource failed or was canceled');
        try {
          content = await cdp.send(
            'Page.getResourceContent',
            { frameId: c.frame, url: c.url },
            c.sessionId
          );
        } catch (error) {
          // Fall back to the body Network read while the document loaded.
          const doc = [...requests.entries()].find(
            ([, r]) =>
              r.type === 'document' && r.url === c.url && r.frame === c.frame
          );
          let got: Payload | undefined = doc && bodies.get(doc[0]);
          if (doc && (!got || 'error' in got)) {
            // An out-of-process iframe's document finishes on the frame's own session: ask each.
            for (const sessionId of [
              undefined,
              ...attached.sessions.values(),
            ]) {
              got = await cdp
                .send(
                  'Network.getResponseBody',
                  { requestId: doc[0] },
                  sessionId
                )
                .catch(() => undefined);
              if (got) break;
            }
          }
          if (!got || 'error' in got) throw error;
          content = got;
        }
      }
      if (content.base64Encoded && !binary) {
        skippedBinary++;
        continue;
      }
      const data = content.base64Encoded
        ? Buffer.from(content.content ?? content.body ?? '', 'base64')
        : String(content.content ?? content.body ?? '');
      const name = `resources/${row.n}-${fileName(c.url, c.type, c.mime)}`;
      Object.assign(row, saveEvidence(dir, name, data));
    } catch (error) {
      row.error = errorMessage(error);
    }
    rows.push(row);
  }
  const index = saveIndex(dir, 'resources-index.json', rows);
  const saved = rows.filter(r => r.file);
  const byType = (t: ResType) => saved.filter(r => r.type === t).length;
  out(
    `[METRIC] PAGE_RESOURCES count=${saved.length} bytes=${saved.reduce((s, r) => s + (r.bytes ?? 0), 0)} ${TYPES.map(t => `${t}=${byType(t)}`).join(' ')} skippedBinary=${skippedBinary} failed=${rows.length - saved.length}`
  );
  showRows(
    rows,
    r =>
      `[RESOURCE] ${r.n} ${r.type} ${r.error ? `unreadable ${r.error}` : `${r.bytes} bytes`} ${r.url}`,
    'RESOURCE',
    index
  );
  if (skippedBinary)
    out(
      `[NOTE] ${skippedBinary} binary resource(s) not saved; pass RES_BINARY=1 to save them`
    );
  if (!url && wantNetwork)
    out(
      '[NOTE] xhr/fetch bodies are not in the resource tree; pass RES_URL to capture them during a fresh load'
    );
  await attached.stop();
  for (const error of [...treeErrors, ...attached.errors])
    out(`[FINDING] PAGE_RESOURCES_COVERAGE_UNAVAILABLE ${error}`);
  for (const row of rows.filter(r => r.error))
    out(
      `[FINDING] PAGE_RESOURCE_UNAVAILABLE ${JSON.stringify({ n: row.n, url: row.url, type: row.type, error: row.error })}`
    );
  out(`[ARTIFACT] PAGE_RESOURCES ${index}`);
  for (const row of saved) out(`[ARTIFACT] RESOURCE_FILE ${dir}/${row.file}`);
  // An unreadable resource is a finding (for example a 404 body that Chrome never cached); a missing tree is a failure.
  if (treeErrors.length || attached.errors.length) process.exitCode = 1;
}
