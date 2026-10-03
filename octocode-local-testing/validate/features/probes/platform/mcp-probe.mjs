import { startServer } from '../../../../../harness/mcp-client.mjs';
import fs from 'node:fs';
const R = '/Users/bgaryy/code/octocode';
const out = {};
const short = (e) => ({ isError: e.isError, rowErrors: e.rowErrors, text: e.text.slice(0, 400), sc: JSON.stringify(e.sc)?.slice(0, 400) });
async function variant(name, env) {
  const s = await startServer({ env });
  out[name] = { tools: s.tools.map(t => t.name), instructionsLen: s.init.instructions?.length };
  if (name === 'default') {
    fs.writeFileSync(new URL('./mcp-instructions.txt', import.meta.url), s.init.instructions ?? '');
    const G = { goal: 'g', reasoning: 'r' };
    const cases = {
      missingGoal: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'clasify' }] }],
      sixQueries: ['localSearch', { queries: Array.from({ length: 6 }, (_, i) => ({ path: R + '/docs', searchText: 'x' + i, ...G })) }],
      unknownField: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'clasify', bogusField: 1, ...G }] }],
      outOfRange: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'clasify', contextLines: 9999, ...G }] }],
      notFound: ['localFetch', { queries: [{ path: R + '/nope-does-not-exist.md', ...G }] }],
      empty: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'zzqqxx_never_present_42', ...G }] }],
      unknownTool: ['fooTool', { queries: [{ ...G }] }],
      betaTool: ['astTopology', { queries: [{ analysis: 'cycles', path: R + '/docs', ...G }] }],
      cloneTool: ['ghCloneRepo', { queries: [{ owner: 'octocat', repo: 'Hello-World', ...G }] }],
      followUpNoGoal: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'OCTOCODE_BETA', followUp: true }] }],
      batchRow1NoGoal: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'OCTOCODE_BETA', ...G }, { path: R + '/docs', searchText: 'DISABLE_TOOLS' }] }],
      parity: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'OCTOCODE_TRUST_PROJECT_LSP_CONFIG', ...G }] }],
      parityDebug: ['localSearch', { queries: [{ path: R + '/docs', searchText: 'OCTOCODE_TRUST_PROJECT_LSP_CONFIG', debug: true, ...G }] }],
    };
    out.cases = {};
    for (const [k, [tool, args]] of Object.entries(cases)) {
      const e = await s.raw(tool, args, k);
      out.cases[k] = short(e);
      if (k.startsWith('parity')) fs.writeFileSync(new URL(`./mcp-${k}.json`, import.meta.url), JSON.stringify(e.sc));
    }
  }
  out[name].stderr = s.stderr().slice(0, 600);
  s.close();
}
await variant('default', {});
await variant('beta', { OCTOCODE_BETA: 'true' });
await variant('noKey', { OCTOCODE_CLASSIFICATION_API: '' });
await variant('noKeyBothBlank', { OCTOCODE_CLASSIFICATION_API: '', OCTOCODE_JEV_KEY: '' });
await variant('disableEnv', { DISABLE_TOOLS: 'ghSearchRepo' });
fs.writeFileSync(new URL('./mcp-probe.json', import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(out, null, 1).slice(0, 12000));
