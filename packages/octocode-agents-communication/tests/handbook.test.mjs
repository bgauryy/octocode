import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync, root, tempWorkspace } from './helpers.mjs';

const instructions = readFileSync(join(root, 'OPERATING.md'), 'utf8');
const reference = join(root, 'scripts/docs/COMMANDS.md');
const section = readFileSync(reference, 'utf8');
const rows = [...section.matchAll(/^\| `([^`]+)` \| ([^|]+) \| `([^`]+)` \|$/gm)]
  .map(([, name, when, example]) => ({ name, when, example }));
const sample = name => {
  const row = rows.find(row => row.name === name);
  assert.ok(row, `No handbook example for ${name}`);
  const json = row.example.match(/'(\{.*\})'/)?.[1];
  return json ? JSON.parse(json.replace(/\bLEASE_ID\b/g, '1').replace(/EXACT_UUID/g, '00000000-0000-4000-8000-000000000001')) : {};
};

test('the command reference covers the public CLI and its JSON examples pass actual command validation', () => {
  const program = `import sys,json
sys.path.insert(0,sys.argv[1])
from communication import catalog
rows=json.load(sys.stdin)
assert sorted(row['name'] for row in rows)==sorted(item['name'] for item in catalog.catalog()['commands'])
for row in rows:
 if row['json']: catalog.command(row['name'],row['input'])
print(len(rows))
`;
  const result = execFileSync('python3', ['-B', '-c', program, join(root, 'scripts')], {
    input: JSON.stringify(rows.map(row => ({ name: row.name, json: row.example.includes("'{"), input: sample(row.name) }))),
    encoding: 'utf8', stdio: 'pipe',
  });
  assert.equal(Number(result), rows.length);
  assert.equal(new Set(rows.map(row => row.name)).size, rows.length);
});

test('referenced examples communicate and search from a standalone copied skill', t => {
  const workspace = tempWorkspace(t, 'communication-handbook-', { real: true });
  const standalone = join(workspace, 'octocode-agents-communication');
  for (const entry of ['OPERATING.md', 'scripts']) cpSync(join(root, entry), join(standalone, entry), { recursive: true });
  const copiedInstructions = readFileSync(join(standalone, 'OPERATING.md'), 'utf8');
  const routes = [...copiedInstructions.matchAll(/\[[^\]]+\]\((scripts\/docs\/[^)]+)\)/g)].map(match => match[1]);
  assert.ok(routes.includes('scripts/docs/COMMANDS.md'), 'Compact skill routes command discovery to the portable reference');
  for (const route of routes) assert.ok(readFileSync(join(standalone, route), 'utf8').length, `Missing standalone reference: ${route}`);
  assert.equal(copiedInstructions, instructions);
  const database = join(workspace, 'shared.sqlite');
  const call = (name, input = sample(name), session) => JSON.parse(execFileSync('python3', [
    '-B', join(standalone, 'scripts/communication.py'), ...name.split(' '), ...(name === 'db info' ? [] : [JSON.stringify(input)]),
    '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : []),
  ], { cwd: workspace, encoding: 'utf8', stdio: 'pipe' }));
  assert.equal(call('db info').exists, false);
  const author = call('join').id, recipient = call('join', { ...sample('join'), name: 'recipient' }).id;
  call('attach', sample('attach'), author); call('attach', sample('attach'), recipient);
  assert.equal(call('binding', {}, author).id, author);
  call('set_status', sample('set_status'), author);
  call('heartbeat', sample('heartbeat'), author);
  const sent = call('send_message', { ...sample('send_message'), to: recipient }, author);
  const incoming = call('fetch', { incoming: true, type: 'message', limit: 20 }, recipient);
  assert.equal(incoming.items.length, 1);
  const message = incoming.items[0].data.messageId;
  assert.ok(Number.isSafeInteger(incoming.items[0].recordId));
  assert.ok(!Object.hasOwn(incoming.items[0], 'id'), 'History exposes recordId, never a bare id usable by complete');
  assert.equal(call('inbox', { message }, recipient).items.length, 1);
  call('complete', { ...sample('complete'), message }, recipient);
  assert.equal(call('fetch', { incoming: true, type: 'message' }, recipient).items.length, 0);
  const answer = call('fetch', { incoming: true, type: 'message' }, author).items[0];
  assert.equal(answer.data.replyTo, message);
  call('complete', { messages: [answer.data.messageId] }, author);
  call('record', sample('record'), author);
  const memories = call('fetch', { type: 'memory', search: 'API', branch: 'feature/api' }, author);
  assert.equal(memories.items.length, 1);
  assert.equal(memories.items[0].data.content, sample('record').data.content);
  assert.equal(memories.items[0].from, author); assert.equal(memories.items[0].path, workspace);
  const leased = call('lock', sample('lock'), author);
  assert.equal(leased.ok, true);
  assert.equal(call('check_write', sample('check_write'), author).ok, true);
  call('unlock', { leaseId: leased.lease.id }, author);
  call('leave', {}, recipient); call('leave', {}, author);
  assert.equal(call('peers').items.length, 0);
  assert.ok(sent);
});
