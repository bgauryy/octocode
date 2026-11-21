// Every tool of the full MCP surface is called against a real isolated headless Chrome (temporary
// workspace, loopback fixture site in a child process) and its answer is asserted by CONTENT: the
// value a healthy tool must return, never just "no error". A tool listed by the contract without a
// row here fails the last subtest.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createConnection } from 'node:net';
import { readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { WebSocket } from 'ws';
import { skip, page, withSite } from './tool-depth-fixture.mjs';
import { har, harEntry } from './helper-fixture.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const listening = port =>
  new Promise(resolve => {
    const socket = createConnection({ port, host: '127.0.0.1' });
    socket.once('connect', () => (socket.destroy(), resolve(true)));
    socket.once('error', () => resolve(false));
  });
const wsOpen = url =>
  new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.once('open', () => resolve(socket));
    socket.once('error', reject);
    socket.once('unexpected-response', (_req, res) =>
      reject(new Error(`HTTP ${res.statusCode}`))
    );
  });
const wsAsk = (socket, method) =>
  new Promise(resolve => {
    socket.once('message', data => resolve(JSON.parse(String(data))));
    socket.send(JSON.stringify({ id: 1, method }));
  });

test(
  'every tool of the full surface returns the content it promises on real Chrome',
  { skip, timeout: 280000 },
  async t => {
    await withSite(
      async ({ cwd, base, port, connection, client, call, ok, read }) => {
        const called = new Set();
        const tool = (name, args) => (called.add(name), call(name, args));
        const toolOk = (name, args) => (called.add(name), ok(name, args));
        const stdoutOf = reply =>
          read(
            JSON.parse(read(reply.evidence)).logs.stdout.replace(`${cwd}/`, '')
          ).toString();
        called.add('open');
        const opened = await call('open', {
          port,
          headless: true,
          url: `${base}/`,
        });
        const product = opened.data?.browser ?? '';
        await t.test(
          'open reports a ready isolated browser with its product string',
          () => {
            assert.equal(opened.data.status, 'BROWSER_READY');
            assert.match(product, /Chrome\//);
          }
        );

        await t.test(
          'targets lists the fixture page by its served title',
          async () => {
            const { data } = await toolOk('targets', { connection });
            const row = data.find(
              entry => entry.type === 'page' && entry.url === `${base}/`
            );
            assert.equal(row?.title, page.title);
          }
        );

        let snapshot;
        await t.test(
          'snapshot refs name every control the fixture serves',
          async () => {
            // No MCP preset exports the snapshot tool: a plan step captures the same refs.
            snapshot = await toolOk('run', {
              connection,
              plan: { steps: [{ op: 'snapshot' }] },
            });
            for (const [role, name] of page.controls)
              assert.match(
                snapshot.snapshot,
                new RegExp(`\\[e\\d+\\] ${role} "${name}"`),
                `${role} ${name}`
              );
            assert.match(
              snapshot.snapshot,
              new RegExp(`\\[PAGE\\] "${page.title}"`)
            );
          }
        );

        await t.test(
          'screenshot saves a non-empty JPEG listed in its files',
          async () => {
            const shot = await toolOk('check', {
              connection,
              recipe: 'page-screenshot',
            });
            const manifestPath = shot.files
              .map(f => (typeof f === 'string' ? f : f.path))
              .find(f => f.endsWith('screenshot-manifest.json'));
            assert.ok(manifestPath, 'files names the screenshot manifest');
            const manifest = JSON.parse(read(manifestPath));
            assert.ok(manifest.tiles.length >= 1);
            for (const tile of manifest.tiles) {
              assert.ok(
                !tile.file.startsWith('/'),
                `relative tile ${tile.file}`
              );
              const bytes = read(tile.file);
              assert.ok(bytes.length > 0);
              assert.deepEqual([...bytes.subarray(0, 3)], [0xff, 0xd8, 0xff]);
              assert.equal(bytes.length, tile.bytes);
            }
          }
        );

        await t.test(
          'run evaluates document.title and returns the served title',
          async () => {
            const run = await toolOk('run', {
              connection,
              plan: {
                steps: [{ op: 'evaluate', expression: 'document.title' }],
              },
            });
            assert.equal(run.flow.steps[0].op, 'evaluate');
            assert.equal(run.flow.steps[0].data.result, page.title);
          }
        );

        await t.test(
          'check page-snapshot prints the SNAPSHOT metric with a ref per control',
          async () => {
            const check = await toolOk('check', {
              connection,
              recipe: 'page-snapshot',
            });
            const out = stdoutOf(check);
            const metric = /\[METRIC\] SNAPSHOT refs=(\d+)/.exec(out);
            assert.ok(metric, out);
            assert.ok(Number(metric[1]) >= page.controls.length);
            assert.equal(
              out.split('\n').filter(l => l.startsWith('[SNAPSHOT] [e')).length,
              Number(metric[1])
            );
          }
        );

        await t.test(
          'check coverage splits the fixture bytes into used and unused',
          async () => {
            const check = await toolOk('check', {
              connection,
              recipe: 'coverage',
              options: { COVERAGE_URL: `${base}/` },
            });
            const out = stdoutOf(check);
            for (const kind of ['js', 'css']) {
              const m = new RegExp(
                `\\[METRIC\\] COVERAGE ${kind} files=(\\d+) total=(\\d+) used=(\\d+) unused=(\\d+)`
              ).exec(out);
              assert.ok(m, `${kind}: ${out}`);
              const [files, total, used, unused] = m.slice(1).map(Number);
              assert.ok(files >= 1 && total > 0, kind);
              assert.ok(unused > 0, `${kind} has dead code in the fixture`);
              assert.equal(used + unused, total, kind);
            }
            const names = JSON.parse(
              read(
                check.files
                  .map(f => f.path ?? f)
                  .find(f => f.endsWith('/coverage.json'))
              )
            );
            assert.ok(JSON.stringify(names).includes('/app.js'));
          }
        );

        await t.test(
          'script output carries the browser product string',
          async () => {
            writeFileSync(
              join(cwd, '.octocode/smoke-script.mjs'),
              "export async function run(cdp) { console.log('product=' + (await cdp.send('Browser.getVersion')).product); }\n"
            );
            const script = await toolOk('script', {
              file: '.octocode/smoke-script.mjs',
              acknowledge: true,
              connection,
            });
            assert.ok(
              script.stdout.includes(`product=${product}`),
              script.stdout
            );
          }
        );

        await t.test(
          'cookies export is mode 0600, names the cookie and keeps its value out of replies',
          async () => {
            const state = '.octocode/smoke-state.json';
            const cookies = await toolOk('cookies', {
              from: { port },
              confirmDomains: ['127.0.0.1'],
              exportStorageState: state,
              acknowledgeSecrets: true,
            });
            assert.equal(statSync(join(cwd, state)).mode & 0o777, 0o600);
            const exported = read(state).toString();
            assert.ok(exported.includes(page.cookie.name));
            assert.ok(
              exported.includes(page.cookie.value),
              'the file is the place the secret lives'
            );
            for (const surface of [cookies.text, stdoutOf(cookies)])
              assert.ok(
                !surface.includes(page.cookie.value),
                'secret value stays out of replies'
              );
            assert.ok(
              cookies.text.includes(page.cookie.name),
              'reply names the cookie'
            );
          }
        );

        await t.test(
          'artifact continuation rebuilds the file byte for byte',
          async () => {
            const file = snapshot.evidence;
            const want = read(file);
            let next = {
              file,
              format: 'json',
              length: 100,
              sha256: sha256(want),
            };
            let rebuilt = '';
            for (let pages = 0; next && pages < 200; pages++) {
              const reply = await toolOk('artifact', next);
              rebuilt += reply.data.content;
              assert.equal(reply.data.sha256, sha256(want));
              next = reply.data.next?.query;
            }
            assert.equal(Buffer.byteLength(rebuilt), want.length);
            assert.equal(sha256(rebuilt), sha256(want));
          }
        );

        await t.test(
          'query row count equals a filter computed independently',
          async () => {
            const coverage = await toolOk('check', {
              connection,
              recipe: 'coverage',
              options: { COVERAGE_URL: `${base}/` },
            });
            const manifest = JSON.parse(read(coverage.evidence));
            const wantRows = manifest.artifacts.filter(a =>
              a.path.endsWith('ranges.json')
            );
            const reply = await toolOk('query', {
              file: coverage.evidence,
              format: 'json',
              pointer: '/artifacts',
              where: [{ path: 'path', op: 'contains', value: 'ranges.json' }],
            });
            assert.equal(reply.data.scanned, manifest.artifacts.length);
            assert.equal(reply.data.matched, wantRows.length);
            assert.deepEqual(
              reply.data.rows.map(r => r.value.path),
              wantRows.map(a => a.path.replace(`${cwd}/`, ''))
            );
          }
        );

        await t.test(
          'skill returns the topic text starting at its first heading',
          async () => {
            const reply = await toolOk('skill', {
              topic: 'recovery',
              length: 2000,
            });
            const first = readFileSync(reply.data.file, 'utf8')
              .split('\n')
              .find(l => l.startsWith('# '));
            assert.ok(first, 'the topic file has a heading');
            assert.ok(reply.data.content.startsWith(first));
          }
        );

        await t.test(
          'schema field guide for the evaluate op lists its required field',
          async () => {
            const { data } = await toolOk('schema', {
              command: 'run',
              operation: 'evaluate',
            });
            assert.ok(
              data.plan.guides.evaluate.required.includes('expression')
            );
            assert.equal(
              data.plan.guides.evaluate.fields.expression.type,
              'string'
            );
          }
        );

        await t.test(
          'protocol lists the selector parameter of DOM.querySelector',
          async () => {
            const { data } = await toolOk('protocol', {
              member: 'DOM.querySelector',
              connection,
            });
            const selector = data.params.find(p => p.name === 'selector');
            assert.ok(selector && selector.optional === false);
          }
        );

        await t.test(
          'session start, list, show and close round-trip one id',
          async () => {
            const started = await toolOk('session', { action: 'start', port });
            const id = started.data.session.id;
            assert.equal(started.data.session.lifecycle, 'active');
            const listed = await toolOk('session', { action: 'list' });
            assert.ok(listed.data.sessions.some(s => s.id === id));
            const shown = await toolOk('session', { action: 'show', id });
            assert.equal(shown.data.session.id, id);
            const closed = await toolOk('session', { action: 'close', id });
            assert.equal(closed.data.session.id, id);
            assert.equal(closed.data.session.lifecycle, 'closed');
          }
        );

        await t.test(
          'proxy serves a token-gated CDP passthrough and stop frees its port',
          async () => {
            const started = await toolOk('proxy', { action: 'start', port });
            const { url, listenPort } = started.data;
            assert.match(url, /^ws:\/\/127\.0\.0\.1:\d+\//);
            const socket = await wsOpen(url);
            const answer = await wsAsk(socket, 'Browser.getVersion');
            socket.close();
            assert.equal(answer.result.product, product);
            const wrong = url.replace(/\/[0-9a-f]+$/, '/not-the-token');
            assert.notEqual(wrong, url);
            await assert.rejects(
              wsOpen(wrong),
              /HTTP 4\d\d|ECONNRESET|socket hang up/
            );
            const status = await toolOk('proxy', { action: 'status', port });
            assert.equal(status.data.listenPort, listenPort);
            await toolOk('proxy', { action: 'stop', port });
            assert.equal(await listening(listenPort), false);
          }
        );

        await t.test(
          'helper har-pager filter keeps only the failing entry',
          async () => {
            writeFileSync(
              join(cwd, 'depth.har'),
              JSON.stringify(
                har(
                  harEntry({
                    url: 'https://a.test/ok',
                    status: 200,
                    body: '{}',
                  }),
                  harEntry({
                    url: 'https://a.test/broken',
                    status: 500,
                    body: 'oops',
                  })
                )
              )
            );
            const { data } = await toolOk('helper', {
              name: 'har-pager',
              args: ['depth.har', '--filter', 'failures', '--format', 'json'],
            });
            assert.equal(data.allEntries, 2);
            assert.equal(data.pageRows.length, 1);
            assert.equal(data.pageRows[0].status, 500);
          }
        );

        await t.test(
          'helper prune dry-run lists only the old run folders',
          async () => {
            const { mkdirSync, utimesSync, existsSync } =
              await import('node:fs');
            const pruneBase =
              '.octocode/octocode-chrome-devtools/tmp/depth-prune';
            const old = ['2020-01-01-00-00-00', '2020-01-02-00-00-00'];
            const fresh = ['2099-01-01-00-00-00'];
            for (const [name, days] of [
              ...old.map(n => [n, 30]),
              ...fresh.map(n => [n, 0]),
            ]) {
              mkdirSync(join(cwd, pruneBase, name), { recursive: true });
              const at = new Date(Date.now() - days * 86400000);
              utimesSync(join(cwd, pruneBase, name), at, at);
            }
            const { data } = await toolOk('helper', {
              name: 'prune',
              args: ['--base', pruneBase, '--max-age-days', '3', '--dry-run'],
            });
            assert.equal(data.dryRun, true);
            assert.deepEqual(
              data.runDirs.paths.map(p => p.split('/').pop()).sort(),
              old
            );
            for (const name of [...old, ...fresh])
              assert.ok(existsSync(join(cwd, pruneBase, name)));
          }
        );

        await t.test(
          'dev-check reports the fixture URL and its served title',
          async () => {
            const { data } = await toolOk('dev-check', {
              url: `${base}/`,
              port,
            });
            assert.equal(data.url, `${base}/`);
            assert.equal(data.title, page.title);
            assert.equal(data.document.status, 200);
            assert.equal(data.clean, true);
          }
        );

        await t.test(
          'flow list names emulate-device and show returns valid params',
          async () => {
            const listed = await toolOk('flow', { action: 'list' });
            assert.ok(JSON.stringify(listed.data).includes('emulate-device'));
            const shown = await toolOk('flow', {
              action: 'show',
              name: 'emulate-device',
              params: { width: 390, height: 844 },
            });
            assert.equal(shown.data.name, 'emulate-device');
            assert.equal(shown.data.params.width.required, true);
            assert.ok(shown.data.valid !== false);
          }
        );

        await t.test(
          'group lifecycle through the MCP tool isolates cookies and frees the name',
          async () => {
            const evaluate = async (conn, expression) =>
              (
                await toolOk('run', {
                  connection: conn,
                  plan: { steps: [{ op: 'evaluate', expression }] },
                })
              ).flow.steps[0].data.result;
            const plain = `${base}/plain`;
            await toolOk('open', { port, url: plain });
            const plainConn = { port, targetUrl: plain };
            assert.match(
              await evaluate(
                plainConn,
                "document.cookie='plain=1;path=/'; document.cookie"
              ),
              /plain=1/
            );

            const created = await toolOk('group', {
              action: 'open',
              name: 'alpha',
              urls: [`${base}/a0`, `${base}/a1`],
              port,
            });
            assert.equal(created.data.private, true);
            assert.deepEqual(
              created.data.tabs.map(tab => tab.status),
              ['loaded', 'loaded']
            );
            const alpha = index => ({
              port,
              group: 'alpha',
              groupIndex: index,
            });
            assert.doesNotMatch(
              await evaluate(alpha(0), 'document.cookie'),
              /plain=1|sessname/
            );
            await evaluate(alpha(0), "document.cookie='alpha=1;path=/'");
            assert.match(
              await evaluate(alpha(1), 'document.cookie'),
              /alpha=1/
            );
            assert.doesNotMatch(
              await evaluate(plainConn, 'document.cookie'),
              /alpha=1/
            );

            const listed = await toolOk('group', { action: 'list', port });
            const group = listed.data.groups.find(g => g.name === 'alpha');
            assert.equal(group.tabCount, 2);
            assert.ok(group.tabs.every(tab => tab.alive));

            const added = await toolOk('group', {
              action: 'add',
              name: 'alpha',
              urls: [`${base}/a2`],
              port,
            });
            assert.equal(added.data.tabs.length, 1);
            const closedTab = await toolOk('group', {
              action: 'close',
              name: 'alpha',
              tab: added.data.tabs[0].targetId,
              port,
            });
            assert.equal(closedTab.ok, true);
            const after = (
              await toolOk('group', { action: 'list', port })
            ).data.groups.find(g => g.name === 'alpha');
            assert.equal(after.tabCount, 2);
            assert.ok(
              !after.tabs.some(
                tab => tab.targetId === added.data.tabs[0].targetId
              )
            );

            await toolOk('group', { action: 'close', name: 'alpha', port });
            const gone = (await toolOk('group', { action: 'list', port })).data
              .groups;
            assert.ok(!gone.some(g => g.name === 'alpha'));
            const again = await toolOk('group', {
              action: 'open',
              name: 'alpha',
              urls: [`${base}/a0`],
              port,
            });
            assert.equal(again.data.group, 'alpha');
            await toolOk('group', { action: 'close', name: 'alpha', port });
          }
        );

        await t.test(
          'cdp answers Browser.getVersion, asks for permission before Browser.close and gates Runtime.evaluate on acknowledge',
          async () => {
            const version = await toolOk('cdp', {
              method: 'Browser.getVersion',
              connection,
            });
            assert.equal(version.flow.steps[0].data.product, product);
            const close = await tool('cdp', {
              method: 'Browser.close',
              connection,
            });
            assert.match(
              close.refused,
              /user's permission.*acknowledge: true/s
            );
            const target = { port, targetUrl: `${base}/plain` };
            const params = { expression: '6*7', returnByValue: true };
            const gated = await tool('cdp', {
              method: 'Runtime.evaluate',
              params,
              connection: target,
            });
            assert.match(gated.refused, /acknowledge/i);
            const allowed = await toolOk('cdp', {
              method: 'Runtime.evaluate',
              params,
              connection: target,
              acknowledge: true,
            });
            assert.equal(allowed.flow.steps[0].data.result.value, 42);
            // The unacknowledged Browser.close left the browser running (the acknowledged one is tested on its own browser).
            assert.equal(await listening(port), true);
          }
        );

        await t.test(
          'cleanup leaves no listener on the debugging port',
          async () => {
            const cleaned = await toolOk('cleanup', { port });
            assert.equal(cleaned.data.status, 'CLEANED_UP');
            assert.equal(await listening(port), false);
          }
        );

        await t.test('every tool in tools/list has a smoke row', async () => {
          const { tools } = await client.listTools();
          for (const { name } of tools)
            assert.ok(called.has(name), `no smoke call for tool ${name}`);
        });
      }
    );
  }
);
