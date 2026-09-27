import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const script = fileURLToPath(new URL('../../src/service-mesh.mjs', import.meta.url));
function plan(input = {}) {
  const env = {...process.env};
  for (const key of ['COMMUNICATION_VENDORS', 'COMMUNICATION_AGENT_ORIGINATED', 'COMMUNICATION_OPENCODE_COMMAND', 'COMMUNICATION_PI_MODEL', 'COMMUNICATION_COPIES', 'COMMUNICATION_COMPLETION_CHECK', 'COMMUNICATION_SCOPED_SKILL', 'COMMUNICATION_TASK_FAMILY', 'COMMUNICATION_VENDOR_COPIES', 'COMMUNICATION_REVIEW_MANIFEST', 'COMMUNICATION_FEATURE_CHECK']) delete env[key];
  return JSON.parse(execFileSync(process.execPath, [script, '--plan'], {env: {...env, ...input}, encoding: 'utf8', timeout: 5000, stdio: ['ignore', 'pipe', 'pipe']}));
}
test('default native vendors and raw fallback retain the historical nine-peer matrix', () => {
  const p = plan(); assert.deepEqual(p.vendors, ['claude', 'codex', 'grok', 'pi']); assert.equal(p.rawPeer, true); assert.equal(p.requestEdges, 72);
});
test('explicit six native collaborators do not require Pi or include a raw peer', () => {
  const p = plan({COMMUNICATION_VENDORS: 'claude,codex,grok', COMMUNICATION_AGENT_ORIGINATED: '1'});
  assert.equal(p.peers, 6); assert.equal(p.requestEdges, 30); assert.equal(p.controllerIdentities, 1); assert.equal(p.agentOriginated, true); assert.equal(p.rawPeer, false);
});
test('optional OpenCode retains the historical eleven-peer default', () => assert.equal(plan({COMMUNICATION_OPENCODE_COMMAND: '/fixture/opencode'}).requestEdges, 110));
test('matched small groups retain every directed pair without inventing a second evaluator', () => {
  const p = plan({COMMUNICATION_VENDORS:'claude,codex,grok',COMMUNICATION_AGENT_ORIGINATED:'1',COMMUNICATION_COPIES:'1'});
  assert.equal(p.peers,3);assert.equal(p.requestEdges,6);
  assert.throws(()=>plan({COMMUNICATION_COPIES:'0'}));
});
test('invalid, duplicate and implicit agent-originated selections fail before starting hosts', () => {
  for (const selection of ['', 'codex,codex', 'raw', 'unknown']) assert.throws(() => plan({COMMUNICATION_VENDORS: selection}));
  assert.throws(() => plan({COMMUNICATION_AGENT_ORIGINATED: '1'}));
  for (const selection of ['claude,codex,grok,opencode']) assert.throws(() => plan({COMMUNICATION_VENDORS: selection, COMMUNICATION_AGENT_ORIGINATED: '1'}));
});

test('agent-originated Pi and two same-vendor workers use supported native receipts', () => {
  const all = plan({ COMMUNICATION_VENDORS: 'claude,codex,grok,pi', COMMUNICATION_AGENT_ORIGINATED: '1' });
  assert.equal(all.peers, 8); assert.equal(all.requestEdges, 56);
  const pair = plan({ COMMUNICATION_VENDORS: 'codex', COMMUNICATION_AGENT_ORIGINATED: '1' });
  assert.equal(pair.peers, 2); assert.equal(pair.requestEdges, 2);
  assert.throws(() => plan({ COMMUNICATION_VENDORS: 'codex', COMMUNICATION_AGENT_ORIGINATED: '1', COMMUNICATION_COPIES: '1' }));
});

test('six cross-vendor code reviewers form a bounded native challenge ring',()=>{
 const args={COMMUNICATION_VENDORS:'claude,codex,grok,pi',COMMUNICATION_AGENT_ORIGINATED:'1',COMMUNICATION_TASK_FAMILY:'code-review',COMMUNICATION_COPIES:'1',COMMUNICATION_VENDOR_COPIES:'{"claude":2,"codex":2}'};
 const p=plan(args);assert.equal(p.peers,6);assert.equal(p.requestEdges,6);assert.equal(p.rawPeer,false);
 assert.throws(()=>plan({...args,COMMUNICATION_VENDOR_COPIES:'{"unknown":1}'}));
 assert.throws(()=>plan({...args,COMMUNICATION_VENDOR_COPIES:'{"claude":3}'}));
 assert.throws(()=>plan({...args,COMMUNICATION_AGENT_ORIGINATED:'0'}));
});

test('production qualification requires native agent-originated review before any host starts', () => {
  const input = {COMMUNICATION_VENDORS:'grok,claude,codex,pi', COMMUNICATION_AGENT_ORIGINATED:'1', COMMUNICATION_FEATURE_CHECK:'1'};
  const p=plan(input); assert.equal(p.featureCheck,true); assert.equal(p.peers,8); assert.equal(p.requestEdges,56);
  assert.throws(()=>plan({...input,COMMUNICATION_AGENT_ORIGINATED:'0'}));
  assert.throws(()=>plan({...input,COMMUNICATION_TASK_FAMILY:'code-review'}));
});
