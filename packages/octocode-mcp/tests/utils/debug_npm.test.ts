import { it, vi, beforeEach } from 'vitest';
import { clearAllCache } from '@octocodeai/octocode-tools-core';
import { resetCircuitBreaker } from '@octocodeai/octocode-tools-core';

const mockFetchWithRetries = vi.fn();
vi.mock('@octocodeai/octocode-tools-core', () => ({
  fetchWithRetries: (...args: unknown[]) => {
    console.log(
      'fetchWithRetries called with URL:',
      (args[0] as string)?.substring(0, 60)
    );
    return mockFetchWithRetries(...args);
  },
}));

const mockExecuteNpmCommand = vi.fn();
vi.mock('@octocodeai/octocode-tools-core', () => ({
  executeNpmCommand: (...args: unknown[]) => mockExecuteNpmCommand(...args),
}));

import {
  searchNpmPackage,
  _resetNpmRegistryUrlCache,
} from '@octocodeai/octocode-tools-core';

beforeEach(() => {
  vi.resetAllMocks();
  clearAllCache();
  resetCircuitBreaker();
  _resetNpmRegistryUrlCache();
  mockExecuteNpmCommand.mockResolvedValue({
    exitCode: 1,
    stdout: '',
    stderr: 'npm ERR! code E404',
    error: null,
  });
});

it('debug source=web test', async () => {
  mockFetchWithRetries
    .mockRejectedValueOnce(new Error('fetch failed'))
    .mockRejectedValueOnce(new Error('fetch failed'))
    .mockRejectedValueOnce(new Error('fetch failed'))
    .mockResolvedValueOnce({
      results: [
        {
          package: {
            name: 'react-query',
            version: '5.0.0',
            links: {
              npm: 'https://npmjs.com/package/react-query',
              repository: 'https://github.com/tanstack/query',
            },
          },
        },
      ],
      total: 1,
    });

  const result = await searchNpmPackage('react-query', 5, false);
  console.log('RESULT:', JSON.stringify(result, null, 2));
  console.log(
    'mockFetchWithRetries call count:',
    mockFetchWithRetries.mock.calls.length
  );
});
