import assert from 'node:assert/strict';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
  realpathSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:net';
import { spawnSync, execFileSync } from 'node:child_process';
import { connectStdio } from '../dist/runtime.js';
const root = dirname(dirname(fileURLToPath(import.meta.url))),
  work = realpathSync(mkdtempSync(join(tmpdir(), 'octo-chrome-release-')));
const reserve = createServer();
await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
const port = reserve.address().port;
await new Promise(resolve => reserve.close(resolve));
let client,
  launched = false;
const content = result => {
  assert(!result.isError, result.content?.[0]?.text);
  assert(result.structuredContent.ok);
  return result.structuredContent;
};
const stdout = result =>
  result.data !== undefined
    ? JSON.stringify(result.data)
    : readFileSync(
        (
          result.logs ??
          JSON.parse(readFileSync(result.capture.file, 'utf8')).logs
        ).stdout,
        'utf8'
      );
const fixtureUrl =
  'data:text/html,' +
  encodeURIComponent(
    '<title>Research fixture</title><label>Query <input id="q"></label><button onclick="document.querySelector(\'output\').textContent=\'Packed pass\'">Go</button><output>Ready</output><p>Source evidence: orbital period 365 days.</p><a href="https://example.com/evidence">Supporting source</a>'
  );
try {
  const pack = spawnSync(
    'npm',
    ['pack', '--ignore-scripts', '--json', '--pack-destination', work],
    { cwd: root, encoding: 'utf8' }
  );
  assert.equal(pack.status, 0, pack.stderr);
  const [manifest] = JSON.parse(pack.stdout),
    extracted = join(work, 'extracted');
  mkdirSync(extracted);
  execFileSync('tar', ['-xzf', join(work, manifest.filename), '-C', extracted]);
  const bin = join(extracted, 'package/bin/octocode-chrome-devtools.mjs');
  client = await connectStdio({
    command: process.execPath,
    args: [bin],
    cwd: work,
  });
  const open = content(
    await client.callTool({
      name: 'open',
      arguments: {
        args: ['--headless', '--port', String(port), '--url', 'about:blank'],
      },
    })
  );
  const ready = JSON.parse(stdout(open));
  assert.equal(ready.reused, false);
  launched = true;
  const response = content(
    await client.callTool({
      name: 'run',
      arguments: {
        connection: { port, newTab: 'about:blank', closeTab: true },
        plan: {
          steps: [
            { op: 'goto', url: fixtureUrl, after: { selector: '#q' } },
            {
              op: 'act',
              role: 'textbox',
              name: 'Query',
              action: 'fill',
              value: 'archive test',
            },
            {
              op: 'act',
              role: 'button',
              name: 'Go',
              action: 'click',
              after: { text: 'Packed pass' },
            },
            { op: 'extract', selector: 'output', fields: ['text'] },
          ],
        },
      },
    })
  );
  const line = stdout(response)
    .split('\n')
    .find(line => line.startsWith('[ARTIFACT] browser-result.json '));
  assert(line);
  const result = JSON.parse(
    readFileSync(line.slice('[ARTIFACT] browser-result.json '.length), 'utf8')
  );
  assert(result.ok);
  assert.equal(result.completedSteps, 4);
  assert.deepEqual(JSON.parse(readFileSync(result.steps[3].artifact, 'utf8')), [
    { text: 'Packed pass' },
  ]);
  const cli = spawnSync(
    process.execPath,
    [
      bin,
      '/cli',
      'cdp',
      '--method',
      'Runtime.evaluate',
      '--params',
      '{"expression":"40+2","returnByValue":true}',
      '--connection',
      JSON.stringify({ port, newTab: 'about:blank', closeTab: true }),
      '--json',
    ],
    { cwd: work, encoding: 'utf8', timeout: 30000 }
  );
  assert.equal(cli.status, 0, cli.stderr);
  const evaluated = JSON.parse(cli.stdout).structuredContent,
    line2 = stdout(evaluated)
      .split('\n')
      .find(line => line.startsWith('[ARTIFACT] cdp-1.json '));
  assert(line2);
  assert.equal(
    JSON.parse(
      readFileSync(line2.slice('[ARTIFACT] cdp-1.json '.length), 'utf8')
    ).result.value,
    42
  );
  let guide = content(
      await client.callTool({
        name: 'skill',
        arguments: { topic: 'web-research', length: 1000 },
      })
    ).data,
    text = guide.content;
  while (guide.next) {
    guide = content(
      await client.callTool({
        name: guide.next.tool,
        arguments: guide.next.query,
      })
    ).data;
    text += guide.content;
  }
  const research = JSON.parse(text.match(/```json\n([\s\S]*?)\n```/)[1]);
  research.connection = { port, newTab: 'about:blank', closeTab: true };
  research.plan.steps[0].url = fixtureUrl;
  const researchFile = join(work, 'research.json');
  writeFileSync(researchFile, JSON.stringify(research));
  for (const transport of ['mcp', 'cli']) {
    let capture;
    if (transport === 'mcp')
      capture = content(
        await client.callTool({ name: 'run', arguments: research })
      );
    else {
      const run = spawnSync(
        process.execPath,
        [bin, '/cli', 'run', '--input', researchFile, '--json'],
        { cwd: work, encoding: 'utf8', timeout: 30000 }
      );
      assert.equal(run.status, 0, run.stderr);
      capture = JSON.parse(run.stdout).structuredContent;
    }
    const path = stdout(capture)
      .split('\n')
      .find(line => line.startsWith('[ARTIFACT] browser-result.json '));
    assert(path);
    const evidence = JSON.parse(
      readFileSync(path.slice('[ARTIFACT] browser-result.json '.length), 'utf8')
    );
    assert(evidence.ok);
    assert.equal(evidence.completedSteps, 4);
    const metadata = JSON.parse(
      readFileSync(evidence.steps[1].artifact, 'utf8')
    ).result.value;
    assert.equal(metadata.url, fixtureUrl);
    assert.equal(metadata.title, 'Research fixture');
    assert(Number.isFinite(Date.parse(metadata.capturedAt)));
    assert.match(
      JSON.parse(readFileSync(evidence.steps[2].artifact, 'utf8'))[0].text,
      /orbital period 365 days/
    );
    assert.deepEqual(
      JSON.parse(readFileSync(evidence.steps[3].artifact, 'utf8')),
      [{ text: 'Supporting source', href: 'https://example.com/evidence' }]
    );
  }
  const cookieDir = join(work, '.octocode/tmp/chrome-devtools/cookie-smoke');
  mkdirSync(cookieDir, { recursive: true });
  const cookieSource = join(cookieDir, 'source.json'),
    cookieExport = join(cookieDir, 'export.json');
  writeFileSync(
    cookieSource,
    JSON.stringify({
      cookies: [
        {
          name: 'research_fixture',
          value: 'synthetic_value',
          domain: 'research.invalid',
          path: '/',
          expires: -1,
          httpOnly: false,
          secure: false,
          sameSite: 'Lax',
        },
      ],
      origins: [],
    }),
    { mode: 0o600 }
  );
  const injected = content(
    await client.callTool({
      name: 'cookies',
      arguments: {
        args: [
          '--i-understand-secrets',
          '--from-storage-state',
          cookieSource,
          '--to-port',
          String(port),
        ],
      },
    })
  );
  assert(!stdout(injected).includes('synthetic_value'));
  const exported = content(
    await client.callTool({
      name: 'cookies',
      arguments: {
        args: [
          '--i-understand-secrets',
          '--from-port',
          String(port),
          '--export-storage-state',
          cookieExport,
        ],
      },
    })
  );
  assert(!stdout(exported).includes('synthetic_value'));
  assert.equal(
    JSON.parse(readFileSync(cookieExport, 'utf8')).cookies.find(
      cookie => cookie.name === 'research_fixture'
    ).value,
    'synthetic_value'
  );
  console.log(
    JSON.stringify({
      ok: true,
      suite: 'chrome-extracted-package-live',
      browser: ready.browser,
      commands: (await client.listTools()).tools.length,
      planSteps: 4,
      typedCliValue: 42,
      researchGuideTransports: ['mcp', 'cli'],
      syntheticCookieRoundTrip: true,
    })
  );
} finally {
  try {
    if (launched)
      content(
        await client.callTool({
          name: 'cleanup',
          arguments: { args: ['--port', String(port)] },
        })
      );
  } finally {
    await client?.close();
    rmSync(work, { recursive: true, force: true });
  }
}
