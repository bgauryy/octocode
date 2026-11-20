// Shared real-Chrome harness for the capture recipes: a loopback page server, an isolated headless
// Chrome on a free port in a temporary workspace, and the engine's /cli entry.
import { execFile } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { promisify } from 'node:util';
import { freePort } from '../fixtures/free-port.mjs';

export const bin = join(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
export const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

export const engineOpenEntry = join(
  import.meta.dirname,
  '../../dist/engine/open-browser.mjs'
);

const runnerEntry = join(
  import.meta.dirname,
  '../../dist/engine/cdp-runner.mjs'
);
const checksDir = join(import.meta.dirname, '../../dist/engine/cdp-checks');
const recipeFiles = {
  coverage: 'coverage-check',
  'trace-capture': 'trace-capture-check',
  'heap-snapshot': 'heap-snapshot-check',
  'page-pdf': 'page-pdf-check',
};

export const fixtureFiles = {
  '/': [
    'text/html',
    `<!doctype html><title>cov</title><link rel="stylesheet" href="/app.css"><div class="used">hello</div><script src="/app.js"></script>`,
  ],
  '/app.js': [
    'text/javascript',
    `function usedFn() { return 1; }\nfunction unusedFn() { return 2; }\nusedFn();\n`,
  ],
  '/app.css': [
    'text/css',
    `.used { color: red; }\n.never-matched { color: blue; }\n`,
  ],
};

/** Starts the page server, a headless Chrome and returns cli/check helpers; call stop() when done. */
export async function startHarness(
  files = fixtureFiles,
  openInput = {},
  engineOpenArgs = null
) {
  const cwd = mkdtempSync(join(tmpdir(), 'octo-capture-'));
  const port = await freePort();
  const server = await new Promise(resolve => {
    const s = createServer((req, res) => {
      const row = files[req.url.split('?')[0]];
      if (!row) {
        res.statusCode = 404;
        res.end();
        return;
      }
      res.setHeader('content-type', row[0]);
      res.end(row[1]);
    }).listen(0, '127.0.0.1', () => resolve(s));
  });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const cli = async (command, input) => {
    writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
    const r = await promisify(execFile)(
      process.execPath,
      [bin, '/cli', command, '--input', 'in.json', '--json'],
      { cwd, encoding: 'utf8' }
    ).catch(error => error);
    const parsed = JSON.parse(r.stdout || r.stderr);
    return parsed.structuredContent ?? parsed;
  };
  // Recipes run through the runner with the options as env values (booleans 1/0, arrays JSON), the
  // same mapping `check` applies; the sandbox's env allowlist is a separate layer under its own tests.
  const check = async (recipe, options) => {
    const env = {
      ...process.env,
      ...Object.fromEntries(
        Object.entries(options).map(([key, value]) => [
          key,
          typeof value === 'boolean'
            ? value
              ? '1'
              : '0'
            : typeof value === 'object'
              ? JSON.stringify(value)
              : String(value),
        ])
      ),
    };
    const r = await promisify(execFile)(
      process.execPath,
      [
        runnerEntry,
        join(checksDir, `${recipeFiles[recipe]}.mjs`),
        '--port',
        String(port),
      ],
      { cwd, encoding: 'utf8', env }
    ).then(
      done => ({ ...done, code: 0 }),
      error => ({
        stdout: error.stdout ?? '',
        stderr: error.stderr ?? '',
        code: error.code,
      })
    );
    const artifacts = Object.fromEntries(
      [...r.stdout.matchAll(/^\[ARTIFACT\] (\S+) (\/.+)$/gm)].map(m => [
        m[1],
        m[2],
      ])
    );
    return {
      out: { ok: r.code === 0 },
      stdout: r.stdout,
      stderr: r.stderr,
      artifacts,
    };
  };
  // engineOpenArgs launches through the engine's open-browser.mjs directly (its own argv).
  const opened = engineOpenArgs
    ? await promisify(execFile)(
        process.execPath,
        [
          engineOpenEntry,
          '--port',
          String(port),
          '--headless',
          ...engineOpenArgs,
        ],
        { cwd, encoding: 'utf8' }
      ).then(
        r => ({ ok: true, ...JSON.parse(r.stdout) }),
        r => ({ ok: false, error: r.stdout + r.stderr })
      )
    : await cli('open', {
        port,
        headless: true,
        url: 'about:blank',
        ...openInput,
      });
  return {
    cwd,
    port,
    origin,
    cli,
    check,
    engineOpen: (...args) =>
      promisify(execFile)(
        process.execPath,
        [engineOpenEntry, '--port', String(port), ...args],
        { cwd, encoding: 'utf8' }
      ).then(
        r => ({ ok: true, ...JSON.parse(r.stdout) }),
        r => ({ ok: false, error: r.stdout + r.stderr })
      ),
    opened,
    async stop() {
      await cli('cleanup', { port });
      server.close();
      rmSync(cwd, { recursive: true, force: true });
    },
  };
}
