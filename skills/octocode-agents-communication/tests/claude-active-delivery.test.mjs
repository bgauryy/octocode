import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdirSync, readFileSync, writeFileSync, existsSync, rmSync} from 'node:fs';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {binary, tempDir, jsonCall, execFileSync, spawn} from './helpers.mjs';

const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, detail) {
  const deadline = Date.now() + 8000;
  while (Date.now() < deadline) {
    if (check()) return;
    await delay(20);
  }
  assert.fail(`Timed out: ${detail()}`);
}

async function fixture(t, mode) {
  const workspace = tempDir('claude-active-', {real:true});
  const database = join(workspace, 'communication.sqlite'), bin = join(workspace, 'bin');
  const capture = join(workspace, 'frames.jsonl'), release = join(workspace, 'release');
  mkdirSync(bin);
  const program = join(bin, 'claude.cjs');
  writeFileSync(program, `
const fs = require('node:fs'), net = require('node:net');
if (process.argv.includes('--fixture-preflight')) process.exit(0);
const capture = process.env.CLAUDE_FIXTURE_CAPTURE, release = process.env.CLAUDE_FIXTURE_RELEASE;
const record = (channel, frame) => fs.appendFileSync(capture, JSON.stringify({channel,frame})+'\\n');
const send = frame => console.log(JSON.stringify(frame));
const index = process.argv.indexOf('--messaging-socket-path');
if (index < 0) throw Error('Managed Claude must expose its native messaging socket');
let received = false, finished = false;
const finish = () => { if (!finished) { finished = true; send({type:'result',is_error:false}); } };
net.createServer(socket => {
  let data = '';
  socket.on('data', chunk => data += chunk);
  socket.on('end', () => {
    for (const line of data.trim().split('\\n')) record('socket', JSON.parse(line));
    received = true;
    if (process.env.CLAUDE_FIXTURE_MODE === 'idle') finish();
  });
}).listen(process.argv[index+1], () => {
  send({type:'system',subtype:'init',session_id:'fixture-claude-native'});
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    record('stdin', JSON.parse(line));
    if (process.env.CLAUDE_FIXTURE_MODE === 'idle') {
      send({type:'result',is_error:false});
    }
  });
});
setInterval(() => { if (received && fs.existsSync(release)) finish(); }, 20);
`);
  const quote = value => "'" + value.replaceAll("'", "'\\''") + "'";
  writeFileSync(join(bin, 'claude'), `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(program)} "$@"\n`, {mode:0o755});
  const env = {...process.env, PATH:bin, CLAUDE_FIXTURE_CAPTURE:capture, CLAUDE_FIXTURE_RELEASE:release, CLAUDE_FIXTURE_MODE:mode};
  delete env.NODE_TEST_CONTEXT;
  // Warm the executable before starting the protocol deadline.
  execFileSync(join(bin, 'claude'), ['--fixture-preflight'], {env, timeout:30000});
  const child = spawn(binary, ['run','--vendor','claude','--model','fixture','--prompt','Stay busy until a collaborator supplies the missing result.','--tools','messaging','--duration-ms','15000','--workspace',workspace,'--database',database], {env});
  let stdout = '', stderr = '', closed = false;
  child.stdout.on('data', chunk => stdout += chunk);
  child.stderr.on('data', chunk => stderr += chunk);
  const done = new Promise(resolve => child.on('close', code => {closed = true; resolve(code);}));
  t.after(async () => {
    if (!closed) child.kill('SIGINT');
    const killer = setTimeout(() => {if (!closed) child.kill('SIGKILL');}, 3000);
    await done; clearTimeout(killer);
    rmSync(workspace, {recursive:true,force:true});
  });
  const events = () => stdout.split('\n').slice(0,-1).filter(Boolean).map(JSON.parse);
  const frames = () => existsSync(capture) ? readFileSync(capture, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
  const detail = () => `${stdout}\n${stderr}`;
  await until(() => frames().some(row => row.channel === 'stdin'), detail);
  const recipient = events().find(event => event.type === 'ready').session;
  const call = jsonCall(binary, workspace, database, {stdio:'pipe'});
  const sender = call('join', {name:'reviewer',vendor:'raw'}).id;
  const send = (body, wake) => call('send_message', {to:recipient,body,key:body,reasoning:'Verify native managed delivery',replyRequired:false,...(wake ? {wake} : {})}, sender).id;
  const state = message => {
    const db = new DatabaseSync(database, {readOnly:true});
    try {
      // The worker commits submission/status concurrently, including in DELETE journal mode.
      db.exec('PRAGMA busy_timeout=2500');
      return db.prepare('SELECT d.acknowledgedAt,x.state,x.token FROM deliveries d LEFT JOIN dispatches x ON x.message=d.message AND x.recipient=d.recipient WHERE d.message=? AND d.recipient=?').get(message, recipient);
    }
    finally {db.close();}
  };
  return {frames,events,send,state,detail,release};
}

test('managed Claude receives native peer input while its original turn is still busy', {skip:process.platform==='win32'}, async t => {
  const f = await fixture(t, 'busy');
  assert.equal(f.events().filter(event => event.type === 'turn-completed').length, 0);
  const message = f.send('collaborator-result');
  await until(() => f.frames().some(row => row.channel === 'socket'), f.detail);
  await until(() => f.state(message)?.state === 'submitted', f.detail);
  assert.equal(f.events().filter(event => event.type === 'turn-completed').length, 0, 'delivery waited for a result event');
  const frames = f.frames(), packet = frames.find(row => row.channel === 'socket').frame;
  assert.equal(frames.filter(row => row.channel === 'stdin').length, 1, 'peer content entered stdin as a new user prompt');
  assert.equal(packet.type, 'user');
  assert.equal(packet.from, 'octocode-communication');
  assert.equal(packet.session_id, 'fixture-claude-native');
  assert.equal(packet.priority, 'next');
  assert.equal(packet.message.role, 'user');
  assert.match(packet.message.content, /collaborator-result/);
  assert.ok(packet.uuid); assert.equal(packet.msg_id, packet.uuid);
  const state = f.state(message);
  assert.equal(state.token, packet.uuid);
  assert.equal(state.acknowledgedAt, null, 'socket submission must not acknowledge recipient handling');
  writeFileSync(f.release, 'finish');
  await until(() => f.events().some(event => event.type === 'turn-completed'), f.detail);
});

test('passive-only mail does not start an idle Claude turn; default action uses the peer socket', {skip:process.platform==='win32'}, async t => {
  const f = await fixture(t, 'idle');
  await until(() => f.events().some(event => event.type === 'turn-completed'), f.detail);
  const passive = f.send('passive-context', 'passive');
  await delay(500);
  assert.equal(f.frames().filter(row => row.channel === 'socket').length, 0);
  assert.equal(f.events().filter(event => event.type === 'turn-completed').length, 1);
  assert.equal(f.state(passive).state, null);
  const action = f.send('default-action');
  await until(() => f.frames().some(row => row.channel === 'socket'), f.detail);
  await until(() => f.state(action)?.state === 'submitted', f.detail);
  assert.equal(f.frames().filter(row => row.channel === 'stdin').length, 1);
  assert.match(f.frames().find(row => row.channel === 'socket').frame.message.content, /default-action/);
  assert.equal(f.state(action).acknowledgedAt, null);
  await until(() => f.events().filter(event => event.type === 'turn-completed').length === 2, f.detail);
});
