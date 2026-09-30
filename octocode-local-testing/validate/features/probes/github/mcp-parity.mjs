import { startServer } from '../../../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const OUT = new URL('.', import.meta.url).pathname;
const s = await startServer();
const names = s.tools.map(t => t.name).sort();
const out = { toolCount: names.length, names, hasClone: names.includes('ghCloneRepo') };
// same file read as CLI file1 (cached)
const f = await s.call('ghGetFileContent', { owner: 'sindresorhus', repo: 'is', path: 'package.json', startLine: 1, endLine: 8 });
out.file = { isError: f.isError, bytes: f.bytes, sc: f.sc };
// clone via MCP must be rejected
const c = await s.raw('ghCloneRepo', { queries: [{ goal: 'g', reasoning: 'r', owner: 'sindresorhus', repo: 'is' }] });
out.clone = { isError: c.isError, text: c.text.slice(0, 400) };
// followUp continuation via MCP
const pr = await s.call('ghGetHistoryItem', { operation: 'pullRequest', owner: 'facebook', repo: 'react', number: 37193, content: { body: true }, charLength: 200 });
const nx = pr.sc?.results?.[0]?.data?.next?.continueBody;
out.prNext = nx;
if (nx) { const r = await s.raw(nx.tool, { queries: [nx.query] }); out.prNextRun = { isError: r.isError, text: r.text.slice(0, 500) }; }
fs.writeFileSync(OUT + 'mcp-parity.json', JSON.stringify(out, null, 1));
console.log(JSON.stringify({ toolCount: out.toolCount, names, hasClone: out.hasClone, fileErr: f.isError, cloneErr: out.clone, prNextRun: out.prNextRun?.text?.slice(0, 300) }, null, 1));
s.close();
