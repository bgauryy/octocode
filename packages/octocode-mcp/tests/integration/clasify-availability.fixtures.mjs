import assert from 'node:assert/strict';
/** Expected policy matrix; independent of runtime availability implementation. */
const credential = 'OCTOCODE_CLASSIFICATION_API=fixture-key\n';
const keyed = (name, env = {}, rest = {}) => ({
  name,
  env: { OCTOCODE_CLASSIFICATION_API: 'fixture-key', ...env },
  key: true,
  ...rest,
});
const filters = 'clasify,ghCloneRepo,graphBuild';
const cloneConfig = { tools: { enableClone: true } };
export const cases = [
  keyed('process-key-beta-unset'),
  keyed(
    'local-tools-disabled',
    { OCTOCODE_ENABLE_LOCAL: 'false' },
    { local: false }
  ),
  keyed('provider-rejects-key', {}, { providerStatus: 401 }),
  keyed('provider-down', {}, { providerStatus: 503 }),
  keyed('provider-unreachable', {}, { disconnect: true }),
  { name: 'absent', key: false },
  ...['', '   '].map((value, i) => ({
    name: i ? 'whitespace' : 'empty',
    env: { OCTOCODE_CLASSIFICATION_API: value },
    key: false,
  })),
  keyed('process-key-beta-off', { OCTOCODE_BETA: 'false' }),
  keyed('process-key-beta-on', { OCTOCODE_BETA: 'true' }, { beta: true }),
  keyed(
    'hidden-public-clasify-keeps-context',
    { DISABLE_TOOLS: 'clasify' },
    { deny: ['clasify'] }
  ),
  { name: 'home-env-key', homeEnv: credential, key: true },
  { name: 'project-env-key', projectEnv: credential, key: true },
  {
    name: 'config-key',
    config: { classification: { api: 'fixture-key' } },
    key: true,
  },
  {
    name: 'process-empty-overrides-home-and-config',
    env: { OCTOCODE_CLASSIFICATION_API: '' },
    homeEnv: 'OCTOCODE_CLASSIFICATION_API=fixture-home\n',
    config: { classification: { api: 'fixture-config' } },
    key: false,
  },
  keyed(
    'key-with-no-classification-route',
    { TOOLS_TO_RUN: 'ghSearchHistory' },
    { allow: ['ghSearchHistory'] }
  ),
  keyed('standalone-only', { TOOLS_TO_RUN: 'clasify' }, { allow: ['clasify'] }),
  {
    name: 'retired-jev-key-is-not-a-capability',
    homeEnv: 'OCTOCODE_JEV_KEY=fixture-key\n',
    key: false,
  },
  keyed('clone-env-opt-in', { OCTOCODE_ENABLE_CLONE: 'true' }, { clone: true }),
  keyed('clone-config-opt-in', {}, { config: cloneConfig, clone: true }),
  keyed(
    'clone-env-off-overrides-config',
    { OCTOCODE_ENABLE_CLONE: 'false' },
    { config: cloneConfig }
  ),
  keyed(
    'all-mcp-opt-ins',
    { OCTOCODE_BETA: 'true', OCTOCODE_ENABLE_CLONE: 'true' },
    { beta: true, clone: true }
  ),
  {
    name: 'allowlist-does-not-enable-clone',
    env: { TOOLS_TO_RUN: 'ghCloneRepo' },
    key: false,
    allow: ['ghCloneRepo'],
  },
  keyed(
    'deny-beats-opt-ins-and-allowlist',
    {
      OCTOCODE_BETA: 'true',
      OCTOCODE_ENABLE_CLONE: 'true',
      TOOLS_TO_RUN: filters,
      DISABLE_TOOLS: filters,
    },
    {
      beta: true,
      clone: true,
      allow: filters.split(','),
      deny: filters.split(','),
    }
  ),
];
export function mcpTools(row, allTools) {
  return allTools.filter(
    name =>
      (!row.allow || row.allow.includes(name)) &&
      !row.deny?.includes(name) &&
      (row.local !== false ||
        ![
          'localFetch',
          'localSearch',
          'structureSearch',
          'astSearch',
          'lspSearch',
          'graphBuild',
          'graphQuery',
        ].includes(name)) &&
      (name !== 'clasify' || row.key) &&
      (name !== 'ghCloneRepo' || row.clone) &&
      (!['graphBuild', 'graphQuery'].includes(name) || row.beta)
  );
}

export const selectionTools = [
  'localFetch',
  'localSearch',
  'ghGetFileContent',
  'ghSearchCode',
];
export const selection = {
  predicates: ['Does this contain selected evidence?'],
};
export const queryFor = name =>
  ({
    localFetch: { path: 'evidence.txt' },
    localSearch: { path: '.', matchString: ['SELECTED'] },
    ghGetFileContent: { owner: 'octo', repo: 'demo', path: 'evidence.txt' },
    ghSearchCode: { owner: 'octo', repo: 'demo', keywords: ['selected'] },
  })[name];

export function checkAdmission(ajv, schema, row, { enabled, envelope, label }) {
  const validate = ajv.compile(schema);
  const input = query => (envelope ? { queries: [query] } : query);
  assert.equal(validate(input(row)), true, `${label}: ordinary read admitted`);
  assert.equal(
    validate(input({ ...row, classification: selection })),
    enabled,
    `${label}: classification capability`
  );
  assert.equal(
    validate(input({ ...row, selection: 'caller-state' })),
    false,
    `${label}: private state rejected`
  );
}
