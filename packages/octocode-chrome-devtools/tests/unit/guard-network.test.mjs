import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import test from 'node:test';

const guard = resolve(
  import.meta.dirname,
  '../../dist/engine/cdp-runner-guard.mjs'
);

/** Runs body after installing the guard in a fresh process (the guard patches process-global modules). */
function inGuardedProcess(body) {
  const code = `
    import { restrictNetworkToLocalhost } from ${JSON.stringify(`file://${guard}`)};
    import net from 'node:net';
    import https from 'node:https';
    import http from 'node:http';
    import dns from 'node:dns';
    const server = net.createServer(s => { s.on('error', () => {}); s.end('ok'); }).listen(0, '127.0.0.1');
    await new Promise(r => server.once('listening', r));
    const port = server.address().port;
    restrictNetworkToLocalhost();
    const attempt = async fn => { try { await fn(); return 'ok'; } catch (e) { return String(e.message); } };
    ${body}
    server.close();
  `;
  return spawnSync(process.execPath, ['--input-type=module', '-e', code], {
    encoding: 'utf8',
    timeout: 30_000,
  });
}

test('guard: https.get, net.connect and dns.lookup to a non-loopback host fail with [SANDBOX]', () => {
  const r = inGuardedProcess(`
    const out = [
      await attempt(() => https.get('https://example.com/')),
      await attempt(() => net.connect(443, 'example.com')),
      await attempt(() => dns.lookup('example.com', () => {})),
    ];
    console.log(JSON.stringify(out));
  `);
  assert.equal(r.status, 0, r.stderr);
  for (const msg of JSON.parse(r.stdout.trim().split('\n').pop()))
    assert.match(msg, /^\[SANDBOX\] .* blocked: only loopback allowed/);
});

test('guard: loopback TCP and http still work', () => {
  const r = inGuardedProcess(`
    const viaNet = await attempt(() => new Promise((res, rej) => {
      const c = net.connect(port, '127.0.0.1');
      c.on('connect', () => { c.destroy(); res(); }); c.on('error', rej);
    }));
    console.log(viaNet);
  `);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout.trim().split('\n').pop(), 'ok');
});

test('guard: no global exemption symbol exists inside a guarded process', () => {
  const r = inGuardedProcess(`
    const keys = Object.getOwnPropertySymbols(globalThis).map(String).filter(k => /exempt/i.test(k));
    console.log(JSON.stringify(keys));
  `);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(JSON.parse(r.stdout.trim().split('\n').pop()), []);
});

test('guard: IP literals through net.connect, http.get and a custom lookup are refused too', () => {
  const r = inGuardedProcess(`
    const out = [
      await attempt(() => net.connect(80, '192.0.2.1')),
      await attempt(() => net.connect({ host: '192.0.2.1', port: 80 })),
      await attempt(() => http.get('http://192.0.2.1/')),
      await attempt(() => net.connect(80, '::ffff:c000:201')),
      await attempt(() => net.connect({ host: 'localhost', port: 80, lookup: (h, o, cb) => cb(null, '192.0.2.1', 4) })),
    ];
    console.log(JSON.stringify(out));
  `);
  assert.equal(r.status, 0, r.stderr);
  const out = JSON.parse(r.stdout.trim().split('\n').pop());
  assert.equal(out.length, 5);
  for (const msg of out)
    assert.match(msg, /^\[SANDBOX\] net blocked: only loopback allowed/);
});
