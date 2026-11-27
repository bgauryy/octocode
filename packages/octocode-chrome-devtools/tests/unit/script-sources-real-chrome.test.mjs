// script-sources and source-map against a real isolated headless Chrome and a loopback server.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { promisify } from 'node:util';
import { handMap } from './source-map-fixture.mjs';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const sha = text => createHash('sha256').update(text).digest('hex');
const mapWithEscape = {
  ...handMap,
  sources: ['../../etc/passwd', 'src/b.ts'],
};
const dataMap = `data:application/json;base64,${Buffer.from(JSON.stringify(handMap)).toString('base64')}`;

test(
  'script-sources saves every served script exactly; source-map maps positions, saves sources safely, refuses foreign maps',
  { skip, timeout: 240000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-scripts-')));
    const port = await freePort();
    let origin = '';
    // A Map: a request path names only a file listed here, never an inherited property.
    const files = new Map(
      Object.entries({
        '/app.js': `function foo(){return 1}\nfoo();\n//# sourceMappingURL=app.js.map\n`,
        '/lib.js': `window.lib = "é✓";\n`,
        '/data.js': `window.d = 1;\n//# sourceMappingURL=${dataMap}\n`,
        '/foreign.js': () =>
          `window.f = 1;\n//# sourceMappingURL=http://localhost:${new URL(origin).port}/app.js.map\n`,
        '/app.js.map': JSON.stringify(mapWithEscape),
      })
    );
    const page = `<!doctype html><title>t</title>
<script src="/app.js"></script><script src="/lib.js"></script><script src="/data.js"></script><script src="/foreign.js"></script>
<script>window.inlineMarker = 1;</script>`;
    const server = await new Promise(resolve => {
      const s = createServer((req, res) => {
        const path = req.url.split('?')[0];
        if (path === '/') {
          res.setHeader('content-type', 'text/html');
          res.end(page);
          return;
        }
        const body = files.get(path);
        if (body === undefined) {
          res.statusCode = 404;
          res.end();
          return;
        }
        res.setHeader(
          'content-type',
          path.endsWith('.map')
            ? 'application/json'
            : 'text/javascript; charset=utf-8'
        );
        res.end(typeof body === 'function' ? body() : body);
      }).listen(0, '127.0.0.1', () => resolve(s));
    });
    origin = `http://127.0.0.1:${server.address().port}`;
    const served = {};
    for (const [p, b] of files)
      if (p.endsWith('.js')) served[p] = typeof b === 'function' ? b() : b;

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
    // The recipe runs under the runner directly: the sandbox env allowlist is owned elsewhere.
    let runs = 0;
    const check = async (recipe, options) => {
      const file = {
        'script-sources': 'script-sources',
        'source-map': 'source-map-check',
      }[recipe];
      const outDir = join(cwd, `.octocode/run-${++runs}`);
      mkdirSync(outDir, { recursive: true });
      const env = { ...process.env, CDP_OUTPUT_DIR: outDir };
      for (const [k, v] of Object.entries(options))
        env[k] = typeof v === 'boolean' ? (v ? '1' : '0') : String(v);
      const r = await promisify(execFile)(
        process.execPath,
        [
          join(import.meta.dirname, '../../dist/engine/cdp-runner.mjs'),
          join(import.meta.dirname, `../../dist/engine/cdp-checks/${file}.mjs`),
          '--port',
          String(port),
        ],
        { cwd, encoding: 'utf8', env }
      ).catch(error => error);
      const stdout = r.stdout ?? '';
      const out = { ok: !r.code, stderr: r.stderr };
      return { out, stdout, dir: outDir };
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );

      // Every served script exactly once, bytes equal the served bytes.
      const all = await check('script-sources', {
        SCRIPTS_URL: `${origin}/`,
      });
      const dir = all.dir;
      assert.ok(dir, all.stdout);
      const rows = JSON.parse(readFileSync(join(dir, 'scripts.json'), 'utf8'));
      for (const [p, body] of Object.entries(served)) {
        const hits = rows.filter(r => r.url === `${origin}${p}`);
        assert.equal(hits.length, 1, `${p} listed once`);
        const saved = readFileSync(join(dir, hits[0].file));
        assert.equal(sha(saved), sha(body), `${p} bytes`);
        assert.equal(hits[0].bytes, Buffer.byteLength(body));
      }
      assert.ok(
        rows.every(r => !r.inline),
        'no inline script without SCRIPTS_INLINE'
      );
      assert.equal(new Set(rows.map(r => r.scriptId)).size, rows.length);
      assert.match(all.stdout, /\[SCRIPT\] 1 /);
      assert.ok(!all.stdout.includes('function foo'), 'source not echoed');

      // match narrows.
      const narrow = await check('script-sources', {
        SCRIPTS_URL: `${origin}/`,
        SCRIPTS_MATCH: 'lib.js',
      });
      const nrows = JSON.parse(
        readFileSync(join(narrow.dir, 'scripts.json'), 'utf8')
      );
      assert.deepEqual(
        nrows.map(r => r.url),
        [`${origin}/lib.js`]
      );

      // inline only with SCRIPTS_INLINE.
      const inl = await check('script-sources', {
        SCRIPTS_URL: `${origin}/`,
        SCRIPTS_INLINE: true,
      });
      const irows = JSON.parse(
        readFileSync(join(inl.dir, 'scripts.json'), 'utf8')
      );
      const inlineRows = irows.filter(r => r.inline);
      assert.ok(inlineRows.length >= 1);
      assert.ok(
        inlineRows.some(r =>
          readFileSync(join(inl.dir, r.file), 'utf8').includes('inlineMarker')
        )
      );

      // source-map: positions, listing, safe saving (page is loaded now).
      const sm = await check('source-map', {
        SOURCEMAP_SCRIPT: `${origin}/app.js`,
        SOURCEMAP_POSITIONS: '1:0,1:12,2:3,9:0',
        SOURCEMAP_LIST_SOURCES: true,
        SOURCEMAP_SAVE_SOURCES: true,
      });
      assert.equal(sm.out.ok, true, sm.stdout);
      assert.match(
        sm.stdout,
        /\[POSITION\] 1:0 -> \.\.\/\.\.\/etc\/passwd:1:0/
      );
      assert.match(
        sm.stdout,
        /\[POSITION\] 1:12 -> \.\.\/\.\.\/etc\/passwd:3:4 foo/
      );
      assert.match(sm.stdout, /\[POSITION\] 2:3 -> src\/b\.ts:6:2/);
      assert.match(sm.stdout, /SOURCEMAP_POSITION_UNMAPPED .*9:0/);
      const smDir = sm.dir;
      const index = JSON.parse(
        readFileSync(join(smDir, 'source-map-index.json'), 'utf8')
      );
      assert.equal(index.sources.length, 2);
      assert.ok(index.sources.every(s => s.hasContent));
      const saved = readdirSync(join(smDir, 'sources'), { recursive: true });
      assert.ok(saved.length >= 2);
      // Nothing was written outside the run folder.
      assert.ok(!existsSync(join(smDir, '..', 'etc', 'passwd')));
      assert.ok(!existsSync(join(smDir, '..', '..', 'etc', 'passwd')));
      const mapSaved = JSON.parse(
        readFileSync(join(smDir, 'app.js.map.json'), 'utf8')
      );
      assert.deepEqual(mapSaved.sourcesContent, handMap.sourcesContent);

      // data: URI map.
      const dm = await check('source-map', {
        SOURCEMAP_SCRIPT: 'data.js',
        SOURCEMAP_POSITIONS: '2:3',
      });
      assert.match(dm.stdout, /\[POSITION\] 2:3 -> src\/b\.ts:6:2/);

      // foreign origin refused, non-zero exit.
      const fm = await check('source-map', { SOURCEMAP_SCRIPT: 'foreign.js' });
      assert.match(fm.stdout, /\[FINDING\] SOURCEMAP_REFUSED/);
      assert.equal(fm.out.ok, false);

      // missing script: non-zero; script without a map: finding, ok.
      const ms = await check('source-map', { SOURCEMAP_SCRIPT: 'nope.js' });
      assert.match(ms.stdout, /SOURCEMAP_SCRIPT_NOT_FOUND/);
      assert.equal(ms.out.ok, false);
      const nm = await check('source-map', { SOURCEMAP_SCRIPT: 'lib.js' });
      assert.match(nm.stdout, /SOURCEMAP_MISSING/);
      assert.equal(nm.out.ok, true);
    } finally {
      try {
        await cli('cleanup', { port });
      } finally {
        server.close();
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
