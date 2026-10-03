import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:net';
import {createHash} from 'node:crypto';
import {existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {promisify} from 'node:util';
import {DatabaseSync} from 'node:sqlite';
import {binary, commandArgs, execFile, jsonCall, spawn, tempWorkspace} from './helpers.mjs';
const exec = promisify(execFile);

function fixture(t) {
  const workspace = tempWorkspace(t, 'active-delivery-'), database = join(workspace, 'db.sqlite');
  const call = jsonCall(binary, workspace, database), args = commandArgs(workspace, database);
  const sender = call('join', {name: 'sender', vendor: 'test'}).id;
  return {workspace, database, call, args, sender};
}
async function until(check, timeout = 5000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (check()) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.fail('Delivery did not reach the active recipient');
}

async function codexSocket(t, workspace) {
  const requests = [], sockets = new Set();
  const server = createServer(socket => {
    sockets.add(socket); socket.on('error', () => {}); socket.on('close', () => sockets.delete(socket));
    let pending = Buffer.alloc(0), upgraded = false;
    const send = value => {
      const payload = Buffer.from(JSON.stringify(value));
      const header = Buffer.alloc(payload.length < 126 ? 2 : 4); header[0] = 0x81;
      if (payload.length < 126) header[1] = payload.length;
      else { header[1] = 126; header.writeUInt16BE(payload.length, 2); }
      socket.write(Buffer.concat([header, payload]));
    };
    socket.on('data', chunk => {
      pending = Buffer.concat([pending, chunk]);
      if (!upgraded) {
        const end = pending.indexOf('\r\n\r\n'); if (end < 0) return;
        const key = pending.subarray(0, end).toString().match(/sec-websocket-key: (.+)/i)[1].trim();
        const accept = createHash('sha1').update(key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
        socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
        pending = pending.subarray(end + 4); upgraded = true;
      }
      while (pending.length >= 2) {
        let length = pending[1] & 127, offset = 2;
        if (length === 126) { if (pending.length < 4) return; length = pending.readUInt16BE(2); offset = 4; }
        assert.notEqual(length, 127, 'Fixture frames stay below 64KiB');
        if (pending.length < offset + 4 + length) return;
        const mask = pending.subarray(offset, offset + 4), body = Buffer.from(pending.subarray(offset + 4, offset + 4 + length));
        for (let i = 0; i < body.length; i++) body[i] ^= mask[i % 4];
        pending = pending.subarray(offset + 4 + length);
        const request = JSON.parse(body.toString()); requests.push(request);
        if (request.id === undefined) continue;
        const result = request.method === 'thread/read'
          ? {thread: {id: 'existing', cwd: realpathSync(workspace), status: {type: 'active'}}}
          : request.method === 'turn/start' ? {turn: {id: 'same-active-turn', status: 'inProgress'}} : {};
        send({id: request.id, result});
      }
    });
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { for (const socket of sockets) socket.destroy(); server.close(); });
  return {requests, endpoint: `ws://127.0.0.1:${server.address().port}`};
}

test('native Codex submits action and passive peer data during an active turn exactly once', async t => {
  const f = fixture(t), server = await codexSocket(t, f.workspace);
  const receiver = f.call('join', {name: 'receiver', vendor: 'codex'}).id;
  f.call('attach', {transport: 'codex', endpoint: server.endpoint, vendorSession: 'existing'}, receiver);
  for (const wake of ['passive', 'action']) {
    const sent = f.call('send_message', {to: receiver, body: `${wake}-while-tool-running`, wake, replyRequired: false, reasoning: 'Verify active native delivery'}, f.sender);
    const result = JSON.parse((await exec(binary, f.args('dispatch', {}, receiver), {timeout: 10000})).stdout);
    assert.equal(result.submitted, 1);
    const delivery = server.requests.filter(row => row.method === (wake === 'action' ? 'turn/start' : 'thread/inject_items')).at(-1);
    assert.equal(delivery.params.threadId, 'existing');
    if (wake === 'action') {
      assert.deepEqual(delivery.params.input, []);
      assert.equal(delivery.params.toolOutput.name, 'octocode_peer_messages');
      assert.match(delivery.params.toolOutput.output, /action-while-tool-running/);
    } else {
      assert.equal(delivery.params.items[0].type, 'function_call_output');
      assert.equal(delivery.params.items[0].name, 'octocode_peer_messages');
      assert.match(delivery.params.items[0].output, /passive-while-tool-running/);
    }
    assert.equal(f.call('dispatch', {}, receiver).submitted, 0);
    assert.ok(f.call('inbox', {}, receiver).items.some(row => row.id === sent.id), 'Submission does not acknowledge recipient work');
  }
  assert.equal(server.requests.filter(row => row.method === 'turn/start').length, 1);
  assert.equal(server.requests.filter(row => row.method === 'thread/inject_items').length, 1);
});

test('managed Codex injects passive and action peer data before the active turn completes', {skip: process.platform === 'win32'}, async t => {
  const f = fixture(t), bin = join(f.workspace, 'bin'), script = join(f.workspace, 'codex-fixture.cjs'), log = join(f.workspace, 'requests.jsonl');
  mkdirSync(bin);
  writeFileSync(script, `const fs=require('node:fs');const rl=require('node:readline').createInterface({input:process.stdin});
const send=x=>console.log(JSON.stringify(x));rl.on('line',line=>{const r=JSON.parse(line);fs.appendFileSync(${JSON.stringify(log)},line+'\\n');if(r.id===undefined)return;
let result={};if(r.method==='config/read')result={config:{}};if(r.method==='skills/list')result={data:[]};if(r.method==='thread/start')result={thread:{id:'busy-thread'}};
if(r.method==='turn/start')result={turn:{id:'held-active-turn',status:'inProgress'}};send({id:r.id,result});});`);
  const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
  writeFileSync(join(bin, 'codex'), `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`, {mode: 0o755});
  const child = spawn(binary, ['run', '--vendor', 'codex', '--model', 'test', '--prompt', 'Remain active', '--duration-ms', '10000', '--workspace', f.workspace, '--database', f.database], {env: {...process.env, PATH: bin}});
  let stdout = '', stderr = '';
  child.stdout.on('data', chunk => stdout += chunk); child.stderr.on('data', chunk => stderr += chunk);
  const closed = new Promise(resolve => child.once('close', code => resolve(code)));
  const requests = () => existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  const db = new DatabaseSync(f.database); db.exec('PRAGMA busy_timeout=5000');
  try {
    await until(() => requests().some(row => row.method === 'turn/start'));
    const ready = stdout.split('\n').filter(Boolean).map(JSON.parse).find(row => row.type === 'ready');
    assert.ok(ready, stderr);
    for (const wake of ['passive', 'action']) {
      const body = `${wake}-during-busy`, sent = f.call('send_message', {to: ready.session, body, wake, replyRequired: false, reasoning: 'Verify managed active delivery'}, f.sender);
      await until(() => db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id)?.state === 'submitted');
      const delivery = requests().find(row => JSON.stringify(row.params).includes(body));
      assert.ok(delivery, `${body} must reach the process while its original turn is still active`);
      assert.equal(delivery.method, wake === 'action' ? 'turn/start' : 'thread/inject_items');
      if (wake === 'action') assert.deepEqual(delivery.params.input, []);
      else assert.equal(delivery.params.items[0].type, 'function_call_output');
      assert.equal(requests().filter(row => JSON.stringify(row.params).includes(body)).length, 1);
    }
    assert.ok(!stdout.includes('turn-completed'), 'Delivery must not wait for a full turn');
  } finally {
    if (child.exitCode === null) child.kill('SIGTERM');
    await closed; db.close();
  }
});
