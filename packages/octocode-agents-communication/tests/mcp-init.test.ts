import assert from 'node:assert/strict';
import { once } from 'node:events';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { test, type TestContext } from 'node:test';
import { binary, execFileSync, spawn, tempWorkspace } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-init-'), database = join(workspace, 'db.sqlite');
  const flags = ['--workspace', workspace, '--database', database];
  const cli = (command: string, input: object = {}, session?: string): JsonRecord => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), ...flags, ...(session ? ['--session', session] : [])], {encoding:'utf8',stdio:['pipe','pipe','pipe']}));
  return { workspace, database, flags, cli };
}
async function connect(t: TestContext, f: ReturnType<typeof fixture>, extra: string[] = []) {
  const child = spawn(binary, ['mcp', ...f.flags, ...extra]);
  let stderr = '';
  child.stderr.on('data', chunk => stderr += chunk);
  const exited = once(child, 'close');
  t.after(async () => { if (child.exitCode === null) child.kill('SIGKILL'); await exited; });
  const lines = createInterface({input:child.stdout})[Symbol.asyncIterator]();
  const notifications: JsonRecord[] = [];
  let id = 0;
  async function request(method: string, params: object = {}): Promise<JsonRecord> {
    child.stdin.write(JSON.stringify({jsonrpc:'2.0', id:++id, method, params})+'\n');
    while (true) {
      const line = await lines.next();assert.equal(line.done,false,stderr);
      const response: JsonRecord = JSON.parse(line.value);
      if (!('id' in response)) { notifications.push(response);continue; }
      assert.equal(response.id,id);assert.equal(response.error,undefined,JSON.stringify(response));
      return response.result;
    }
  }
  const initialized = await request('initialize');
  const call = (name: string, input: object = {}) => request('tools/call',{name,arguments:input});
  const close = async () => {child.stdin.end();const [code]=await exited;assert.equal(code,0,stderr);};
  return { request, initialized, call, notifications, close };
}
const value = (result: JsonRecord): JsonRecord => {assert.equal(result.isError,undefined,result.content[0].text);return JSON.parse(result.content[0].text);};

test('unbound MCP exposes only init, creates nothing until called and changes discovery once', {timeout:30000}, async t => {
  const f=fixture(t), client=await connect(t,f,['--tools','messaging']);
  assert.deepEqual(client.initialized.capabilities,{tools:{listChanged:true}});
  assert.deepEqual((await client.request('tools/list')).tools.map((x:JsonRecord)=>x.name),['init']);
  assert.equal(existsSync(f.database),false);
  assert.equal((await client.call('agents',{type:'list',input:{}})).isError,true);
  assert.equal((await client.call('init',{name:'worker'})).isError,true);
  assert.equal(existsSync(f.database),false,'invalid init does not open storage');
  const binding=value(await client.call('init',{name:'worker',vendor:'generic',task:'Review code'}));
  assert.equal(binding.managed,true);assert.match(binding.instructions,/Communication binding/);
  assert.deepEqual((await client.request('tools/list')).tools.map((x:JsonRecord)=>x.name),['agents','messages','records']);
  assert.deepEqual(client.notifications.map(x=>x.method),['notifications/tools/list_changed']);
  assert.equal(value(await client.call('agents',{type:'list',input:{}})).items[0].id,binding.session);
  assert.equal((await client.call('init',{name:'duplicate',vendor:'generic'})).isError,true);
  assert.equal(f.cli('peers').items.length,1);
  await client.close();assert.equal(f.cli('peers').items.length,0);
});

test('explicit binding omits init and exact host identity reuse leaves the external owner intact', {timeout:30000}, async t => {
  const f=fixture(t), host=f.cli('join',{name:'host',vendor:'codex',vendorSession:'exact-host-id'});
  const bound=await connect(t,f,['--session',host.id]);
  assert.ok((await bound.request('tools/list')).tools.every((x:JsonRecord)=>x.name!=='init'));
  assert.equal((await bound.call('init',{name:'duplicate',vendor:'codex'})).isError,true);
  await bound.close();
  const pending=await connect(t,f);
  const reused=value(await pending.call('init',{name:'ignored',vendor:'codex',vendorSession:'exact-host-id'}));
  assert.equal(reused.session,host.id);assert.equal(reused.managed,false);
  assert.match(reused.instructions,/external lifecycle owner/);
  await pending.close();assert.equal(f.cli('peers').items[0].id,host.id);
});

test('ambiguous or expired exact host binding cannot create a duplicate; failed init stays recoverable', {timeout:30000}, async t => {
  const f=fixture(t), a=f.cli('join',{name:'a',vendor:'codex',vendorSession:'ambiguous'});
  f.cli('join',{name:'b',vendor:'codex',vendorSession:'ambiguous'});
  const expired=f.cli('join',{name:'expired',vendor:'codex',vendorSession:'expired'});
  f.cli('leave',{},expired.id);
  const client=await connect(t,f);
  for (const vendorSession of ['ambiguous','expired']) {
    assert.equal((await client.call('init',{name:'candidate',vendor:'codex',vendorSession})).isError,true);
    assert.deepEqual((await client.request('tools/list')).tools.map((x:JsonRecord)=>x.name),['init']);
    assert.equal(f.cli('peers').items.length,2);
  }
  const initialized=value(await client.call('init',{name:'fresh',vendor:'generic'}));
  assert.notEqual(initialized.session,a.id);await client.close();assert.equal(f.cli('peers').items.length,2);
});

test('concurrent exact host initialization resolves one identity and only one lifecycle owner', {timeout:30000}, async t => {
  const f=fixture(t);f.cli('join',{name:'seed',vendor:'generic'});
  const first=await connect(t,f),second=await connect(t,f);
  const answers=await Promise.all([first.call('init',{name:'first',vendor:'generic',vendorSession:'shared-host'}),second.call('init',{name:'second',vendor:'generic',vendorSession:'shared-host'})]);
  const bindings=answers.map(value);assert.equal(bindings[0].session,bindings[1].session);
  assert.equal(bindings.filter(x=>x.managed).length,1);
  const clients=[first,second],owner=bindings.findIndex(x=>x.managed),borrower=1-owner;
  await clients[borrower].close();assert.ok(f.cli('peers').items.some((x:JsonRecord)=>x.id===bindings[owner].session));
  await clients[owner].close();assert.equal(f.cli('peers').items.length,1);
});
