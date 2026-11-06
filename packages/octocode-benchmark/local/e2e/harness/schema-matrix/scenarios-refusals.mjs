// Public refusals on both surfaces. Secrets here are fake and built at run time, so no
// file in the tree trips a scanner; each case checks exactly which bytes went missing.
import { rawPatch, sourceView } from '../responses.mjs';
import { dataOf, errorOf, rows } from './scenario-helpers.mjs';

const FAKE_TOKEN = ['ghp', '_', 'a1B2'.repeat(9)].join('');
const FAKE_KEY = ['AK', 'IA', 'IOSFODNN7EXAMPLE'].join('');
const SHORT = `const keep_before = 1; const token = "${FAKE_TOKEN}"; const keep_after = 2;\n`;
const LONG_SIDE = 'x'.repeat(150_000);
const LONG = `const before = "${LONG_SIDE}"; const token = "${FAKE_TOKEN}"; const after = "${LONG_SIDE}";\n`;

/** Only the secret went: everything around it is there, its bytes and tail are not. */
function onlyTheSecretGone(text, original) {
  if (text.includes(FAKE_TOKEN) || text.includes(FAKE_TOKEN.slice(4)) || text.includes(FAKE_KEY)) return 'the secret is still readable';
  const [head, tail] = original.split(FAKE_TOKEN);
  const start = text.indexOf(head.trim());
  const end = text.lastIndexOf(tail.trim());
  if (start < 0 || end < 0 || end < start) return `bytes around the secret are gone: ${text.slice(0, 160)}`;
  const between = text.slice(start + head.trim().length, end);
  return between.includes('REDACTED') && between.length < 80 ? null : `redaction took more than the secret: ${between.slice(0, 120)}`;
}

const readBack = (pages, field = 'content') => pages.flatMap(page => rows(page.json)).map(row => sourceView({ content: row.data?.[field] ?? '' }).text).join('');
const redactRead = (id, tool) => (query, source, extra = {}) => ({
  id, tool, surfaces: ['cli', 'mcp'],
  files: { 's.ts': source },
  ...extra,
  run: async ({ walk }) => {
    const report = await walk(tool, { queries: [query], responseChars: 40_000 });
    if (report.failures.length) return JSON.stringify(report.failures).slice(0, 300);
    return onlyTheSecretGone(readBack(report.pages), source);
  },
});

const denied = (id, path, code, files) => ({ id, tool: 'localFetch', files, query: { path }, codes: [code] });

const withheldSearch = {
  id: 'refuse.search.credentialFiles', tool: 'localSearch',
  files: { '.env': 'SEEK_ME=1\n', '.npmrc': 'SEEK_ME=1\n', 'keys/server.pem': 'SEEK_ME\n', 'plain.txt': 'SEEK_ME\n' },
  run: async ({ call, rel }) => {
    const result = await call('localSearch', { queries: [{ path: '$DIR', matchString: ['SEEK_ME'], includeHidden: true, view: 'files' }] });
    const data = dataOf(result);
    const files = (data.files ?? []).map(file => file.path);
    const skipped = JSON.stringify(data.skippedPaths ?? {});
    if (files.some(file => /\.env|\.npmrc|\.pem/.test(file))) return `a credential file was searched: ${files}`;
    if (!files.some(file => file.endsWith('plain.txt'))) return `the plain file is missing under ${rel}: ${files}`;
    return skipped.includes('withheld') ? null : `the withheld files are not disclosed: ${skipped}`;
  },
};

const listing = {
  id: 'refuse.structure.credentialFiles', tool: 'structureSearch',
  files: { '.env': 'x\n', 'keep.txt': 'x\n' },
  run: async ({ call }) => {
    const data = dataOf(await call('structureSearch', { queries: [{ path: '$DIR', operation: 'files', includeHidden: true, view: 'paths' }] }));
    const text = JSON.stringify(data.files ?? []);
    return text.includes('.env') || !text.includes('keep.txt') ? `listing ${text}` : null;
  },
};

const OFF = { OCTOCODE_SECURITY_CHECKS: 'false' };
const securityOff = {
  id: 'refuse.securityCheck.off', tool: 'securityCheck', surfaces: ['cli', 'mcp'],
  env: () => OFF,
  run: async ({ call, surface, worker }) => {
    // Every OSV request: the advisory lookup is a GET, which `osvBodies` (POST bodies) never records.
    const osvRequests = () => worker.context.registry.requests.filter(request => request.includes('//api.osv.dev/')).length;
    const before = osvRequests();
    const result = await call('securityCheck', { queries: [{ operation: 'advisory', id: 'GHSA-happy' }] });
    const sent = osvRequests() - before;
    if (surface === 'cli') {
      const text = JSON.stringify(result.json).slice(0, 200);
      if (/tools\.enableSecurityCheck|OCTOCODE_SECURITY_CHECKS/.test(JSON.stringify(result.json))) return `the CLI always offers securityCheck, but it refused: ${text}`;
      if (errorOf(result) || !result.json?.results?.[0]?.data) return `the CLI did not answer the advisory: ${text}`;
      return sent > 0 ? null : 'the CLI answered without asking OSV';
    }
    if (sent) return `the off switch still sent ${sent} requests`;
    const listed = worker.owned.mcpServerFor(worker.server).tools.map(tool => tool.name);
    if (listed.includes('securityCheck')) return 'MCP still lists securityCheck with the switch off';
    return result.isError ? null : `MCP answered a tool it does not list: ${JSON.stringify(result.json).slice(0, 200)}`;
  },
};

const NO_KEY = { OCTOCODE_CLASSIFICATION_API: '' };
const clasifyOff = {
  id: 'refuse.clasify.noKey', tool: 'clasify', surfaces: ['cli', 'mcp'], env: () => NO_KEY,
  run: async ({ call, surface, worker }) => {
    const before = worker.context.mock.classificationRequests.length;
    const result = await call('clasify', { queries: [{ operation: 'evaluate', content: 'x', questions: { k: { type: 'noul', instructions: 'x?' } } }] });
    if (worker.context.mock.classificationRequests.length !== before) return 'a provider request left without a key';
    if (surface === 'mcp') {
      const listed = worker.owned.mcpServerFor(worker.server).tools;
      if (listed.some(tool => tool.name === 'clasify')) return 'MCP still lists clasify without a key';
      const search = listed.find(tool => tool.name === 'localSearch');
      if (JSON.stringify(search?.inputSchema ?? {}).includes('"classification"')) return 'MCP localSearch still offers classification without a key';
      return result.isError ? null : `MCP answered a tool it does not list: ${JSON.stringify(result.json).slice(0, 200)}`;
    }
    return errorOf(result) === 'missingConfiguration' ? null : `CLI answered ${errorOf(result)}: ${JSON.stringify(result.json).slice(0, 200)}`;
  },
};

const REMOTE_REPO = 'matrix';
const remoteFile = text => request => (request.p === `/repos/octo/${REMOTE_REPO}/contents/secret.ts`
  ? { body: { type: 'file', encoding: 'base64', path: 'secret.ts', size: Buffer.byteLength(text), sha: 'a'.repeat(40), content: Buffer.from(text).toString('base64') } } : null);

const patchFile = text => ({ sha: 'b'.repeat(40), filename: 'secret.ts', status: 'modified', additions: 1, deletions: 0, changes: 1, patch: `@@ -1 +1 @@\n+${text}` });
const secretInPatch = {
  id: 'refuse.secret.patch', tool: 'ghGetHistoryItem',
  github: request => (request.p === '/repos/octo/demo/pulls/7/files' ? { body: [patchFile(SHORT.trim())] } : null),
  run: async ({ walk }) => {
    const report = await walk('ghGetHistoryItem', { queries: [{ owner: 'octo', repo: 'demo', operation: 'pullRequest', number: 7, sections: ['patches'] }] });
    const patch = report.pages.flatMap(page => rows(page.json)).flatMap(row => row.data?.pullRequests ?? []).flatMap(item => item.files ?? []).map(file => rawPatch(file.patch ?? '')).join('');
    return onlyTheSecretGone(patch, SHORT.trim());
  },
};

export const REFUSAL_SCENARIOS = [
  redactRead('refuse.secret.localFetch', 'localFetch')({ path: '$DIR/s.ts' }, SHORT),
  redactRead('refuse.secret.localFetch.longLine', 'localFetch')({ path: '$DIR/s.ts' }, LONG),
  { id: 'refuse.secret.localSearch', tool: 'localSearch', files: { 's.ts': SHORT }, run: async ({ call }) => onlyTheSecretGone(dataOf(await call('localSearch', { queries: [{ path: '$DIR', matchString: ['keep_before'] }] })).files?.[0]?.matches?.[0]?.value ?? '', SHORT) },
  { id: 'refuse.secret.localSearch.matchOnly', tool: 'localSearch', files: { 's.ts': SHORT }, run: async ({ call }) => { const text = JSON.stringify(dataOf(await call('localSearch', { queries: [{ path: '$DIR', matchString: ['ghp_[A-Za-z0-9]+'], regexEngine: 'rust', view: 'matchOnly' }] }))); return text.includes(FAKE_TOKEN) ? 'matchOnly returned the secret' : null; } },
  { id: 'refuse.secret.astSearch', tool: 'astSearch', files: { 's.ts': SHORT }, run: async ({ call }) => onlyTheSecretGone(JSON.stringify(dataOf(await call('astSearch', { queries: [{ path: '$DIR', operation: 'match', parser: 'typescript', rule: { kind: 'program' } }] }))).replaceAll('\\"', '"'), SHORT) },
  redactRead('refuse.secret.ghGetFileContent', 'ghGetFileContent')({ owner: 'octo', repo: REMOTE_REPO, path: 'secret.ts' }, SHORT, { github: remoteFile(SHORT), files: {} }),
  secretInPatch,
  denied('refuse.path.outside', '/etc/hosts', 'outsideAllowedRoots'),
  denied('refuse.path.traversal', '../../../../etc/hosts', 'outsideAllowedRoots'),
  denied('refuse.path.symlink', '$DIR/link', 'symlinkEscape', { link: { symlink: '/etc/hosts' } }),
  denied('refuse.path.dotenv', '$DIR/.env', 'pathPolicyDenied', { '.env': 'SECRET=1\n' }),
  denied('refuse.path.sshKey', '$DIR/.ssh/id_rsa', 'pathPolicyDenied', { '.ssh/id_rsa': 'key\n' }),
  denied('refuse.path.awsCredentials', '$DIR/.aws/credentials', 'pathPolicyDenied', { '.aws/credentials': 'x\n' }),
  denied('refuse.path.npmrc', '$DIR/.npmrc', 'pathPolicyDenied', { '.npmrc': 'x\n' }),
  denied('refuse.path.pem', '$DIR/server.pem', 'pathPolicyDenied', { 'server.pem': 'x\n' }),
  { id: 'refuse.search.outside', tool: 'localSearch', query: { path: '/etc', matchString: ['root'] }, codes: ['outsideAllowedRoots'] },
  { id: 'refuse.structure.outside', tool: 'structureSearch', query: { path: '/etc', operation: 'files' }, codes: ['outsideAllowedRoots'] },
  { id: 'refuse.ast.outside', tool: 'astSearch', query: { path: '/etc', operation: 'symbols' }, codes: ['outsideAllowedRoots'] },
  withheldSearch,
  listing,
  securityOff,
  clasifyOff,
];
