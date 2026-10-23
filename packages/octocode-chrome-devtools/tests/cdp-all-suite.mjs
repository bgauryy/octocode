#!/usr/bin/env node
// Sends every command the installed Chrome advertises through the package's public
// engine CLI and classifies each reply. Params come from the schema alone: required
// fields, type placeholders, first enum value, resolved $ref. Destructive commands are
// survived by replacing the target or relaunching the browser, never skipped by name.
// Modules under cdp-all/: browser (engine CLI and isolated Chrome), schema (placeholder
// params and reply classes) and sweep (per-target queues and loss attribution).
import { createServer } from 'node:net';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import {
  artifact,
  counters,
  engine,
  ensureBrowser,
  launch,
  suite,
} from './cdp-all/browser.mjs';
import { placeholders } from './cdp-all/schema.mjs';
import { options, scopes, selectionFor, sweep } from './cdp-all/sweep.mjs';

const args = process.argv.slice(2),
  get = (flag, fallback) =>
    args.includes(flag) ? args[args.indexOf(flag) + 1] : fallback;
if (args.includes('--help')) {
  console.log(`Usage: cdp-all-suite.mjs [--via script|run] [--batch <n>] [--command-ms <ms>] [--only <Domain>] [--report <JSON file>] [--keep]
Sends every advertised CDP command to an isolated headless Chrome (page target; then the
browser, tab and dedicated worker targets, each for what the previous one answered -32601)
and classifies each reply:
  ok, chrome-error (Chrome routed and rejected the call), exception (reply carried
  exceptionDetails), timeout (no reply within --command-ms), target-destroyed /
  browser-destroyed (loss reproduced by replay; the suite relaunches and continues),
  unavailable (-32601 on every target for an experimental or deprecated member),
  not-found (-32601 on a stable member), transport (unreproduced connection loss),
  engine (the package failed without a Chrome reply).
not-found, transport and engine fail the suite.
--via script (default) sends the queue over one long-lived sandboxed connection with the
script command; --via run sends batched run plans of cdp steps, resuming after each stop.
Events: one run plan registers a listen step for every advertised event.`);
  process.exit(0);
}
const via = get('--via', 'script'),
  batchSize = Number(get('--batch', '120')),
  commandMs = Number(get('--command-ms', '3000')),
  only = get('--only'),
  engineDir = join(import.meta.dirname, '../dist/engine');
if (!['run', 'script'].includes(via)) throw Error('--via needs script or run');
const failing = new Set(['not-found', 'transport', 'engine']);
// Classes that mean no target knew the command.
const notFound = new Set(['not-found', 'unavailable']);
const work = mkdtempSync(join(tmpdir(), 'octo-cdp-all-'));
const port = await new Promise((yes, no) => {
  const server = createServer().listen(0, '127.0.0.1', () => {
    const { port } = server.address();
    server.close(() => yes(String(port)));
  });
  server.once('error', no);
});
Object.assign(suite, { work, port, engineDir });
Object.assign(options, { via, batchSize, commandMs });

const started = Date.now();
let report;
try {
  const info = await launch(true);
  const capture = await engine(['protocol', '--browser', '--port', port]);
  const protocol = JSON.parse(
    readFileSync(artifact(capture.out, 'cdp-protocol.json'), 'utf8')
  );
  const params = placeholders(protocol);
  const domains = protocol.domains.filter(
    domain => !only || domain.domain === only
  );
  const queue = domains.flatMap(domain =>
    (domain.commands ?? []).map(command => ({
      method: domain.domain + '.' + command.name,
      params: params(domain, command),
      stable: !(
        domain.experimental ||
        domain.deprecated ||
        command.experimental ||
        command.deprecated
      ),
    }))
  );
  const events = domains.flatMap(domain =>
    (domain.events ?? []).map(entry => domain.domain + '.' + entry.name)
  );

  const sweepStarted = Date.now();
  // Each target answers what the previous ones did not know (-32601): the browser target
  // owns browser-level domains, a tab target owns tab-scoped ones, workers their own.
  const finals = new Array(queue.length);
  let remaining = queue.map((_row, index) => index);
  const tried = [];
  for (const scope of Object.keys(scopes)) {
    if (!remaining.length) break;
    tried.push(scope);
    const outcome = await sweep(
      remaining.map(index => queue[index]),
      scope
    );
    remaining = remaining.filter((index, position) => {
      const row = outcome[position];
      if (scopes[scope].silentUnknown && row.class === 'timeout') {
        finals[index] = { ...finals[index], tried: [...tried] };
        return true;
      }
      finals[index] = { target: scope, tried: [...tried], ...row };
      return row.class === 'not-found';
    });
  }
  const sweepMs = Date.now() - sweepStarted;
  const methods = queue.map((row, index) => {
    const { index: _index, ...final } = finals[index];
    if (final.class === 'not-found' && !row.stable) final.class = 'unavailable';
    return {
      method: row.method,
      params: row.params,
      ...final,
      ...(final.error ? { error: final.error.slice(0, 300) } : {}),
    };
  });

  // Events: register a listen step for every advertised event, then enable every domain
  // whose parameterless enable answered ok on the page, so subscriptions see traffic.
  await ensureBrowser();
  const enables = methods.filter(
    row =>
      row.class === 'ok' &&
      row.target === 'page' &&
      row.method.endsWith('.enable') &&
      !Object.keys(row.params).length
  );
  const planFile = join(work, 'events-plan.json');
  writeFileSync(
    planFile,
    JSON.stringify({
      commandMs,
      waitMs: commandMs,
      steps: [
        ...events.map((event, index) => ({
          op: 'listen',
          id: 'e' + index,
          event,
        })),
        ...enables.map(row => ({ op: 'cdp', method: row.method, params: {} })),
      ],
    })
  );
  const eventRun = await engine([
    'run',
    ...selectionFor('page'),
    '--port',
    port,
    '--plan',
    planFile,
  ]);
  const eventResultFile = artifact(eventRun.out, 'browser-result.json');
  const eventResult = eventResultFile
    ? JSON.parse(readFileSync(eventResultFile, 'utf8'))
    : {
        ok: false,
        eventCoverage: [],
        failure: { error: eventRun.out.slice(-600) },
      };
  const coverage = new Map(
    eventResult.eventCoverage.map(row => [row.event, row])
  );
  const eventRows = events.map(event => ({
    event,
    subscribed: coverage.has(event),
    observed: coverage.get(event)?.observed ?? 0,
  }));

  const totals = {};
  for (const row of methods) totals[row.class] = (totals[row.class] ?? 0) + 1;
  const failures = methods.filter(row => failing.has(row.class));
  const unsubscribed = eventRows.filter(row => !row.subscribed);
  report = {
    ok: !failures.length && !unsubscribed.length && eventResult.ok,
    suite: 'chrome-cdp-all',
    via,
    browser: info.browser,
    protocolVersion: protocol.version,
    elapsedMs: Date.now() - started,
    sweepMs,
    ...counters,
    commands: {
      advertised: queue.length,
      sent: methods.length,
      totals,
      byTarget: Object.fromEntries(
        Object.keys(scopes).map(scope => [
          scope,
          {
            tried: methods.filter(row => row.tried.includes(scope)).length,
            answered: methods.filter(
              row => row.target === scope && !notFound.has(row.class)
            ).length,
          },
        ])
      ),
      failures: failures.length,
    },
    events: {
      advertised: events.length,
      subscribed: eventRows.length - unsubscribed.length,
      observed: eventRows.filter(row => row.observed).length,
      enablesSent: enables.length,
      planOk: eventResult.ok,
      ...(eventResult.ok ? {} : { failure: eventResult.failure }),
    },
    failures,
    methods,
    eventRows,
  };
} finally {
  await engine(['cleanup', '--port', port], 30000).catch(() => {});
  if (!args.includes('--keep')) rmSync(work, { recursive: true, force: true });
}
const file = resolve(
  get('--report', join(tmpdir(), 'octocode-cdp-all-report.json'))
);
mkdirSync(dirname(file), { recursive: true });
writeFileSync(file, JSON.stringify(report, null, 2) + '\n');
console.log('[ARTIFACT] cdp-all-report.json ' + file);
for (const row of report.failures)
  console.log(
    'FAIL ' + row.method + ' ' + row.class + ': ' + (row.error ?? '')
  );
const { methods: _methods, eventRows: _events, failures, ...summary } = report;
console.log(
  JSON.stringify({ ...summary, failures: failures.map(row => row.method) })
);
process.exitCode = report.ok ? 0 : 1;
