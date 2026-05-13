/** @type {import("syncpack").RcFile} */
const config = {
  source: ['package.json', 'packages/*/package.json', 'skills/*/package.json'],
  sortFirst: [
    'name',
    'version',
    'description',
    'keywords',
    'author',
    'homepage',
    'repository',
    'bugs',
    'license',
    'type',
    'main',
    'types',
    'exports',
    'typesVersions',
    'bin',
    'files',
    'engines',
    'activationEvents',
    'categories',
    'contributes',
    'publisher',
    'displayName',
    'icon',
    'scripts',
    'dependencies',
    'devDependencies',
    'peerDependencies',
  ],
  semverGroups: [
    {
      range: '^',
      dependencyTypes: ['dev', 'prod', 'peer'],
      dependencies: ['**'],
      packages: ['**'],
    },
  ],
  versionGroups: [
    {
      label: 'Use workspace protocol for internal packages',
      dependencies: ['octocode-shared', 'octocode-security-utils'],
      pinVersion: 'workspace:^',
    },
    {
      label: 'Align TypeScript across all packages',
      dependencies: ['typescript'],
      policy: 'sameRange',
    },
    {
      label: 'Align ESLint across all packages',
      dependencies: ['eslint'],
      policy: 'sameRange',
    },
    {
      label: 'Align @types/node across all packages',
      dependencies: ['@types/node'],
      policy: 'sameRange',
    },
    {
      label: 'Align Vitest across all packages',
      dependencies: ['vitest', '@vitest/*'],
      policy: 'sameRange',
    },
    {
      label: 'Align esbuild across all packages',
      dependencies: ['esbuild'],
      policy: 'sameRange',
    },
  ],
};

export default config;
