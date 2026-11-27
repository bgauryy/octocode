/**
 * cdp-domain: enable one CDP domain of the installed Chrome, dump its parameterless getters and/or
 * record its events into files. Results and events are saved whole (never printed): stdout carries
 * counts, notes and findings only. Calls go through the scope router, so browser-level getters
 * (SystemInfo.*) work from a page session. Options: CDPDOM_DOMAIN (required), CDPDOM_MODE,
 * CDPDOM_MS, CDPDOM_METHODS, CDPDOM_URL.
 */
import { appendFileSync } from 'node:fs';
import type { Handler } from '../cdp-connection.mjs';
import { connectCDP } from '../cdp-connection.mjs';
import { policyEntry, twinEntries } from '../cdp-policy.mjs';
import type { CheckSession } from '../cdp-runner-session.mjs';
import { enableInfo, loadSchema, nearestDomains } from '../cdp-schema.mjs';
import type { Domain } from '../cdp-schema.mjs';
import { BrowserHub, CdpRouter } from '../cdp-scope.mjs';
import { cdpHttp } from '../engine-io.mjs';
import { errorMessage } from '../error-message.mjs';
import { hazardOf } from './cdp-domain-hazards.mjs';
import { navigateAndWait, out, saveEvidence } from './evidence-io.mjs';

const env = process.env;
const MODES = ['dump', 'events', 'both'] as const;
type Mode = (typeof MODES)[number];
const MIN_MS = 100;
const DEFAULT_MS = 2000;
/** A result above this many bytes goes to its own file; the aggregate file holds the smaller ones. */
export const SEPARATE_FILE_BYTES = 64 * 1024;
/** The deadline of one getter: a call that does not answer is recorded as a timeout and the dump goes on. */
const CALL_DEADLINE_MS = 10_000;
const GETTER = /^(get|is|can|request|list|query)[A-Z]/;

const invalid = (error: string) => {
  out(`[FINDING] CDP_DOMAIN_INVALID ${JSON.stringify({ error })}`);
  process.exitCode = 2;
};

type Options = {
  domain: string;
  mode: Mode;
  ms: number;
  methods: string[];
  url?: string;
};

function parseOptions(): Options | string {
  const domain = env.CDPDOM_DOMAIN ?? '';
  if (!/^[A-Z][A-Za-z0-9]*$/.test(domain))
    return 'CDPDOM_DOMAIN is required: a domain name such as Performance';
  const mode = env.CDPDOM_MODE || 'both';
  if (!MODES.some(item => item === mode))
    return `CDPDOM_MODE must be ${MODES.join('|')}`;
  const ms = env.CDPDOM_MS ? Number(env.CDPDOM_MS) : DEFAULT_MS;
  if (!Number.isSafeInteger(ms) || ms < MIN_MS)
    return `CDPDOM_MS must be a whole number of at least ${MIN_MS}`;
  let methods: string[] = [];
  if (env.CDPDOM_METHODS) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(env.CDPDOM_METHODS);
    } catch {
      return 'CDPDOM_METHODS must be a JSON array of command names';
    }
    if (
      !Array.isArray(parsed) ||
      !parsed.length ||
      !parsed.every(
        (name): name is string =>
          typeof name === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(name)
      )
    )
      return 'CDPDOM_METHODS must be a non-empty JSON array of command names';
    methods = parsed;
  }
  return {
    domain,
    mode: mode as Mode,
    ms,
    methods,
    ...(env.CDPDOM_URL ? { url: env.CDPDOM_URL } : {}),
  };
}

/** The loopback port of the attached Chrome: from the target's own socket URL, else `--port`. */
function portOf(cdp: CheckSession): string | undefined {
  const url: unknown = cdp.targetInfo?.webSocketDebuggerUrl;
  if (typeof url === 'string') {
    try {
      const port = new URL(url).port;
      if (port) return port;
    } catch {
      // fall through to --port
    }
  }
  const at = process.argv.indexOf('--port');
  return at < 0 ? undefined : process.argv[at + 1];
}

export type IndexRow = {
  method: string;
  status: 'ok' | 'error' | 'timeout' | 'skipped';
  reason?: string;
  bytes: number;
  file?: string;
  scope?: 'page' | 'browser';
};

type Command = NonNullable<Domain['commands']>[number];
const parameterless = (command: Command) =>
  !(command.parameters ?? []).some(param => !param.optional);

/** The methods one dump covers, in schema order, then the caller's extras that are not already in. */
function candidates(domain: Domain, extra: string[]) {
  const named = new Map((domain.commands ?? []).map(c => [c.name, c]));
  const picked = (domain.commands ?? [])
    .filter(c => GETTER.test(c.name) && parameterless(c))
    .map(c => c.name);
  return [...new Set([...picked, ...extra])].map(name => ({
    name,
    command: named.get(name),
  }));
}

async function dumpDomain(
  cdp: CheckSession,
  router: CdpRouter,
  domain: Domain,
  options: Options,
  twins: ReturnType<typeof twinEntries>,
  setDeadline: (ms: number) => void
) {
  const small: Record<string, unknown> = {};
  const rows: IndexRow[] = [];
  for (const { name, command } of candidates(domain, options.methods)) {
    const method = `${domain.domain}.${name}`;
    const skip = (reason: string) =>
      rows.push({ method, status: 'skipped', reason, bytes: 0 });
    if (!command) {
      skip('not a command of this domain in the installed Chrome');
      continue;
    }
    const hazard = hazardOf(method);
    if (hazard) {
      skip(`hazard: ${hazard.reason}`);
      continue;
    }
    if (!parameterless(command)) {
      skip('has required parameters');
      continue;
    }
    const policy = policyEntry(method, twins);
    if (policy) {
      skip(`needsAck: ${policy.reason}`);
      continue;
    }
    setDeadline(CALL_DEADLINE_MS);
    try {
      const { result, scope } = await router.route(method, {}, 'auto');
      const text = JSON.stringify(result ?? {});
      const bytes = Buffer.byteLength(text);
      if (bytes > SEPARATE_FILE_BYTES) {
        const file = `domain-${domain.domain}/${name}.json`;
        saveEvidence(cdp.outputDir, file, text);
        rows.push({ method, status: 'ok', bytes, file, scope });
      } else {
        small[method] = result ?? {};
        rows.push({ method, status: 'ok', bytes, scope });
      }
    } catch (error) {
      const reason = errorMessage(error);
      rows.push({
        method,
        status: /CDP timeout/.test(reason) ? 'timeout' : 'error',
        reason,
        bytes: 0,
      });
    }
  }
  return { small, rows };
}

export async function run(cdp: CheckSession) {
  const options = parseOptions();
  if (typeof options === 'string') return invalid(options);
  const port = portOf(cdp);
  if (port === undefined)
    return invalid(
      'cannot find the Chrome port: pass --port (the schema of the installed Chrome validates the domain)'
    );
  const schema = await loadSchema(port);
  const domain = schema.domains.find(d => d.domain === options.domain);
  if (!domain) {
    const nearest = nearestDomains(schema, options.domain);
    out(
      `[FINDING] CDP_DOMAIN_UNKNOWN ${JSON.stringify({ domain: options.domain, nearest, hint: 'protocol * lists the domains of the installed Chrome' })}`
    );
    process.exitCode = 1;
    return;
  }
  const name = domain.domain;
  const events = (domain.events ?? []).map(event => event.name);
  const wantDump = options.mode !== 'events';
  const wantEvents = options.mode !== 'dump';

  let deadline = CALL_DEADLINE_MS;
  const hub = new BrowserHub(async () => {
    const version = await cdpHttp(port, '/json/version');
    const url: unknown = version?.webSocketDebuggerUrl;
    if (typeof url !== 'string')
      throw new Error(`Chrome on port ${port} gave no browser WebSocket URL`);
    const connection = await connectCDP(url, {
      targetInfo: { type: 'browser' },
    });
    return {
      send: (method, params) =>
        connection.send(method, params, undefined, { timeoutMs: deadline }),
      close: () => connection.close(),
    };
  });
  const router = new CdpRouter({
    schema,
    page: (method, params) =>
      cdp.send(method, params, undefined, { timeoutMs: deadline }),
    pageIsBrowser: cdp.targetInfo?.type === 'browser',
    hub,
    log: out,
  });

  const counts = new Map<string, number>();
  const eventsFile = `domain-${name}.jsonl`;
  const listeners: [string, Handler][] = [];
  try {
    if (wantEvents && events.length) {
      saveEvidence(cdp.outputDir, eventsFile, '');
      const path = `${cdp.outputDir}/${eventsFile}`;
      for (const event of events) {
        const method = `${name}.${event}`;
        const handler: Handler = (
          params,
          meta: { sessionId?: string } = {}
        ) => {
          counts.set(method, (counts.get(method) ?? 0) + 1);
          appendFileSync(
            path,
            `${JSON.stringify({
              ts: new Date().toISOString(),
              method,
              params,
              ...(meta?.sessionId ? { sessionId: meta.sessionId } : {}),
            })}\n`
          );
        };
        listeners.push([method, handler]);
        cdp.on(method, handler);
      }
    } else if (wantEvents)
      out(`[NOTE] ${name} declares no events in this Chrome`);

    // Enable before any load: Network, Page, DOM and DOMStorage events are not replayed on a late enable.
    const info = enableInfo(domain);
    if (name === 'Fetch')
      out(
        '[NOTE] Fetch.enable pauses requests and is never sent automatically; enable it yourself with patterns'
      );
    else if (info.available)
      try {
        const { enabled, notes } = await router.enable([name]);
        if (enabled.length) out(`[NOTE] enabled ${enabled.join(', ')}`);
        for (const note of notes) out(`[NOTE] ${note}`);
      } catch (error) {
        out(
          `[FINDING] CDP_DOMAIN_ENABLE_FAILED ${JSON.stringify({ domain: name, error: errorMessage(error) })}`
        );
      }
    const windowStart = Date.now();

    if (options.url) {
      try {
        const nav = await navigateAndWait(cdp, options.url);
        if (nav.errorText)
          out(
            `[FINDING] CDP_DOMAIN_NAVIGATION_FAILED ${JSON.stringify({ url: options.url, errorText: nav.errorText })}`
          );
        else if (!nav.loaded)
          out(`[NOTE] ${options.url} did not fire its load event in time`);
      } catch (error) {
        out(
          `[FINDING] CDP_DOMAIN_NAVIGATION_FAILED ${JSON.stringify({ url: options.url, errorText: errorMessage(error) })}`
        );
      }
    }

    let rows: IndexRow[] = [];
    if (wantDump) {
      const dumped = await dumpDomain(
        cdp,
        router,
        domain,
        options,
        twinEntries(schema),
        ms => {
          deadline = ms;
        }
      );
      rows = dumped.rows;
      const file = `domain-${name}.json`;
      saveEvidence(
        cdp.outputDir,
        file,
        `${JSON.stringify(dumped.small, null, 2)}\n`
      );
      saveEvidence(
        cdp.outputDir,
        `domain-${name}-index.json`,
        `${JSON.stringify(rows, null, 2)}\n`
      );
      out(`[ARTIFACT] CDP_DOMAIN_DUMP ${cdp.outputDir}/${file}`);
      out(
        `[ARTIFACT] CDP_DOMAIN_INDEX ${cdp.outputDir}/domain-${name}-index.json (method, status, bytes and file per getter)`
      );
      if (rows.some(row => row.file))
        out(
          `[ARTIFACT] CDP_DOMAIN_FILES ${cdp.outputDir}/domain-${name}/ (results over ${SEPARATE_FILE_BYTES} bytes, one file per method)`
        );
      for (const row of rows)
        if (row.status === 'error' || row.status === 'timeout')
          out(
            `[FINDING] CDP_DOMAIN_METHOD_FAILED ${JSON.stringify({ method: row.method, status: row.status, error: row.reason })}`
          );
    }

    if (wantEvents && events.length) {
      const remaining = options.ms - (Date.now() - windowStart);
      if (remaining > 0)
        await new Promise(resolve => setTimeout(resolve, remaining));
      for (const [event, count] of counts)
        out(`[EVENT] ${JSON.stringify({ event, count })}`);
      const silent = events.filter(event => !counts.has(`${name}.${event}`));
      if (silent.length)
        out(
          `[NOTE] no events in ${options.ms} ms: ${silent.map(event => `${name}.${event}`).join(', ')}`
        );
      out(
        `[ARTIFACT] CDP_DOMAIN_EVENTS ${cdp.outputDir}/${eventsFile} (one event per line, params whole; may contain tokens, treat like HAR bodies)`
      );
    }

    if (wantDump && !rows.length)
      out(
        `[NOTE] ${name} has no parameterless getters; use CDPDOM_MODE=events or \`protocol ${name}\` for its commands`
      );

    const total = (status: IndexRow['status']) =>
      rows.filter(row => row.status === status).length;
    const eventTotal = [...counts.values()].reduce((a, b) => a + b, 0);
    out(
      `[METRIC] CDP_DOMAIN domain=${name} methods=${rows.length} ok=${total('ok')} skipped=${total('skipped')} events=${eventTotal} failed=${total('error') + total('timeout')} mode=${options.mode}`
    );
    // The per-event counts ride in the reply summary: which events fired is the first thing an agent asks.
    if (counts.size)
      out(
        `[METRIC] CDP_DOMAIN_EVENTS ${[...counts]
          .toSorted((x, y) => y[1] - x[1])
          .slice(0, 8)
          .map(([event, count]) => `${event}=${count}`)
          .join(' ')}`
      );
  } finally {
    for (const [method, handler] of listeners) cdp.off(method, handler);
    hub.close();
  }
}
