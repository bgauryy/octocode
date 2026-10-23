#!/usr/bin/env node
// Isolated local fixtures exercise generic CDP plans across representative domains.
// The installed schema inventory is only plan-validated offline here; tests/cdp-all-suite.mjs
// sends every advertised command to Chrome. testedMethods counts methods that reached Chrome.
// Modules under protocol/: fixture-server (HTTP/WebSocket fixtures), harness (transports,
// plans and the check runner) and checks-* (named checks in suite order).
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { createFixtureServer } from './protocol/fixture-server.mjs';
import {
  suite,
  results,
  tested,
  advertised,
  cli,
  artifact,
  check,
} from './protocol/harness.mjs';
import { checks as pageChecks } from './protocol/checks-page.mjs';
import { checks as targetChecks } from './protocol/checks-targets.mjs';
import { checks as isolationChecks } from './protocol/checks-isolation.mjs';
import { checks as contentChecks } from './protocol/checks-content.mjs';
import { checks as staleRefChecks } from './protocol/checks-stale-refs.mjs';
import { checks as domainChecks } from './protocol/checks-domains.mjs';
import { checks as evidenceChecks } from './protocol/checks-evidence.mjs';

const args = process.argv.slice(2),
  get = flag => args[args.indexOf(flag) + 1],
  scripts = join(import.meta.dirname, '../dist/engine');
if (args.includes('--help')) {
  console.log(
    'Usage: protocol-suite.mjs [--only <name>] [--report <JSON file>] [--transport legacy|mcp|typed-cli] [--keep]\nLocal Chrome fixtures: offline schema plan validation, DOM/CSS/AX, browser and worker sessions, interception, debugger/profiler, heap, trace streams, PDF, sockets, storage, emulation and failure evidence. Advertised methods are inventoried separately from methods executed.'
  );
  process.exit(0);
}
if (args.includes('--serve')) {
  const server = createFixtureServer();
  server.listen(0, '127.0.0.1', () => console.log(server.address().port));
  await new Promise(() => {});
}
const work = mkdtempSync(join(tmpdir(), 'octo-protocol-')),
  port = String(9600 + Math.floor(Math.random() * 300));
const fixture = spawn(
  process.execPath,
  [fileURLToPath(import.meta.url), '--serve'],
  { stdio: ['ignore', 'pipe', 'inherit'] }
);
const fixturePort = await new Promise((yes, no) => {
  const timer = setTimeout(
    () => no(Error('Fixture server did not start')),
    5000
  );
  fixture.stdout.once('data', data => {
    clearTimeout(timer);
    yes(String(data).trim());
  });
  fixture.once('error', no);
});
const base = 'http://127.0.0.1:' + fixturePort;
const transport = args.includes('--transport') ? get('--transport') : 'legacy';
if (!['legacy', 'mcp', 'typed-cli'].includes(transport))
  throw Error('Unknown transport ' + transport);
const { connectStdio } =
  transport === 'legacy' ? {} : await import('../dist/runtime.js');
const client =
  transport === 'mcp'
    ? await connectStdio({
        command: process.execPath,
        args: [join(scripts, '../../bin/octocode-chrome-devtools.mjs')],
        cwd: work,
      })
    : null;
Object.assign(suite, {
  work,
  port,
  base,
  fixturePort,
  scripts,
  transport,
  client,
  only: args.includes('--only') ? get('--only') : undefined,
});
try {
  const launch = await cli(['open', '--headless', '--url', 'about:blank']);
  if (launch.code !== 0) throw Error(launch.out);
  const info = JSON.parse(
    launch.out.split('\n').find(line => line.startsWith('{'))
  );
  assert.equal(
    info.reused,
    false,
    'Protocol suite requires an isolated fresh browser'
  );
  suite.browser = info.browser;
  const capture = await cli(['protocol', '--browser']);
  assert.equal(capture.code, 0, capture.out);
  suite.protocol = artifact(capture.out, 'cdp-protocol.json');
  for (const domain of suite.protocol.domains)
    for (const command of domain.commands ?? [])
      advertised.add(domain.domain + '.' + command.name);
  for (const { name, needs, fn } of [
    ...pageChecks,
    ...targetChecks,
    ...isolationChecks,
    ...contentChecks,
    ...staleRefChecks,
    ...domainChecks,
    ...evidenceChecks,
  ])
    await check(name, needs, fn);
} catch (error) {
  results.push({
    name: 'suite setup or unexpected failure',
    status: 'failed',
    error: error.message,
  });
} finally {
  fixture.kill();
  if (!args.includes('--keep')) {
    try {
      const cleanup = await cli(['cleanup'], 15000);
      assert.equal(cleanup.code, 0, cleanup.out);
    } catch (error) {
      results.push({
        name: 'owned browser cleanup',
        status: 'failed',
        error: error.message,
      });
    }
    if (results.some(row => row.status === 'failed'))
      console.log('Retained failed-flow captures ' + work);
    else rmSync(work, { recursive: true, force: true });
  } else console.log('Kept browser port ' + port + ', captures ' + work);
  await client?.close();
}
const failed = results.filter(row => row.status === 'failed'),
  unavailable = results.filter(row => row.status === 'unavailable');
const inventory = {
  transport,
  ...(results.some(row => row.status === 'failed') || args.includes('--keep')
    ? { evidenceRoot: work }
    : {}),
  browser: suite.browser,
  protocolVersion: suite.protocol?.version,
  planValidated: {
    commands: suite.planValidatedCommands,
    events: suite.planValidatedEvents,
  },
  testedMethods: [...tested].sort(),
  domains: (suite.protocol?.domains ?? []).map(domain => ({
    domain: domain.domain,
    commands: (domain.commands ?? []).map(row => ({
      method: domain.domain + '.' + row.name,
      executed: tested.has(domain.domain + '.' + row.name),
    })),
    events: (domain.events ?? []).map(row => domain.domain + '.' + row.name),
  })),
  results,
};
if (args.includes('--report')) {
  const file = resolve(get('--report'));
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify(inventory, null, 2) + '\n');
  console.log('[ARTIFACT] protocol-coverage.json ' + file);
  console.log(
    '[NEXT] ' +
      JSON.stringify({
        continue: {
          command: process.execPath,
          args: [
            join(scripts, 'artifact-query.mjs'),
            '--file',
            file,
            '--format',
            'json',
          ],
        },
      })
  );
}
console.log(
  JSON.stringify({
    ok: failed.length === 0,
    suite: 'chrome-cdp-protocol',
    transport,
    browser: suite.browser,
    domains: suite.protocol?.domains.length,
    advertisedCommands: advertised.size,
    planValidatedCommands: suite.planValidatedCommands,
    planValidatedEvents: suite.planValidatedEvents,
    testedMethods: tested.size,
    passed: results.length - failed.length - unavailable.length,
    failed: failed.length,
    unavailable: unavailable.length,
  })
);
process.exitCode = failed.length ? 1 : 0;
