import { startServer } from '../../../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const R = '/Users/bgaryy/code/octocode';
const s = await startServer();
const log = [];
const A = { id: 'walk-1', goal: 'Find the default time-to-live for clone cache entries.', reasoning: 'Locate the deciding row before reading.',
  resources: [{ id: 'tools', context: { tool: 'localFetch', query: { reasoning: 'unread', path: R + '/docs/OCTOCODE_TOOLS.md', fullContent: true } } }],
  questions: [{ id: 'ttl', questionType: 'locate', target: 'The default time-to-live of clone cache entries.' }] };
let args = A; let calls = 0;
while (args && calls < 4) {
  const e = await s.raw('clasify', args, 'walk' + calls); calls++;
  const sc = e.sc; log.push({ call: 'walk' + calls, ms: e.ms, isError: e.isError, textLen: e.text.length, sc });
  const q = sc?.queries?.[0];
  console.log('walk', calls, 'ms', e.ms, 'isError', e.isError, 'text', JSON.stringify(e.text).slice(0, 80), 'best', JSON.stringify(q?.best), 'next?', !!q?.next?.clasify, 'carry', JSON.stringify(q?.next?.clasify?.carry));
  args = q?.next?.clasify ?? null;
}
// repeat identical first call -> process-local judgment cache
const rep = await s.raw('clasify', A, 'repeat');
log.push({ call: 'repeat', ms: rep.ms, sc: rep.sc });
console.log('repeat ms', rep.ms, 'identical output to walk1:', JSON.stringify(rep.sc) === JSON.stringify(log[0].sc));
// root queries[] with two matrices, ids preserved
const M = { queries: [
  { id: 'mA', goal: 'Classify a claim.', reasoning: 'r', resources: [{ id: 'held1', context: { value: 'The server refuses to start when .octocoderc is invalid JSON.' } }], questions: [{ id: 'isStartupBlock', type: 'noul', instructions: 'The statement says invalid configuration blocks startup.' }] },
  { goal: 'Classify another claim.', reasoning: 'r', resources: [{ context: { value: ['Env vars win over files.', 'Workspace wins over home.'] } }], questions: [{ type: 'score', instructions: 'How many distinct precedence rules are stated?', criteria: ['0', '1', '2', '3+'] }] } ] };
const m = await s.raw('clasify', M, 'multi');
log.push({ call: 'multi', ms: m.ms, isError: m.isError, text: m.text.slice(0, 500), sc: m.sc });
console.log('multi', m.isError, m.text.slice(0, 300), JSON.stringify(m.sc ?? null).slice(0, 1200));
fs.writeFileSync(new URL('./clasify/mcp-clasify.json', import.meta.url), JSON.stringify(log, null, 1));
s.close();
