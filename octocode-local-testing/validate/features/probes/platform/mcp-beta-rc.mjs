import { startServer } from '../../../../../harness/mcp-client.mjs';
const H = new URL('./cfg/home', import.meta.url).pathname;
const s = await startServer({ env: { OCTOCODE_HOME: H, OCTOCODE_BETA: '' } });
console.log('rc local.beta via scratch OCTOCODE_HOME:', s.tools.map(t => t.name).join(','));
s.close();
