// The evidence recipes on a real isolated headless Chrome and a loopback fixture server:
// page-html, page-archive, page-resources, console-log and downloads. Each recipe module runs in its
// own process (options come from the environment), as in storage-dump-real-chrome.test.mjs.
import { DIE_WITH_PARENT } from '../fixtures/die-with-parent.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const dist = join(import.meta.dirname, '../../dist/engine');
const RUNNER = `
import { connectCDP } from ${JSON.stringify(join(dist, 'cdp-connection.mjs'))};
const targets = await (await fetch('http://127.0.0.1:' + process.env.PORT + '/json')).json();
const target = targets.find(t => t.type === 'page');
const cdp = await connectCDP(target.webSocketDebuggerUrl, { targetInfo: target });
cdp.outputDir = process.env.OUT;
const recipe = await import(${JSON.stringify(dist)} + '/cdp-checks/' + process.argv[1] + '-check.mjs');
await recipe.run(cdp);
cdp.close();
process.exit(process.exitCode ?? 0);
`;

const CSS = '.used{color:red}\n/* css-marker */\n';
const JS = `console.log('log-marker', {a: 1});
console.info('info-marker');
console.warn('warn-marker');
console.error('error-marker');
fetch('/api.json').then(r => r.json()).then(j => console.debug('debug-marker', j.ok));
setTimeout(() => { throw new Error('thrown-marker'); }, 50);
const host = document.getElementById('host').attachShadow({ mode: 'open' });
host.innerHTML = '<p id="inner">SHADOW-CONTENT-MARKER</p>';
`;
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);
const API = JSON.stringify({ ok: true, marker: 'xhr-body-marker' });
const ATTACHMENT = Buffer.from('attachment-bytes\u0000\u0001marker\n');
const ROUTES = altOrigin => ({
  '/': [
    'text/html',
    `<!doctype html><html><head><title>evidence</title><link rel=stylesheet href=/s.css></head><body>
<h1>PAGE-HTML-MARKER</h1><ul><li class=item>one</li><li class=item>two</li><li class=item>three</li></ul>
<div id=host></div><img src=/p.png><img src=/missing.png>
<iframe src=/frame></iframe><iframe src="${altOrigin}/frame2"></iframe>
<script src=/a.js></script></body></html>`,
  ],
  '/frame': ['text/html', '<!doctype html><p>SAME-PROCESS-FRAME-MARKER</p>'],
  '/frame2': ['text/html', '<!doctype html><p>CROSS-ORIGIN-FRAME-MARKER</p>'],
  '/s.css': ['text/css', CSS],
  '/a.js': ['text/javascript', JS],
  '/p.png': ['image/png', PNG],
  '/api.json': ['application/json', API],
  '/dlpage': ['text/html', '<!doctype html><a id=get href="/file">get</a>'],
});
const serverSource = `
const consts = ${JSON.stringify({ CSS, JS, API, PNG: PNG.toString('base64'), ATTACHMENT: ATTACHMENT.toString('base64') })};
const { CSS, JS, API } = consts;
const routes = (${ROUTES.toString()});
const PNG = Buffer.from(consts.PNG, 'base64');
const png = PNG;
const attachment = Buffer.from(consts.ATTACHMENT, 'base64');
require('node:http').createServer((req, res) => {
  const alt = 'http://' + req.headers.host.replace('127.0.0.1', 'localhost');
  if (req.url === '/file') {
    res.setHeader('content-type', 'application/octet-stream');
    res.setHeader('content-disposition', 'attachment; filename="../evil name.bin"');
    return res.end(attachment);
  }
  const row = routes(alt)[req.url.split('?')[0]];
  if (!row) { res.statusCode = 404; return res.end('missing'); }
  res.setHeader('content-type', row[0]);
  res.end(row[0] === 'image/png' ? png : row[1]);
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

const sha = data => createHash('sha256').update(data).digest('hex');

test(
  'evidence recipes save page data to files on a real Chrome',
  { skip, timeout: 280000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-evidence-'));
    const web = spawn(
      process.execPath,
      [...DIE_WITH_PARENT, '-e', serverSource],
      {
        stdio: ['pipe', 'pipe', 'inherit'],
      }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const base = `http://127.0.0.1:${webPort}`;
    const port = await freePort();
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
    const recipe = async (name, env = {}) => {
      const OUT = mkdtempSync(join(cwd, 'out-'));
      const r = await promisify(execFile)(
        process.execPath,
        ['--input-type=module', '-e', RUNNER, name],
        {
          encoding: 'utf8',
          env: { ...process.env, PORT: String(port), OUT, ...env },
        }
      ).catch(error => error);
      const stdout = String(r.stdout ?? '') + String(r.stderr ?? '');
      const artifacts = [...stdout.matchAll(/\[ARTIFACT\] \S+ (\S+)/g)].map(
        m => m[1]
      );
      // Invariant for every recipe: each file named in an [ARTIFACT] line exists.
      for (const file of artifacts)
        assert.ok(existsSync(file), `${name}: ${file} listed but missing`);
      return {
        stdout,
        OUT,
        code: r.code ?? 0,
        read: file => readFileSync(join(OUT, file)),
        json: file => JSON.parse(readFileSync(join(OUT, file), 'utf8')),
      };
    };
    try {
      assert.equal(
        (await cli('open', { port, headless: true, url: 'about:blank' })).ok,
        true
      );

      // page-html
      const html = await recipe('page-html', { PAGEHTML_URL: `${base}/` });
      assert.equal(html.code, 0, html.stdout);
      const page = html.read('page.html').toString();
      assert.match(page, /^<!DOCTYPE html>/i);
      assert.ok(page.includes('PAGE-HTML-MARKER'));
      assert.ok(
        !page.includes('SHADOW-CONTENT-MARKER'),
        'no shadow content without PAGEHTML_SHADOW'
      );
      assert.ok(
        !existsSync(join(html.OUT, 'frame-1.html')),
        'no frame files without PAGEHTML_FRAMES'
      );
      const entry = html.json('html-index.json')[0];
      assert.equal(entry.bytes, Buffer.byteLength(page));
      assert.equal(entry.sha256, sha(page));

      const shadow = await recipe('page-html', { PAGEHTML_SHADOW: '1' });
      assert.equal(shadow.code, 0, shadow.stdout);
      assert.match(shadow.read('page.html').toString(), /^<!DOCTYPE html>/i);
      assert.ok(
        shadow.read('page.html').toString().includes('SHADOW-CONTENT-MARKER')
      );

      const framed = await recipe('page-html', { PAGEHTML_FRAMES: '1' });
      assert.equal(framed.code, 0, framed.stdout);
      const index = framed.json('html-index.json');
      const frameRows = index.filter(r => r.file.startsWith('frame-'));
      assert.equal(frameRows.length, 2, framed.stdout);
      const frameText = frameRows
        .map(r => framed.read(r.file).toString())
        .join('\n');
      assert.ok(frameText.includes('SAME-PROCESS-FRAME-MARKER'));
      assert.ok(frameText.includes('CROSS-ORIGIN-FRAME-MARKER'));
      assert.ok(frameRows.every(r => r.url && r.frame));

      const picked = await recipe('page-html', {
        PAGEHTML_SELECTOR: 'li.item',
      });
      assert.equal(picked.code, 0, picked.stdout);
      const selected = picked.json('html-index.json').filter(r => r.selector);
      assert.equal(selected.length, 3);
      assert.deepEqual(
        selected.map(r => picked.read(r.file).toString()),
        [
          '<li class="item">one</li>',
          '<li class="item">two</li>',
          '<li class="item">three</li>',
        ]
      );
      const none = await recipe('page-html', {
        PAGEHTML_SELECTOR: 'li.nothing',
      });
      assert.equal(none.code, 1);
      assert.match(none.stdout, /PAGE_HTML_NO_MATCH/);

      // page-archive
      const archive = await recipe('page-archive', { ARCHIVE_URL: `${base}/` });
      assert.equal(archive.code, 0, archive.stdout);
      const mhtml = archive.read('page.mhtml').toString();
      // Chrome writes From/Snapshot-Content-Location/Subject/Date headers before MIME-Version.
      assert.match(mhtml.slice(0, 1000), /^MIME-Version: 1\.0\r?$/m);
      assert.ok(mhtml.includes('PAGE-HTML-MARKER'));
      assert.equal(archive.json('archive-index.json')[0].sha256, sha(mhtml));

      // page-resources
      const res = await recipe('page-resources', { RES_URL: `${base}/` });
      assert.equal(res.code, 0, res.stdout);
      const rows = res.json('resources-index.json');
      const once = url => rows.filter(r => r.url === `${base}${url}`);
      for (const [path, expected] of [
        ['/', null],
        ['/s.css', CSS],
        ['/a.js', JS],
        ['/api.json', API],
      ]) {
        const found = once(path);
        assert.equal(found.length, 1, `${path} listed once: ${res.stdout}`);
        assert.ok(found[0].file && found[0].sha256 && found[0].bytes > 0);
        const bytes = res.read(found[0].file);
        assert.equal(sha(bytes), found[0].sha256);
        if (expected !== null) assert.equal(bytes.toString(), expected);
      }
      const sameFrame = rows.find(r => r.url === `${base}/frame`);
      assert.ok(sameFrame?.file, res.stdout);
      assert.ok(
        res
          .read(sameFrame.file)
          .toString()
          .includes('SAME-PROCESS-FRAME-MARKER')
      );
      // A cross-origin (out-of-process) iframe document is listed; Chrome may not cache its body,
      // and then the row carries the error and a finding instead of a silent gap.
      const crossUrl = `${base.replace('127.0.0.1', 'localhost')}/frame2`;
      const cross = rows.find(r => r.url === crossUrl);
      assert.ok(cross, res.stdout);
      assert.ok(
        cross.file ||
          (cross.error && res.stdout.includes('PAGE_RESOURCE_UNAVAILABLE'))
      );
      assert.equal(once('/s.css')[0].source, 'resourceTree');
      assert.equal(once('/api.json')[0].source, 'network');
      assert.ok(
        res
          .read(once('/api.json')[0].file)
          .toString()
          .includes('xhr-body-marker')
      );
      assert.equal(
        once('/p.png').length,
        0,
        'binary absent without RES_BINARY'
      );
      assert.match(res.stdout, /RES_BINARY=1/);
      assert.ok(rows.every(r => !r.file || !r.file.includes('..')));

      const binary = await recipe('page-resources', { RES_BINARY: '1' });
      const png = binary
        .json('resources-index.json')
        .find(r => r.url.endsWith('/p.png'));
      assert.ok(png, binary.stdout);
      assert.equal(png.sha256, sha(PNG), JSON.stringify(png));
      assert.deepEqual(binary.read(png.file), PNG);

      const narrow = await recipe('page-resources', {
        RES_TYPES: JSON.stringify(['stylesheet']),
        RES_MATCH: 's.css',
      });
      assert.deepEqual(
        narrow.json('resources-index.json').map(r => `${r.type} ${r.url}`),
        [`stylesheet ${base}/s.css`]
      );
      const noXhr = await recipe('page-resources');
      assert.equal(
        noXhr.json('resources-index.json').some(r => r.source === 'network'),
        false
      );
      assert.match(noXhr.stdout, /pass RES_URL/);

      // console-log
      const levels = async list => {
        const r = await recipe('console-log', {
          CONSOLE_URL: `${base}/`,
          CONSOLE_MS: '2500',
          ...(list ? { CONSOLE_LEVELS: JSON.stringify(list) } : {}),
        });
        assert.equal(r.code, 0, r.stdout);
        const lines = r.read('console.jsonl').toString().trim().split('\n');
        return { r, events: lines.filter(Boolean).map(l => JSON.parse(l)) };
      };
      const all = await levels();
      const has = (m, level) =>
        all.events.some(
          e => e.text.includes(m) && (!level || e.level === level)
        );
      assert.ok(has('log-marker', 'log'));
      assert.ok(has('info-marker', 'info'));
      assert.ok(has('warn-marker', 'warning'));
      assert.ok(has('error-marker', 'error'));
      assert.ok(has('debug-marker', 'debug'));
      const thrown = all.events.find(
        e => e.source === 'exception' && e.text.includes('thrown-marker')
      );
      assert.ok(
        thrown &&
          thrown.level === 'error' &&
          thrown.url.endsWith('/a.js') &&
          thrown.line >= 1
      );
      const logEntry = all.events.find(e => e.source === 'network');
      assert.ok(logEntry, 'a Log.entryAdded event (the missing image)');
      assert.ok(all.events.every(e => e.ts && e.level && 'text' in e));
      assert.ok(
        all.events
          .find(e => e.text.includes('error-marker'))
          .url.endsWith('/a.js')
      );
      const onlyErrors = await levels(['error']);
      assert.ok(onlyErrors.events.length > 0);
      assert.ok(onlyErrors.events.every(e => e.level === 'error'));
      assert.ok(onlyErrors.events.some(e => e.source === 'exception'));
      assert.ok(onlyErrors.events.some(e => e.source === 'network'));
      assert.ok(!onlyErrors.events.some(e => e.text.includes('warn-marker')));

      // downloads: refused without consent, before the browser is touched.
      const refused = await recipe('downloads', {
        DL_URL: `${base}/file`,
        DL_MS: '3000',
      });
      assert.equal(refused.code, 1);
      assert.match(refused.stdout, /consent/);
      assert.ok(!existsSync(join(refused.OUT, 'downloads')));

      const fetched = await recipe('downloads', {
        DL_URL: `${base}/file`,
        DL_MS: '8000',
        DL_ACKNOWLEDGE: '1',
      });
      const noEvents = /DOWNLOADS_NONE|DOWNLOADS_UNAVAILABLE/.test(
        fetched.stdout
      );
      if (noEvents) {
        // Documented skip: this Chrome gave no download events; the reason is the recipe's own finding.
        console.warn(`downloads skipped: ${fetched.stdout}`);
      } else {
        assert.equal(fetched.code, 0, fetched.stdout);
        const row = fetched.json('downloads-index.json')[0];
        assert.equal(row.state, 'completed');
        assert.equal(row.suggestedFilename.includes('evil name'), true);
        assert.ok(
          !row.file.includes('..') && row.file.startsWith('downloads/')
        );
        assert.equal(row.sha256, sha(ATTACHMENT));
        assert.equal(row.bytes, ATTACHMENT.length);
        assert.deepEqual(fetched.read(row.file), ATTACHMENT);
        assert.equal(
          statSync(join(fetched.OUT, 'downloads')).mode & 0o777,
          0o700
        );
        assert.match(fetched.stdout, /untrusted/);
        // A click starts the download from a page.
        const clicked = await recipe('downloads', {
          DL_URL: `${base}/dlpage`,
          DL_CLICK_SELECTOR: '#get',
          DL_MS: '8000',
          DL_ACKNOWLEDGE: '1',
        });
        assert.equal(clicked.code, 0, clicked.stdout);
        assert.equal(
          clicked.json('downloads-index.json')[0].sha256,
          sha(ATTACHMENT)
        );
        // Invariant: downloads/ holds exactly the files the index lists (no Chrome background or partial files).
        for (const run of [fetched, clicked]) {
          const listed = run
            .json('downloads-index.json')
            .map(r => r.file.replace('downloads/', ''))
            .sort();
          const left = readdirSync(join(run.OUT, 'downloads')).sort();
          assert.deepEqual(left, listed, `${left.join(', ')}\n${run.stdout}`);
        }
      }
    } finally {
      try {
        await cli('cleanup', { port });
      } finally {
        web.kill();
        rmSync(cwd, { recursive: true, force: true });
      }
    }
  }
);
