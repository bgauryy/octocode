import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const [referenceServer, nativeServer, addon, corpusPath, reportPath] = process.argv.slice(2);
const corpus = JSON.parse(await readFile(corpusPath, 'utf8'));
const root = new URL('.', `file://${corpusPath}`).pathname;
const env = {
  PATH: '/usr/bin:/bin', HOME: `${root}reference-home`, OCTOCODE_HOME: `${root}reference-home`,
  ENABLE_LOCAL: 'true', ENABLE_CLONE: 'false', OCTOCODE_ENABLE_STATS: 'false',
  ALLOWED_PATHS: root, WORKSPACE_ROOT: root,
};
async function connect(server, extra = {}) {
  const transport = new StdioClientTransport({ command: process.execPath, args: [server], cwd: root, env: { ...env, ...extra }, stderr: 'pipe' });
  let stderr = '';
  transport.stderr?.on('data', data => { stderr += data; });
  const client = new Client({ name: 'octocode-native-parity', version: '1.0.0' });
  await client.connect(transport);
  const tools = await client.listTools();
  assert(tools.tools.some(tool => tool.name === 'localFetch'), `Missing real localFetch: ${stderr}`);
  return { client, stderr: () => stderr };
}
const reference = await connect(referenceServer);
let candidate;
const cases = [];
try {
  candidate = await connect(nativeServer, { OCTOCODE_NATIVE_BINDING: addon });
  for (const item of corpus.cases.filter(row => !['invalid-range', 'unknown-field'].includes(row.id))) {
    const arguments_ = { queries: [{ ...item.query, goal: 'Parity fixture', reasoning: 'Frozen comparison' }] };
    const expected = await reference.client.callTool({ name: 'localFetch', arguments: arguments_ });
    const actual = await candidate.client.callTool({ name: 'localFetch', arguments: arguments_ });
    let equal = true;
    try { assert.deepEqual(actual, expected); } catch { equal = false; }
    cases.push({ id: item.id, equal, expected, actual });
  }
} finally {
  await reference.client.close();
  await candidate?.client.close();
}
const report = { scope: 'Real stdio MCP localFetch parity', passed: cases.filter(row => row.equal).length, total: cases.length, cases };
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ scope: report.scope, passed: report.passed, total: report.total }));
process.exitCode = report.passed === report.total ? 0 : 1;
