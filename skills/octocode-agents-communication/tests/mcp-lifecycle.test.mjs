import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { nativeBinary as binary, binary as launcher, tempWorkspace } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-mcp-lifecycle-');
  const database = join(workspace, 'communication.sqlite');
  const flags = ['--workspace', workspace, '--database', database];
  const call = (...args) => JSON.parse(execFileSync(binary, [...args, ...flags], {encoding:'utf8',stdio:['pipe','pipe','pipe']}));
  return {workspace, database, flags, call};
}
function start(t, f, args) {
  const child = spawn(binary, ['mcp', ...args, ...f.flags]);
  t.after(() => { if (child.exitCode === null) child.kill('SIGKILL'); });
  let stdout = '', stderr = '';
  child.stdout.on('data', data => stdout += data);
  child.stderr.on('data', data => stderr += data);
  const closed = new Promise((resolve,reject) => { child.once('error',reject);child.once('close',code=>resolve({code,stdout,stderr})); });
  async function ready() {
    const deadline = Date.now() + 10000;
    while (Date.now() < deadline) {
      const line = stderr.split('\n').slice(0,-1).find(line => line.includes('mcp_ready'));
      if (line) return JSON.parse(line);
      if (child.exitCode !== null) throw new Error(stderr);
      await new Promise(resolve=>setTimeout(resolve,20));
    }
    throw new Error(`No readiness: ${stderr}`);
  }
  return {child, closed, ready};
}
const initialize = JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'test',version:'1'}}})+'\n';

test('managed MCP joins once, binds tools, keeps stdout pure and leaves on EOF', async t => {
  const f = fixture(t), worker = start(t,f,['--managed','--name','worker','--vendor','generic','--tools','peers,inbox']);
  const ready = await worker.ready();
  assert.equal(ready.delivery,'manual-inbox');assert.equal(ready.automaticWake,false);
  worker.child.stdin.end(initialize + JSON.stringify({jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'peers',arguments:{}}})+'\n');
  const result = await worker.closed;assert.equal(result.code,0,result.stderr);
  const frames = result.stdout.trim().split('\n').map(JSON.parse);
  assert.equal(frames.length,2);assert.ok(frames.every(frame=>frame.jsonrpc==='2.0'));
  assert.match(frames[0].result.instructions,new RegExp(ready.session));
  const peers=JSON.parse(frames[1].result.content[0].text);assert.equal(peers.items[0].id,ready.session);
  assert.equal(f.call('peers').items.length,0);
});

test('managed MCP refuses another owner without ending its identity and leaves on signal', async t => {
  const f=fixture(t), worker=start(t,f,['--managed','--name','worker','--vendor','generic']);
  const ready=await worker.ready();
  const duplicate=start(t,f,['--managed','--session',ready.session,'--vendor','generic']);duplicate.child.stdin.end();
  const rejected=await duplicate.closed;assert.equal(rejected.code,1);assert.match(rejected.stderr,/Another delivery owner/);
  assert.equal(f.call('peers').items[0].id,ready.session);
  worker.child.kill('SIGTERM');const ended=await worker.closed;assert.equal(ended.code,0,ended.stderr);
  assert.equal(f.call('peers').items.length,0);
});

test('managed MCP resumes expired identity and removes stale leases; plain MCP leaves live identity untouched', async t => {
  const f=fixture(t), agent=f.call('join',JSON.stringify({name:'worker',vendor:'generic'}));
  f.call('lock',JSON.stringify({path:'owned.txt',reasoning:'Verify stale lease recovery'}),'--session',agent.id);
  const db=new DatabaseSync(f.database);t.after(()=>db.close());
  db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(agent.id);
  const worker=start(t,f,['--managed','--session',agent.id,'--vendor','generic']);await worker.ready();
  assert.equal(db.prepare('SELECT count(*) AS n FROM leases WHERE owner=?').get(agent.id).n,0);
  assert.equal(f.call('peers').items[0].id,agent.id);worker.child.stdin.end();assert.equal((await worker.closed).code,0);
  f.call('resume','{"vendor":"generic"}','--session',agent.id);
  const plain=start(t,f,['--session',agent.id]);plain.child.stdin.end(initialize);const result=await plain.closed;
  assert.equal(result.code,0,result.stderr);assert.equal(result.stderr,'');assert.equal(f.call('peers').items[0].id,agent.id);
});

test('managed MCP maintains idle presence and fails closed on externally expired identity', async t => {
  const f=fixture(t),worker=start(t,f,['--managed','--name','worker','--vendor','generic']);
  const ready=await worker.ready(), db=new DatabaseSync(f.database);t.after(()=>db.close());
  const initial=db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(ready.session).expiresAt;
  await new Promise(resolve=>setTimeout(resolve,16000));
  assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(ready.session).expiresAt>initial);
  f.call('leave','--session',ready.session);
  const result=await worker.closed;assert.equal(result.code,1);assert.match(result.stderr,/expired session/);
  assert.equal(f.call('peers').items.length,0);
});


test('shipped launcher exposes managed stdio MCP with discovery, calls and EOF cleanup', t => {
  const f=fixture(t);
  const requests=initialize+[
    {method:'notifications/initialized'},
    {id:2,method:'tools/list'},
    {id:3,method:'tools/call',params:{name:'set_status',arguments:{status:'busy',task:'Verify launcher binding'}}},
    {id:4,method:'tools/call',params:{name:'inbox',arguments:{}}},
  ].map(frame=>JSON.stringify({jsonrpc:'2.0',...frame})).join('\n')+'\n';
  const frames=execFileSync(launcher,['mcp','--managed','--name','launcher-check','--vendor','generic','--tools','messaging',...f.flags],{input:requests,encoding:'utf8',timeout:10000,stdio:['pipe','pipe','pipe']}).trim().split('\n').map(JSON.parse);
  assert.equal(frames.length,4,'Notifications emit no stdout frame');
  assert.deepEqual(frames[0].result.capabilities,{tools:{}});
  assert.deepEqual(frames[1].result.tools.map(tool=>tool.name),['peers','set_status','send_message','inbox','complete']);
  for(const frame of frames)assert.equal(frame.jsonrpc,'2.0');
  for(const frame of frames.slice(2))assert.equal(frame.result.isError,undefined);
  assert.equal(f.call('peers').items.length,0,'EOF releases the managed identity');
});
