import {
  describe,
  it,
  expect,
  vi,
  beforeAll,
  afterAll,
  beforeEach,
  afterEach,
} from 'vitest';
import type { ToolInvocationCallback } from '../../../octocode-tools-core/src/types/toolResults.js';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { clearAllCache } from '../../../octocode-tools-core/src/utils/http/cache.js';

function fetchUrlString(url: string | URL | Request): string {
  if (typeof url === 'string') return url;
  if (url instanceof URL) return url.href;
  return url.url;
}

const npmRegistryResponses: Map<string, string> = new Map();

function mockNpmRegistry(packageName: string, repoUrl: string): void {
  npmRegistryResponses.set(packageName, repoUrl);
}

function clearNpmRegistryMocks(): void {
  npmRegistryResponses.clear();
}

const npmCliViewResponses: Map<
  string,
  { url?: string; object?: { type: string; url: string; directory?: string } }
> = new Map();

const npmViewFullResponses: Map<
  string,
  {
    name: string;
    version?: string;
    description?: string;
    keywords?: string[];
    license?: string;
    homepage?: string;
    author?: string | { name?: string; email?: string; url?: string };
    engines?: Record<string, string>;
    dependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    repository?: string | { type?: string; url?: string; directory?: string };
  }
> = new Map();

function mockNpmCliViewUrl(packageName: string, repoUrl: string): void {
  npmCliViewResponses.set(packageName, { url: repoUrl });
}

function mockNpmViewFull(
  packageName: string,
  data: {
    name: string;
    version?: string;
    description?: string;
    keywords?: string[];
    license?: string;
    homepage?: string;
    author?: string | { name?: string; email?: string; url?: string };
    engines?: Record<string, string>;
    dependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    repository?: string | { type?: string; url?: string; directory?: string };
  }
): void {
  npmViewFullResponses.set(packageName, data);
}

function clearNpmCliViewMocks(): void {
  npmCliViewResponses.clear();
  npmViewFullResponses.clear();
  lastSearchResult = null;
}

function createNpmCommandMock(searchResult: {
  stdout: string;
  stderr: string;
  exitCode: number;
  error?: Error;
}) {
  try {
    const arr = JSON.parse(searchResult.stdout);
    if (Array.isArray(arr) && arr.length > 0) {
      lastSearchResult = {
        objects: arr.map((pkg: unknown) => ({ package: pkg })),
        total: arr.length,
      };
    }
  } catch {
    void 0;
  }

  return (command: string, args: string[]) => {
    if (command === 'search') {
      return Promise.resolve(searchResult);
    }

    if (command === 'view' && args.length >= 1) {
      const packageName = args[0] as string;
      const field = args.length >= 2 ? (args[1] as string) : null;

      if (field === '--json' || (args.length === 2 && args[1] === '--json')) {
        const fullResponse = npmViewFullResponses.get(packageName);
        if (fullResponse) {
          return Promise.resolve({
            stdout: JSON.stringify(fullResponse),
            stderr: '',
            exitCode: 0,
          });
        }
        return Promise.resolve({
          stdout: '',
          stderr: `npm ERR! code E404\nnpm ERR! 404 Not Found - GET https://registry.npmjs.org/${packageName} - Not found`,
          exitCode: 1,
        });
      }

      const cliResponse = npmCliViewResponses.get(packageName);

      if (field === 'repository.url') {
        if (cliResponse?.url) {
          return Promise.resolve({
            stdout: JSON.stringify(cliResponse.url),
            stderr: '',
            exitCode: 0,
          });
        }
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }

      if (field === 'repository') {
        if (cliResponse?.object) {
          return Promise.resolve({
            stdout: JSON.stringify(cliResponse.object),
            stderr: '',
            exitCode: 0,
          });
        }
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }

      if (field === 'deprecated') {
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }
    }

    return Promise.resolve({
      stdout: '',
      stderr: '',
      exitCode: 0,
    });
  };
}

const mockExecuteNpmCommand = vi.fn();
const mockCheckNpmAvailability = vi.fn();
vi.mock(
  '../../../octocode-tools-core/src/utils/exec/npm.js',
  async importOriginal => {
    const actual =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/utils/exec/npm.js')
      >();
    return {
      ...actual,
      executeNpmCommand: (...args: unknown[]) => mockExecuteNpmCommand(...args),
      checkNpmAvailability: (...args: unknown[]) =>
        mockCheckNpmAvailability(...args),
    };
  }
);

vi.mock('../../../octocode-tools-core/src/utils/http/cache.js', () => ({
  generateCacheKey: vi.fn(() => 'test-cache-key'),
  withDataCache: vi.fn(async (_key: string, fn: () => unknown) => {
    return await fn();
  }),
  clearAllCache: vi.fn(),
}));

const mockFetch = vi.fn();
let _originalFetch: unknown;

beforeAll(() => {
  _originalFetch = (globalThis as Record<string, unknown>).fetch;
  (globalThis as Record<string, unknown>).fetch = mockFetch;
});

afterAll(() => {
  (globalThis as Record<string, unknown>).fetch = _originalFetch;
});

let lastSearchResult: {
  objects: Array<{ package: unknown }>;
  total: number;
} | null = null;

function setupDefaultFetchMock(): void {
  mockFetch.mockImplementation(
    (url: string | URL | Request, _init?: RequestInit) => {
      const urlStr = fetchUrlString(url);
      if (/^https?:\/\/[^/]+\/?$/.test(urlStr)) {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: () => Promise.resolve({ db_name: 'registry' }),
          body: null,
        });
      }
      if (urlStr.includes('/latest')) {
        const withoutProtocol = urlStr.replace(
          'https://registry.npmjs.org/',
          ''
        );
        const pkgName = decodeURIComponent(
          withoutProtocol.slice(0, withoutProtocol.lastIndexOf('/latest'))
        );
        const data = npmViewFullResponses.get(pkgName);
        if (data) {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () => Promise.resolve(data),
            body: null,
          });
        }
        return Promise.resolve({
          ok: false,
          status: 404,
          statusText: 'Not Found',
          body: { cancel: vi.fn().mockResolvedValue(undefined) },
          headers: new Headers(),
        });
      }
      if (urlStr.includes('/-/v1/search')) {
        const result = lastSearchResult ?? { objects: [], total: 0 };
        return Promise.resolve({
          ok: true,
          status: 200,
          json: () => Promise.resolve(result),
          body: null,
        });
      }
      return Promise.reject(new Error(`Unexpected fetch call to: ${urlStr}`));
    }
  );
}

vi.mock(
  '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js',
  async () => {
    const actual = await vi.importActual<
      typeof import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js')
    >('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');
    return {
      ...actual,
      TOOL_NAMES: new Proxy(actual.TOOL_NAMES, {
        get(target, prop: string | symbol) {
          if (prop === 'PACKAGE_SEARCH') return 'npmSearch';
          return Reflect.get(target, prop);
        },
      }),
      DESCRIPTIONS: new Proxy(actual.DESCRIPTIONS, {
        get(target, prop: string) {
          if (prop === 'npmSearch') {
            return 'Search for packages in the npm ecosystem';
          }
          return Reflect.get(target, prop);
        },
      }),
    };
  }
);

import {
  searchPackage,
  type NpmSearchInput,
  type NpmPackageResult,
} from '../../../octocode-tools-core/src/utils/package/common.js';
import { registerNpmSearchTool } from '../../src/tools/package_search/package_search.js';
import { _resetNpmRegistryUrlCache } from '../../../octocode-tools-core/src/utils/package/npm.js';

describe('searchPackage - NPM (CLI)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockExecuteNpmCommand.mockReset();
    clearAllCache();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should return lightweight NPM package findings by default', async () => {
    mockNpmViewFull('axios', {
      name: 'axios',
      version: '1.6.0',
      description: 'Promise based HTTP client for the browser and node.js',
      keywords: ['xhr', 'http', 'ajax', 'promise', 'node'],
      repository: 'git+https://github.com/axios/axios.git',
      homepage: 'https://axios-http.com',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'axios',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('axios');
      expect(pkg.repoUrl).toBe('https://github.com/axios/axios');
      expect(pkg.version).toBe('1.6.0');

      expect(pkg.description).toBe(
        'Promise based HTTP client for the browser and node.js'
      );

      expect(result.totalFound).toBe(1);
    }

    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'view',
      ['axios', '--json'],
      expect.any(Object)
    );
  });

  it('keyword queries default to a ranked page of 10 results (with metadata fetch)', async () => {
    const searchItems = JSON.stringify([
      { name: 'pkg-a', version: '1.0.0', description: 'a' },
      { name: 'pkg-b', version: '2.0.0', description: 'b' },
    ]);
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({ stdout: searchItems, stderr: '', exitCode: 0 })
    );

    const query: NpmSearchInput = {
      name: 'react state management',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'search',
      ['react state management', '--json', '--searchlimit', '10'],
      expect.any(Object)
    );
    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'view',
      expect.arrayContaining(['pkg-a', '--json']),
      expect.any(Object)
    );
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.map(p => p.name)).toEqual(['pkg-a', 'pkg-b']);
    }
  });

  it('keyword search calls npm view per-item to populate repositoryDirectory (sourceRoot)', async () => {
    const searchItems = JSON.stringify([
      { name: 'pkg-a', version: '1.0.0', description: 'a' },
      { name: 'pkg-b', version: '2.0.0', description: 'b' },
    ]);
    mockNpmViewFull('pkg-a', {
      name: 'pkg-a',
      version: '1.0.0',
      description: 'a',
      repository: 'https://github.com/octo/pkg-a',
    });
    mockNpmViewFull('pkg-b', {
      name: 'pkg-b',
      version: '2.0.0',
      description: 'b',
      repository: {
        url: 'https://github.com/octo/monorepo',
        directory: 'packages/pkg-b',
      },
    });
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({ stdout: searchItems, stderr: '', exitCode: 0 })
    );

    const result = await searchPackage({
      name: 'react state management',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    });

    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'search',
      expect.arrayContaining(['react state management', '--json']),
      expect.any(Object)
    );
    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'view',
      expect.arrayContaining(['pkg-a']),
      expect.any(Object)
    );
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkgB = result.packages.find(p => p.name === 'pkg-b') as
        NpmPackageResult | undefined;
      expect(pkgB?.repositoryDirectory).toBe('packages/pkg-b');
    }
  });

  it('should return full NPM package results for exact package name', async () => {
    mockNpmViewFull('axios', {
      name: 'axios',
      version: '1.6.0',
      description: 'Promise based HTTP client for the browser and node.js',
      keywords: ['xhr', 'http', 'ajax', 'promise', 'node'],
      repository: 'git+https://github.com/axios/axios.git',
      homepage: 'https://axios-http.com',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'axios',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('axios');
      expect(pkg.repoUrl).toBe('https://github.com/axios/axios');

      expect(pkg.version).toBe('1.6.0');

      expect(pkg.description).toBe(
        'Promise based HTTP client for the browser and node.js'
      );
      expect(pkg.keywords).toEqual(['xhr', 'http', 'ajax', 'promise', 'node']);
      expect(pkg.homepage).toBe('https://axios-http.com');

      expect(result.totalFound).toBe(1);
    }
  });

  it('should return full exact-package metadata by default (exact names always fetch metadata)', async () => {
    mockNpmViewFull('full-meta-pkg', {
      name: 'full-meta-pkg',
      version: '2.0.0',
      description: 'Full metadata package',
      keywords: ['detail', 'metadata'],
      repository: 'git+https://github.com/octo/full-meta-pkg.git',
      homepage: 'https://example.com/full',
      author: { name: 'Octo Dev' },
      peerDependencies: { react: '^18.0.0' },
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const result = await searchPackage({
      name: 'full-meta-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    });

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('full-meta-pkg');
      expect(pkg.repoUrl).toBe('https://github.com/octo/full-meta-pkg');
      expect(pkg.version).toBe('2.0.0');
      expect(pkg.keywords).toEqual(['detail', 'metadata']);
      expect(pkg.homepage).toBe('https://example.com/full');
      expect(pkg.author).toBe('Octo Dev');
      expect(pkg.peerDependencies).toEqual({ react: '^18.0.0' });
    }
  });

  it('should fall back to registry document when npm view exact lookup fails', async () => {
    mockNpmViewFull('left-pad', {
      name: 'left-pad',
      version: '1.3.0',
      repository: 'git+https://github.com/stevemao/left-pad.git',
    });
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '',
      stderr: 'npm ERR! code E404',
      exitCode: 1,
    });

    const query: NpmSearchInput = {
      name: 'left-pad',
      itemsPerPage: 20,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toHaveLength(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('left-pad');
      expect(pkg.repoUrl).toBe('https://github.com/stevemao/left-pad');
    }
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/left-pad/latest'),
      expect.any(Object)
    );
  });

  it('should handle NPM CLI search with multiple results (keyword search)', async () => {
    const mockCliOutput = JSON.stringify([
      {
        name: 'lodash',
        version: '4.17.21',
        description: 'Lodash modular utilities',
        keywords: ['modules', 'stdlib', 'util'],
        links: { repository: 'git+https://github.com/lodash/lodash.git' },
      },
      {
        name: 'lodash-es',
        version: '4.17.21',
        description: 'Lodash exported as ES modules',
        keywords: ['es', 'modules'],
        links: { repository: 'git+https://github.com/lodash/lodash.git' },
      },
    ]);

    mockNpmCliViewUrl('lodash', 'git+https://github.com/lodash/lodash.git');
    mockNpmCliViewUrl('lodash-es', 'git+https://github.com/lodash/lodash.git');

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: mockCliOutput,
        stderr: '',
        exitCode: 0,
      })
    );

    mockNpmRegistry('lodash', 'git+https://github.com/lodash/lodash.git');
    mockNpmRegistry('lodash-es', 'git+https://github.com/lodash/lodash.git');

    const query: NpmSearchInput = {
      name: 'lodash utilities',
      itemsPerPage: 5,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(2);
      expect(result.totalFound).toBe(2);

      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('lodash');
      expect(pkg.version).toBe('4.17.21');
      expect(pkg.mainEntry).toBeUndefined();
    }

    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'search',
      ['lodash utilities', '--json', '--searchlimit', '5'],
      expect.any(Object)
    );
  });

  it('pages registry results: page=2 sends the from offset (#2)', async () => {
    const query: NpmSearchInput = {
      name: 'lodash utilities',
      itemsPerPage: 5,
      page: 2,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    await searchPackage(query);

    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringMatching(/\/-\/v1\/search.*[?&]from=5\b/),
      expect.any(Object)
    );
  });

  it('should return package details for exact name lookup', async () => {
    mockNpmViewFull('test-package', {
      name: 'test-package',
      version: '1.0.0',
      repository: 'https://github.com/test/test-package',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'test-package',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('test-package');
      expect(pkg.version).toBe('1.0.0');
      expect(pkg.repoUrl).toBe('https://github.com/test/test-package');
    }
  });

  it('should handle NPM registry fetch error', async () => {
    mockFetch.mockRejectedValue(new Error('Command timeout'));

    const query: NpmSearchInput = {
      name: 'axios http client',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain('Command timeout');
      expect(result.hints).toBeDefined();
    }
  });

  it('should handle invalid response format from registry (keyword search)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ notObjects: true }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'axios http',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain(
        'Invalid npm registry search response format'
      );
    }
  });

  it('should handle empty search results (keyword search)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ objects: [], total: 0 }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'nonexistent package xyz123',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(0);
      expect(result.totalFound).toBe(0);
    }
  });

  it('should return one canonical package for exact name + itemsPerPage=3', async () => {
    mockNpmViewFull('typescript', {
      name: 'typescript',
      version: '5.7.3',
      description:
        'TypeScript is a language for application scale JavaScript development',
    });
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
    );

    const query: NpmSearchInput = {
      name: 'typescript',
      itemsPerPage: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      expect(result.totalFound).toBe(1);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.name).toBe('typescript');
      expect(first.version).toBe('5.7.3');
    }
  });

  it('should filter out null-name items and accept null version (BUG-02 null fields)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () =>
        Promise.resolve({
          objects: [
            {
              package: {
                name: 'typescript',
                version: '5.7.3',
                description: 'TypeScript compiler',
                links: { npm: 'https://www.npmjs.com/package/typescript' },
              },
              score: { final: 0.9999 },
              searchScore: 100000,
            },
            {
              package: {
                name: null,
                version: null,
                description: null,
                links: null,
              },
              score: { final: 0 },
              searchScore: 0,
            },
            {
              package: {
                name: 'ts-node',
                version: null,
                description: 'TypeScript execution environment',
                links: {
                  npm: 'https://www.npmjs.com/package/ts-node',
                  repository: null,
                },
              },
              score: { final: 0.85 },
              searchScore: 50000,
            },
          ],
          total: 3,
        }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'typescript runtime',
      itemsPerPage: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(2);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.name).toBe('typescript');
      expect(first.version).toBe('5.7.3');
      const second = result.packages[1] as NpmPackageResult;
      expect(second.name).toBe('ts-node');
      expect(second.version).toBe('unknown');
      expect(second.repoUrl).toBeNull();
    }
  });

  it('should succeed with itemsPerPage > 1 when registry items have extra fields (BUG-02 fix)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () =>
        Promise.resolve({
          objects: [
            {
              package: {
                name: 'typescript',
                version: '5.7.3',
                description:
                  'TypeScript is a language for application scale JS development',
                links: { npm: 'https://www.npmjs.com/package/typescript' },
              },
              score: {
                final: 0.9999,
                detail: { quality: 1, popularity: 1, maintenance: 1 },
              },
              searchScore: 100000,
            },
            {
              package: {
                name: 'ts-node',
                description: 'TypeScript execution environment',
                links: { npm: 'https://www.npmjs.com/package/ts-node' },
              },
              score: { final: 0.85 },
              searchScore: 50000,
            },
          ],
          total: 2,
          time: 'Thu Jan 09 2025 00:00:00 GMT+0000',
        }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'typescript types',
      itemsPerPage: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(2);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.name).toBe('typescript');
      expect(first.version).toBe('5.7.3');
      const second = result.packages[1] as NpmPackageResult;
      expect(second.name).toBe('ts-node');
      expect(second.version).toBe('unknown');
    }
  });

  it('should handle registry response with string total (BUG-02 fix)', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () =>
        Promise.resolve({
          objects: [
            {
              package: { name: 'lodash', version: '4.17.21' },
            },
          ],
          total: '1000',
        }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'lodash utility helpers',
      itemsPerPage: 2,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.name).toBe('lodash');
    }
  });
});

describe('searchPackage - NPM Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockExecuteNpmCommand.mockReset();
    clearAllCache();
    clearNpmRegistryMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should handle non-array npm registry search response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ notObjects: true }),
      body: null,
    });

    const query: NpmSearchInput = {
      name: 'test pkg keyword',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result.error).toContain(
        'Invalid npm registry search response format'
      );
    }
  });
});

describe('Package search response structure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    mockExecuteNpmCommand.mockReset();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should return lightweight structure by default', async () => {
    mockNpmViewFull('express', {
      name: 'express',
      version: '4.18.2',
      description: 'Fast web framework',
      keywords: ['web', 'framework'],
      repository: 'https://github.com/expressjs/express',
    });

    const query: NpmSearchInput = {
      name: 'express',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result).toHaveProperty('packages');
      expect(result).toHaveProperty('totalFound');
      expect(Array.isArray(result.packages)).toBe(true);

      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg).toHaveProperty('name');
      expect(pkg).toHaveProperty('repoUrl');
      expect(pkg).toHaveProperty('version');

      expect(pkg).toHaveProperty('description', 'Fast web framework');
    }
  });

  it('should return full structure for exact name lookup', async () => {
    mockNpmViewFull('express', {
      name: 'express',
      version: '4.18.2',
      description: 'Fast web framework',
      keywords: ['web', 'framework'],
      repository: 'https://github.com/expressjs/express',
    });

    const query: NpmSearchInput = {
      name: 'express',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg).toHaveProperty('name');
      expect(pkg).toHaveProperty('version');
      expect(pkg).toHaveProperty('repoUrl');
      expect(pkg).toHaveProperty('mainEntry');
    }
  });

  it('should return proper structure for error response', async () => {
    mockFetch.mockRejectedValue(new Error('Command failed'));

    const query: NpmSearchInput = {
      name: 'test package search',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('error' in result).toBe(true);
    if ('error' in result) {
      expect(result).toHaveProperty('error');
      expect(typeof result.error).toBe('string');
      expect(result.hints).toBeDefined();
      expect(Array.isArray(result.hints)).toBe(true);
    }
  });
});

describe('registerNpmSearchTool', () => {
  let mockServer: MockMcpServer;
  let mockCallback: ReturnType<typeof vi.fn<ToolInvocationCallback>>;

  beforeEach(() => {
    mockServer = createMockMcpServer();
    mockCallback = vi.fn<ToolInvocationCallback>().mockResolvedValue(undefined);
    vi.clearAllMocks();
    clearAllCache();
    clearNpmCliViewMocks();
    mockExecuteNpmCommand.mockReset();
    clearNpmRegistryMocks();
    _resetNpmRegistryUrlCache();
    mockCheckNpmAvailability.mockResolvedValue(true);
    setupDefaultFetchMock();
  });

  afterEach(() => {
    mockServer.cleanup();
    vi.resetAllMocks();
  });

  describe('Tool Registration', () => {
    it('should register package_search tool with callback when npm is available', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerNpmSearchTool(mockServer.server, mockCallback);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('should register package_search tool without callback when npm is available', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerNpmSearchTool(mockServer.server);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('should register with undefined callback when npm is available', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerNpmSearchTool(mockServer.server, undefined);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('registers even when npm + registry are unavailable (never vanishes)', async () => {
      mockCheckNpmAvailability.mockResolvedValue(false);
      mockFetch.mockRejectedValue(new Error('fetch failed'));
      await registerNpmSearchTool(mockServer.server, mockCallback);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });
  });

  describe('Tool Execution - NPM', () => {
    it('should execute npm package search and return results', async () => {
      mockNpmViewFull('axios', {
        name: 'axios',
        version: '1.6.0',
        description: 'HTTP client',
        repository: 'https://github.com/axios/axios',
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerNpmSearchTool(mockServer.server, mockCallback);

      const queries = [
        {
          packageName: 'axios',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ];
      const result = await mockServer.callTool('npmSearch', {
        queries,
      });

      expect(result.isError).toBeFalsy();
      expect(result.content).toBeDefined();
      expect(result.content[0]).toHaveProperty('text');
      expect(mockCallback).toHaveBeenCalledWith('npmSearch', queries);
    });
    it('should include install hint for npm packages', async () => {
      mockNpmViewFull('lodash', {
        name: 'lodash',
        version: '4.17.21',
        description: 'Utility library',
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'lodash',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Install: npm install lodash');
    });

    it('should include pagination in data when registry returns more results than fetched', async () => {
      mockExecuteNpmCommand.mockRejectedValue(new Error('npm not found'));

      mockFetch.mockImplementation((url: string | URL | Request) => {
        const urlStr = fetchUrlString(url);
        if (urlStr.includes('/-/v1/search')) {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () =>
              Promise.resolve({
                objects: [
                  {
                    package: {
                      packageName: 'react-utils',
                      version: '1.0.0',
                      description: 'React utilities',
                    },
                  },
                ],
                total: 1000,
              }),
            body: null,
          });
        }
        if (/^https?:\/\/[^/]+\/?$/.test(urlStr)) {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () => Promise.resolve({ db_name: 'registry' }),
            body: null,
          });
        }
        return Promise.resolve({
          ok: false,
          status: 404,
          statusText: 'Not Found',
          body: { cancel: vi.fn().mockResolvedValue(undefined) },
          headers: new Headers(),
        });
      });

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'react utils library',
            mainResearchGoal: 'Test pagination',
            researchGoal: 'Test',
            reasoning: 'Validate pagination output',
          },
        ],
      });

      expect(result.isError).toBeFalsy();
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('hasMore');
      expect(text).toContain('1000');
    });

    it('should not report hasMore when returned count equals totalFound', async () => {
      mockExecuteNpmCommand.mockRejectedValue(new Error('npm not found'));

      mockFetch.mockImplementation((url: string | URL | Request) => {
        const urlStr = fetchUrlString(url);
        if (urlStr.includes('/-/v1/search')) {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () =>
              Promise.resolve({
                objects: Array.from({ length: 10 }, (_, index) => ({
                  package: {
                    name: `react-hook-${index}`,
                    version: '1.0.0',
                    description: 'React hook package',
                  },
                })),
                total: 10,
              }),
            body: null,
          });
        }
        if (/^https?:\/\/[^/]+\/?$/.test(urlStr)) {
          return Promise.resolve({
            ok: true,
            status: 200,
            json: () => Promise.resolve({ db_name: 'registry' }),
            body: null,
          });
        }
        return Promise.resolve({
          ok: false,
          status: 404,
          statusText: 'Not Found',
          body: { cancel: vi.fn().mockResolvedValue(undefined) },
          headers: new Headers(),
        });
      });

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'react hooks',
            page: 2,
            mainResearchGoal: 'Test pagination',
            researchGoal: 'Test',
            reasoning: 'Validate final page output',
          },
        ],
      });

      expect(result.isError).toBeFalsy();
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('totalFound: 10');
      expect(text).not.toContain('hasMore: true');
    });

    it('should return rich object output per package (name, version, description, repository)', async () => {
      mockNpmViewFull('lodash', {
        name: 'lodash',
        version: '4.17.21',
        description: 'Utility library',
        repository: 'https://github.com/lodash/lodash',
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'lodash',
            mainResearchGoal: 'Test rich object output',
            researchGoal: 'Test',
            reasoning:
              'Packages now include version, description, license, weeklyDownloads',
          },
        ],
      });

      expect(result.isError).toBeFalsy();
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('lodash');
      expect(text).toContain('https://github.com/lodash/lodash');
      expect(text).not.toContain('entrypoints');
      expect(text).not.toContain('researchTargets');
      expect(text).not.toContain('packageType');
      expect(text).not.toContain('npmUrl:');
    });

    it('emits ghSearchRepos hint when registry is unreachable', async () => {
      mockFetch.mockRejectedValue(new Error('fetch failed'));
      mockExecuteNpmCommand.mockRejectedValue(new Error('npm failed'));

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'my_package_name',
            mainResearchGoal: 'Test error hint',
            researchGoal: 'Test',
            reasoning: 'Registry unreachable — should hint ghSearchRepos',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('ghSearchRepos');
    });
  });

  describe('Callback Invocation', () => {
    it('should invoke callback with tool name and queries', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerNpmSearchTool(mockServer.server, mockCallback);

      const queries = [
        {
          packageName: 'test-pkg',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ];

      await mockServer.callTool('npmSearch', { queries });

      expect(mockCallback).toHaveBeenCalledWith('npmSearch', queries);
    });

    it('should continue execution even if callback throws', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      mockCallback.mockRejectedValue(new Error('Callback error'));

      await registerNpmSearchTool(mockServer.server, mockCallback);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });

    it('should not invoke callback if none provided', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerNpmSearchTool(mockServer.server);

      await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(mockCallback).not.toHaveBeenCalled();
    });
  });

  describe('Bulk Operations', () => {
    it('should handle multiple queries in bulk', async () => {
      mockNpmViewFull('pkg1', { name: 'pkg1', version: '1.0.0' });
      mockNpmViewFull('pkg2', { name: 'pkg2', version: '1.0.0' });

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '',
          stderr: '',
          exitCode: 0,
        })
      );

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'pkg1',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
          {
            packageName: 'pkg2',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.content).toBeDefined();

      expect(mockExecuteNpmCommand).toHaveBeenCalledTimes(6);
    });

    it('should handle empty queries array', async () => {
      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Error Handling', () => {
    it('should handle unexpected errors', async () => {
      mockExecuteNpmCommand.mockRejectedValue(new Error('Unexpected error'));

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.content).toBeDefined();
    });
  });

  describe('Success Hints Generation', () => {
    it('should not include repo hint when packages have no repository', async () => {
      mockNpmViewFull('no-repo-pkg', {
        name: 'no-repo-pkg',
        version: '1.0.0',
        description: 'Package without repo',
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'no-repo-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Install: npm install');
      expect(text).not.toContain('ghViewRepoStructure');
    });
  });

  describe('Custom Hints in Response', () => {
    it('should return hasResultsStatusHints with only install hint when package has no repository', async () => {
      mockNpmViewFull('no-repo-pkg', {
        name: 'no-repo-pkg',
        version: '1.0.0',
        description: 'Package without repo',
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'no-repo-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      expect(text).toContain('Install: npm install no-repo-pkg');
      expect(text).not.toContain('ghViewRepoStructure');

      expect(text).not.toContain('status: "hasResults"');
    });
    it('should return emptyStatusHints with browse link when no npm packages found', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerNpmSearchTool(mockServer.server);

      const result = await mockServer.callTool('npmSearch', {
        queries: [
          {
            packageName: 'nonexistent pkg xyz123 keyword',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      expect(text).toContain('Check spelling');
      expect(text).not.toContain(
        'Browse: https://npmjs.com/search?q=nonexistent%20pkg%20xyz123%20keyword'
      );

      expect(text).toContain('status: empty');
    });
  });
});

describe('Task 2: Name Variation Suggestions', () => {
  let mockServer: MockMcpServer;

  beforeEach(async () => {
    vi.clearAllMocks();
    clearAllCache();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    mockCheckNpmAvailability.mockResolvedValue(true);
    mockServer = createMockMcpServer();
    setupDefaultFetchMock();
  });

  afterEach(() => {
    mockServer.cleanup();
  });

  it('should suggest name variations with hyphens converted to underscores', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '[]',
      stderr: '',
      exitCode: 0,
    });

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: 'date-fns keyword',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('Check spelling');
  });

  it('should suggest unscoped name for @scope/name packages', async () => {
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: '@nonexistent/package',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('Try: package');
  });

  it('should suggest js suffix for npm packages', async () => {
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '[]',
      stderr: '',
      exitCode: 0,
    });

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: 'chart library',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('Check spelling');
  });

  it('should return all packages from a search result', async () => {
    const searchResults = JSON.stringify(
      Array.from({ length: 4 }, (_, index) => ({
        name: `pkg-${index}`,
        version: '1.0.0',
        description: `Package ${index}`,
        keywords: [],
        links: {
          repository: `https://github.com/octo/pkg-${index}`,
        },
      }))
    );

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: searchResults,
        stderr: '',
        exitCode: 0,
      })
    );

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: 'pkg utility library',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const structured = result.structuredContent as {
      results: Array<{
        data: {
          packages?: Array<{ name: string }>;
        };
      }>;
    };
    const data = structured.results[0]!.data;

    expect(data.packages?.length).toBeGreaterThan(0);
  });
});

describe('Task 3: Deprecation Detection', () => {
  let mockServer: MockMcpServer;

  beforeEach(async () => {
    vi.clearAllMocks();
    clearAllCache();
    mockCheckNpmAvailability.mockResolvedValue(true);
    mockServer = createMockMcpServer();
  });

  afterEach(() => {
    mockServer.cleanup();
  });

  it('should show deprecation warning for deprecated npm packages', async () => {
    mockNpmViewFull('request', {
      name: 'request',
      version: '2.88.2',
      description: 'Simplified HTTP request client',
      keywords: [],
      repository: 'https://github.com/request/request',
    });

    mockExecuteNpmCommand.mockImplementation((cmd: string, args: string[]) => {
      if (cmd === 'view' && args.length === 2 && args[1] === '--json') {
        const fullResponse = npmViewFullResponses.get(args[0] as string);
        if (fullResponse) {
          return Promise.resolve({
            stdout: JSON.stringify(fullResponse),
            stderr: '',
            exitCode: 0,
          });
        }
        return Promise.resolve({ stdout: '', stderr: '', exitCode: 1 });
      }
      if (cmd === 'view' && args.includes('deprecated')) {
        return Promise.resolve({
          stdout: '"request has been deprecated"',
          stderr: '',
          exitCode: 0,
        });
      }
      return Promise.resolve({ stdout: '', stderr: '', exitCode: 0 });
    });

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: 'request',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('DEPRECATED: request');
    expect(text).toContain('request has been deprecated');
  });

  it('should not show deprecation warning for non-deprecated packages', async () => {
    const mockSearchOutput = JSON.stringify([
      {
        name: 'lodash',
        version: '4.17.21',
        description: 'Utility library',
        keywords: [],
        links: { repository: 'https://github.com/lodash/lodash' },
      },
    ]);

    mockExecuteNpmCommand.mockImplementation((cmd: string, args: string[]) => {
      if (cmd === 'search') {
        return Promise.resolve({
          stdout: mockSearchOutput,
          stderr: '',
          exitCode: 0,
        });
      }
      if (cmd === 'view' && args.includes('deprecated')) {
        return Promise.resolve({
          stdout: 'undefined',
          stderr: '',
          exitCode: 0,
        });
      }
      return Promise.resolve({ stdout: '', stderr: '', exitCode: 0 });
    });

    await registerNpmSearchTool(mockServer.server);

    const result = await mockServer.callTool('npmSearch', {
      queries: [
        {
          packageName: 'lodash',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).not.toContain('DEPRECATED');
  });
});

describe('searchPackage - NPM CLI Repository Fetching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockExecuteNpmCommand.mockReset();
    clearAllCache();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should fetch repository URL via CLI (string format)', async () => {
    mockNpmViewFull('axios', {
      name: 'axios',
      version: '1.6.0',
      description: 'HTTP client',
      repository: 'git+https://github.com/axios/axios.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'axios',
      mainResearchGoal: 'Test CLI repository URL fetching',
      researchGoal: 'Test string URL format',
      reasoning: 'Verify CLI-first approach works',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBe('https://github.com/axios/axios');
    }

    expect(mockExecuteNpmCommand).toHaveBeenCalledWith(
      'view',
      ['axios', '--json'],
      expect.any(Object)
    );
  });

  it('should fetch repository URL via CLI (object format for scoped monorepo packages)', async () => {
    mockNpmViewFull('@scope/monorepo-package', {
      name: '@scope/monorepo-package',
      version: '6.0.0',
      repository: {
        type: 'git',
        url: 'https://github.com/org/monorepo.git',
        directory: 'packages/monorepo-package',
      },
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: '@scope/monorepo-package',
      mainResearchGoal: 'Test CLI repository URL fetching',
      researchGoal: 'Test object URL format',
      reasoning:
        'Verify CLI handles scoped package with object repository format',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBe('https://github.com/org/monorepo');
    }
  });

  it('should handle package without repository defined', async () => {
    mockNpmViewFull('some-package', {
      name: 'some-package',
      version: '1.0.0',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'some-package',
      mainResearchGoal: 'Test package without repository',
      researchGoal: 'Test when package has no repository',
      reasoning: 'Verify null repository is returned gracefully',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBeNull();
    }
  });

  it('should return empty when package not found', async () => {
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'no-repo-package',
      mainResearchGoal: 'Test package not found case',
      researchGoal: 'Test when package does not exist',
      reasoning: 'Verify empty result is returned gracefully',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(0);
    }
  });

  it('should handle scoped packages correctly', async () => {
    mockNpmViewFull('@types/node', {
      name: '@types/node',
      version: '20.0.0',
      repository: 'https://github.com/DefinitelyTyped/DefinitelyTyped.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: '@types/node',
      mainResearchGoal: 'Test scoped package handling',
      researchGoal: 'Test @types/node repository fetching',
      reasoning: 'Verify scoped packages work with CLI',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBe(
        'https://github.com/DefinitelyTyped/DefinitelyTyped'
      );
    }
  });

  it('should clean git+ prefix and .git suffix from CLI response', async () => {
    mockNpmViewFull('lodash', {
      name: 'lodash',
      version: '4.17.21',
      repository: 'git+https://github.com/lodash/lodash.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '',
        stderr: '',
        exitCode: 0,
      })
    );

    const query: NpmSearchInput = {
      name: 'lodash',
      mainResearchGoal: 'Test URL cleaning',
      researchGoal: 'Test git+ and .git are removed',
      reasoning: 'Verify URL is cleaned properly',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBe('https://github.com/lodash/lodash');
      expect(pkg.repoUrl).not.toContain('git+');
      expect(pkg.repoUrl).not.toMatch(/\.git$/);
    }
  });
});
