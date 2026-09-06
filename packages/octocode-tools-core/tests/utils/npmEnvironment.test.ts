import { expect, it, vi } from 'vitest';
const spawn = vi.hoisted(() => vi.fn(async () => ({ stdout: '', stderr: '', exitCode: 0 })));
vi.mock('../../src/utils/exec/spawn/wrappers.js', () => ({
  spawnWithTimeout: spawn,
  spawnCheckSuccess: vi.fn(),
  validateArgs: () => ({ valid: true }),
}));
import { executeNpmCommand } from '../../src/utils/exec/npm.js';

it('passes registry settings and npm auth interpolation variables only to npm', async () => {
  await executeNpmCommand('config', ['get', 'registry']);
  const options = (spawn.mock.calls[0] as unknown as [string, string[], { allowEnvVars: string[] }])[2];
  expect(options.allowEnvVars).toEqual(expect.arrayContaining([
    'NPM_CONFIG_REGISTRY', 'npm_config_registry', 'NPM_CONFIG_USERCONFIG', 'npm_config_userconfig', 'NPM_TOKEN', 'NODE_AUTH_TOKEN',
  ]));
  expect(options.allowEnvVars).not.toContain('NODE_OPTIONS');
  expect(options.allowEnvVars).not.toContain('GITHUB_TOKEN');
});
