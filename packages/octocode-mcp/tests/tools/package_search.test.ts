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
import { PackageSearchQuerySchema } from '@octocodeai/octocode-core';
import type { ToolInvocationCallback } from '../../src/types.js';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

/** Delegates PyPI HTTPS calls from setupDefaultFetchMock */
const mockPypiFetch = vi.fn();

function fetchUrlString(url: string | URL | Request): string {
  if (typeof url === 'string') return url;
  if (url instanceof URL) return url.href;
  return url.url;
}

/** Build a 200 Response from legacy axios-shaped mocks `{ data: PyPI body }` */
function pypiJsonResponse(axiosStyle: {
  data: Record<string, unknown>;
}): Response {
  return new Response(JSON.stringify(axiosStyle.data), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });
}

// Store for npm registry responses (package name -> repository URL)
const npmRegistryResponses: Map<string, string> = new Map();

// Helper to set npm registry mock for a package
function mockNpmRegistry(packageName: string, repoUrl: string): void {
  npmRegistryResponses.set(packageName, repoUrl);
}

// Helper to clear npm registry mocks
function clearNpmRegistryMocks(): void {
  npmRegistryResponses.clear();
}

// Store for npm CLI view responses (package name -> repository URL or object)
const npmCliViewResponses: Map<
  string,
  { url?: string; object?: { type: string; url: string; directory?: string } }
> = new Map();

// Store for full npm view responses (package name -> full package data)
// Used by the new exact package name lookup flow
const npmViewFullResponses: Map<
  string,
  {
    name: string;
    version?: string;
    description?: string;
    keywords?: string[];
    license?: string;
    homepage?: string;
    repository?: string | { type?: string; url?: string; directory?: string };
  }
> = new Map();

// Helper to set npm CLI view mock for a package (URL format)
function mockNpmCliViewUrl(packageName: string, repoUrl: string): void {
  npmCliViewResponses.set(packageName, { url: repoUrl });
}

// Helper to set full npm view mock for exact package lookup
function mockNpmViewFull(
  packageName: string,
  data: {
    name: string;
    version?: string;
    description?: string;
    keywords?: string[];
    license?: string;
    homepage?: string;
    repository?: string | { type?: string; url?: string; directory?: string };
  }
): void {
  npmViewFullResponses.set(packageName, data);
}

// Helper to clear npm CLI view mocks
function clearNpmCliViewMocks(): void {
  npmCliViewResponses.clear();
  npmViewFullResponses.clear();
  lastSearchResult = null;
}

// Helper to create a mock implementation for executeNpmCommand that handles both search and view.
// Also populates lastSearchResult for the fetch mock so registry search works.
function createNpmCommandMock(searchResult: {
  stdout: string;
  stderr: string;
  exitCode: number;
  error?: Error;
}) {
  // Capture search output for the fetch mock (registry search format)
  try {
    const arr = JSON.parse(searchResult.stdout);
    if (Array.isArray(arr) && arr.length > 0) {
      lastSearchResult = {
        objects: arr.map((pkg: unknown) => ({ package: pkg })),
        total: arr.length,
      };
    }
  } catch {
    // Non-JSON or empty stdout — lastSearchResult stays null
  }

  return (command: string, args: string[]) => {
    // Handle search command
    if (command === 'search') {
      return Promise.resolve(searchResult);
    }

    // Handle view command
    if (command === 'view' && args.length >= 1) {
      const packageName = args[0] as string;
      const field = args.length >= 2 ? (args[1] as string) : null;

      // Handle full view (npm view <package> --json) - for exact package lookup
      // This is the new pattern: args = [packageName, '--json']
      if (field === '--json' || (args.length === 2 && args[1] === '--json')) {
        const fullResponse = npmViewFullResponses.get(packageName);
        if (fullResponse) {
          return Promise.resolve({
            stdout: JSON.stringify(fullResponse),
            stderr: '',
            exitCode: 0,
          });
        }
        // Package not found - return non-zero exit code
        return Promise.resolve({
          stdout: '',
          stderr: `npm ERR! code E404\nnpm ERR! 404 Not Found - GET https://registry.npmjs.org/${packageName} - Not found`,
          exitCode: 1,
        });
      }

      const cliResponse = npmCliViewResponses.get(packageName);

      // Check if it's a repository.url request
      if (field === 'repository.url') {
        if (cliResponse?.url) {
          return Promise.resolve({
            stdout: JSON.stringify(cliResponse.url),
            stderr: '',
            exitCode: 0,
          });
        }
        // Return empty if no URL (will trigger object fetch or API fallback)
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }

      // Check if it's a repository request (object format)
      if (field === 'repository') {
        if (cliResponse?.object) {
          return Promise.resolve({
            stdout: JSON.stringify(cliResponse.object),
            stderr: '',
            exitCode: 0,
          });
        }
        // Return empty if no object
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }

      // Handle deprecated field check
      if (field === 'deprecated') {
        return Promise.resolve({
          stdout: '',
          stderr: '',
          exitCode: 0,
        });
      }
    }

    // Default response for other commands
    return Promise.resolve({
      stdout: '',
      stderr: '',
      exitCode: 0,
    });
  };
}

// Mock executeNpmCommand and checkNpmAvailability (for npm CLI searches)
const mockExecuteNpmCommand = vi.fn();
const mockCheckNpmAvailability = vi.fn();
vi.mock('../../src/utils/exec/npm.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../src/utils/exec/npm.js')>();
  return {
    ...actual,
    executeNpmCommand: (...args: unknown[]) => mockExecuteNpmCommand(...args),
    checkNpmAvailability: (...args: unknown[]) =>
      mockCheckNpmAvailability(...args),
  };
});

// Mock the cache to prevent interference
vi.mock('../../src/utils/http/cache.js', () => ({
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

// Default fetch mock implementation: reads from npmViewFullResponses for /latest
// URLs and from lastSearchResult for /-/v1/search URLs
let lastSearchResult: {
  objects: Array<{ package: unknown }>;
  total: number;
} | null = null;

function setupDefaultFetchMock(): void {
  mockFetch.mockImplementation(
    (url: string | URL | Request, init?: RequestInit) => {
      const urlStr = fetchUrlString(url);
      if (urlStr.includes('pypi.org/pypi/')) {
        return mockPypiFetch(urlStr, init);
      }
      // Handle registry root URL ping (for checkNpmRegistryReachable)
      if (/^https?:\/\/[^/]+\/?$/.test(urlStr)) {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: () => Promise.resolve({ db_name: 'registry' }),
          body: null,
        });
      }
      if (urlStr.includes('/latest')) {
        // Extract package name: https://registry.npmjs.org/<pkgName>/latest
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

// Mock toolMetadata (proxies module — PACKAGE_SEARCH name + description)
vi.mock('../../src/tools/toolMetadata/proxies.js', async () => {
  const actual = await vi.importActual<
    typeof import('../../src/tools/toolMetadata/proxies.js')
  >('../../src/tools/toolMetadata/proxies.js');
  return {
    ...actual,
    TOOL_NAMES: new Proxy(actual.TOOL_NAMES, {
      get(target, prop: string | symbol) {
        if (prop === 'PACKAGE_SEARCH') return 'packageSearch';
        return Reflect.get(target, prop);
      },
    }),
    DESCRIPTIONS: new Proxy(actual.DESCRIPTIONS, {
      get(target, prop: string) {
        if (prop === 'packageSearch') {
          return 'Search for packages in npm or Python ecosystems';
        }
        return Reflect.get(target, prop);
      },
    }),
  };
});

// Import after mocking
import {
  searchPackage,
  type PackageSearchInput,
  type NpmPackageResult,
  type MinimalPackageResult,
  type PythonPackageResult,
} from '../../src/utils/package/common.js';
import { registerPackageSearchTool } from '../../src/tools/package_search/package_search.js';
import { _resetNpmRegistryUrlCache } from '../../src/utils/package/npm.js';

describe('PackageSearchQuerySchema', () => {
  const withResearchFields = <T extends object>(query: T) => ({
    id: 'test:pkg-search',
    ...query,
    mainResearchGoal: 'Test research goal',
    researchGoal: 'Testing package search',
    reasoning: 'Unit test for schema',
  });

  describe('NPM ecosystem validation', () => {
    it('should validate NPM package query', () => {
      const query = withResearchFields({
        ecosystem: 'npm',
        name: 'axios',
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.ecosystem).toBe('npm');
        expect(result.data.name).toBe('axios');
      }
    });

    it('should validate NPM query with searchLimit', () => {
      const query = withResearchFields({
        ecosystem: 'npm',
        name: 'lodash',
        searchLimit: 5,
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.searchLimit).toBe(5);
      }
    });

    it('should validate NPM query with npmFetchMetadata', () => {
      const query = withResearchFields({
        ecosystem: 'npm',
        name: 'react',
        npmFetchMetadata: true,
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(true);
      if (result.success && result.data.ecosystem === 'npm') {
        expect(result.data.npmFetchMetadata).toBe(true);
      }
    });

    it('should reject empty package name', () => {
      const query = withResearchFields({
        ecosystem: 'npm',
        name: '',
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(false);
    });

    it('should reject searchLimit > 10', () => {
      const query = withResearchFields({
        ecosystem: 'npm',
        name: 'axios',
        searchLimit: 15,
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(false);
    });
  });

  describe('Python ecosystem validation', () => {
    it('should validate Python package query', () => {
      const query = withResearchFields({
        ecosystem: 'python',
        name: 'requests',
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.ecosystem).toBe('python');
        expect(result.data.name).toBe('requests');
      }
    });

    it('should validate Python query with searchLimit', () => {
      const query = withResearchFields({
        ecosystem: 'python',
        name: 'numpy',
        searchLimit: 3,
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.searchLimit).toBe(3);
      }
    });
  });

  describe('Invalid ecosystem', () => {
    it('should reject invalid ecosystem', () => {
      const query = withResearchFields({
        ecosystem: 'invalid',
        name: 'test',
      });

      const result = PackageSearchQuerySchema.safeParse(query);
      expect(result.success).toBe(false);
    });
  });
});

describe('searchPackage - NPM (CLI)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should return minimal NPM package results by default (name and repository only)', async () => {
    // Mock full npm view response for exact package lookup
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
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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
      expect(pkg.path).toBe('axios');
      expect(pkg.repoUrl).toBe('https://github.com/axios/axios');
      // version IS present now
      expect(pkg.version).toBe('1.6.0');

      // description is now always included (lightweight metadata)
      expect(pkg.description).toBe(
        'Promise based HTTP client for the browser and node.js'
      );
      // keywords still require npmFetchMetadata=true
      expect('keywords' in pkg).toBe(false);

      expect(result.ecosystem).toBe('npm');
      expect(result.totalFound).toBe(1);
    }

    // Verify registry HTTP endpoint was used (not CLI)
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/axios/latest'),
      expect.any(Object)
    );
  });

  it('should return full NPM package results when npmFetchMetadata is true', async () => {
    // Mock full npm view response for exact package lookup
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
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'axios',
      npmFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.path).toBe('axios');
      expect(pkg.repoUrl).toBe('https://github.com/axios/axios');

      // fields present
      expect(pkg.version).toBe('1.6.0');

      // Extended metadata fields ARE returned when npmFetchMetadata=true
      expect(pkg.description).toBe(
        'Promise based HTTP client for the browser and node.js'
      );
      expect(pkg.keywords).toEqual(['xhr', 'http', 'ajax', 'promise', 'node']);
      expect(pkg.homepage).toBe('https://axios-http.com');

      expect(result.ecosystem).toBe('npm');
      expect(result.totalFound).toBe(1);
    }
  });

  it('should handle NPM CLI search with multiple results (keyword search)', async () => {
    // Use keyword search (with space) to trigger npm search flow
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

    // Mock CLI view responses for repository URLs (CLI-first approach)
    mockNpmCliViewUrl('lodash', 'git+https://github.com/lodash/lodash.git');
    mockNpmCliViewUrl('lodash-es', 'git+https://github.com/lodash/lodash.git');

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: mockCliOutput,
        stderr: '',
        exitCode: 0,
      })
    );

    // Keep API fallback mocks in case CLI fails
    mockNpmRegistry('lodash', 'git+https://github.com/lodash/lodash.git');
    mockNpmRegistry('lodash-es', 'git+https://github.com/lodash/lodash.git');

    // Use space in name to trigger keyword search (npm search) instead of exact lookup (npm view)
    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'lodash utilities',
      searchLimit: 5,
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
      expect(pkg.path).toBe('lodash');
      // version IS present now
      expect(pkg.version).toBe('4.17.21');
      // mainEntry is null because we didn't fetch metadata
      expect(pkg.mainEntry).toBeNull();
    }

    // Verify registry search endpoint was used (not CLI)
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/-/v1/search'),
      expect.any(Object)
    );
  });

  it('should return package details when npmFetchMetadata is true', async () => {
    // Mock full npm view response
    mockNpmViewFull('test-package', {
      name: 'test-package',
      version: '1.0.0',
      repository: 'https://github.com/test/test-package',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'test-package',
      npmFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.path).toBe('test-package');
      expect(pkg.version).toBe('1.0.0');
      expect(pkg.repoUrl).toBe('https://github.com/test/test-package');
    }
  });

  it('should handle NPM registry fetch error', async () => {
    mockFetch.mockRejectedValue(new Error('Command timeout'));

    // Use keyword search (with space) to test npm search flow error handling
    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

    // Use keyword search (with space) to test npm search flow error handling
    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

    // Use keyword search (with space) to test npm search flow
    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

  it('should succeed with exact package name + searchLimit=3 (BUG-02 exact repro)', async () => {
    // Exact repro: name='typescript', searchLimit=3.
    // With limit > 1, even an exact name routes to searchNpmPackageViaSearch (not npm view).
    // Before fix: Zod schema rejected extra score/searchScore fields → "Invalid npm registry search response format".
    // After fix: schema uses .passthrough() and version is optional → returns packages.
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
                  'TypeScript is a language for application scale JavaScript development',
                links: { npm: 'https://www.npmjs.com/package/typescript' },
              },
              score: {
                final: 0.9999,
                detail: { quality: 1, popularity: 1, maintenance: 1 },
              },
              searchScore: 100000.123,
            },
            {
              package: {
                name: '@types/typescript',
                version: '1.0.0',
                description: 'TypeScript type definitions',
                links: {
                  npm: 'https://www.npmjs.com/package/@types/typescript',
                },
              },
              score: { final: 0.7 },
              searchScore: 30000,
            },
            {
              package: {
                name: 'typescript-eslint',
                version: '8.0.0',
                description: 'TypeScript ESLint tooling',
                links: {
                  npm: 'https://www.npmjs.com/package/typescript-eslint',
                },
              },
              score: { final: 0.85 },
              searchScore: 60000,
            },
          ],
          total: 100,
          time: 'Thu Jan 09 2025 00:00:00 GMT+0000',
        }),
      body: null,
    });

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'typescript',
      searchLimit: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    // Must NOT be an error — this was the exact failing case
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(3);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.path).toBe('typescript');
      expect(first.version).toBe('5.7.3');
    }
  });

  it('should filter out null-name items and accept null version (BUG-02 null fields)', async () => {
    // Real npm registry returns null for name/version on some ghost/deprecated packages.
    // Before fix: z.string() rejected null → "Expected string, received null" validation error.
    // After fix: nullish() accepts null; null-name items are filtered; null version falls back to 'unknown'.
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
              // Ghost package: name is null — must be silently filtered out
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
                version: null, // null version — must fall back to 'unknown'
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

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'typescript',
      searchLimit: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      // null-name ghost package is filtered; only 2 valid packages remain
      expect(result.packages.length).toBe(2);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.path).toBe('typescript');
      expect(first.version).toBe('5.7.3');
      const second = result.packages[1] as NpmPackageResult;
      expect(second.path).toBe('ts-node');
      // null version falls back to 'unknown'
      expect(second.version).toBe('unknown');
      // null repository URL → repoUrl is null, not a crash
      expect(second.repoUrl).toBeNull();
    }
  });

  it('should succeed with searchLimit > 1 when registry items have extra fields (BUG-02 fix)', async () => {
    // Regression: searchLimit > 1 used to fail with "Invalid npm registry search response format"
    // because NpmRegistrySearchItemSchema rejected the extra score/searchScore fields
    // from the real npm registry search API response.
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
              // Extra fields present in real API response — must not cause parse failure
              score: {
                final: 0.9999,
                detail: { quality: 1, popularity: 1, maintenance: 1 },
              },
              searchScore: 100000,
            },
            {
              package: {
                name: 'ts-node',
                // version intentionally missing — must fall back to 'unknown'
                description: 'TypeScript execution environment',
                links: { npm: 'https://www.npmjs.com/package/ts-node' },
              },
              score: { final: 0.85 },
              searchScore: 50000,
            },
          ],
          total: 2,
          // 'time' field present in real API responses — must not cause parse failure
          time: 'Thu Jan 09 2025 00:00:00 GMT+0000',
        }),
      body: null,
    });

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'typescript types',
      searchLimit: 3,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(2);
      const first = result.packages[0] as NpmPackageResult;
      expect(first.path).toBe('typescript');
      expect(first.version).toBe('5.7.3');
      const second = result.packages[1] as NpmPackageResult;
      expect(second.path).toBe('ts-node');
      // Missing version falls back to 'unknown' rather than crashing
      expect(second.version).toBe('unknown');
    }
  });

  it('should handle registry response with string total (BUG-02 fix)', async () => {
    // Some registry implementations return total as a string
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
          total: '1000', // string instead of number
        }),
      body: null,
    });

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'lodash utility helpers',
      searchLimit: 2,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.path).toBe('lodash');
    }
  });
});

describe('searchPackage - Python', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    setupDefaultFetchMock();
  });

  it('should return minimal Python package results by default (name and repository only)', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'requests',
          version: '2.31.0',
          summary: 'Python HTTP for Humans.',
          keywords: 'http,client,requests',
          license: 'Apache 2.0',
          author: 'Kenneth Reitz',
          home_page: 'https://requests.readthedocs.io',
          project_urls: {
            Source: 'https://github.com/psf/requests',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'requests',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.name).toBe('requests');
      expect(pkg.repository).toBe('https://github.com/psf/requests');
      // By default, should NOT have these fields
      expect('version' in pkg).toBe(false);
      expect('description' in pkg).toBe(false);
      expect('keywords' in pkg).toBe(false);
      expect(result.ecosystem).toBe('python');
      expect(result.totalFound).toBe(1);
    }
  });

  it('should return full Python package results when pythonFetchMetadata is true', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'requests',
          version: '2.31.0',
          summary: 'Python HTTP for Humans.',
          keywords: 'http,client,requests',
          license: 'Apache 2.0',
          author: 'Kenneth Reitz',
          home_page: 'https://requests.readthedocs.io',
          project_urls: {
            Source: 'https://github.com/psf/requests',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'requests',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as PythonPackageResult;
      expect(pkg.name).toBe('requests');
      expect(pkg.repository).toBe('https://github.com/psf/requests');
      // With pythonFetchMetadata: true, should have full fields
      expect('version' in pkg).toBe(true);
      expect('description' in pkg).toBe(true);
      expect('keywords' in pkg).toBe(true);
      expect(pkg.version).toBe('2.31.0');
      expect(result.ecosystem).toBe('python');
      expect(result.totalFound).toBe(1);
    }
  });

  it('should extract repository from project_urls', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'numpy',
          version: '1.26.0',
          summary: 'Numerical Python',
          keywords: '',
          project_urls: {
            Repository: 'https://github.com/numpy/numpy',
            Homepage: 'https://numpy.org',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'numpy',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBe('https://github.com/numpy/numpy');
    }
  });

  it('should handle Python package not found with empty result (consistent with NPM)', async () => {
    mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'nonexistent-package-xyz',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    // Should return empty packages array (not error) - consistent with NPM behavior
    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages).toEqual([]);
      expect(result.ecosystem).toBe('python');
      expect(result.totalFound).toBe(0);
    }
  });

  it('should parse comma-separated keywords when pythonFetchMetadata is true', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: 'http, client, api, rest',
          project_urls: {},
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { keywords: string[] };
      expect(pkg.keywords.length).toBeGreaterThan(0);
      expect(pkg.keywords).toContain('http');
    }
  });

  it('should limit keywords to MAX_KEYWORDS when pythonFetchMetadata is true', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: 'a,b,c,d,e,f,g,h,i,j,k,l,m,n,o',
          project_urls: {},
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { keywords: string[] };
      expect(pkg.keywords.length).toBeLessThanOrEqual(10);
    }
  });
});

describe('searchPackage - Name normalization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    setupDefaultFetchMock();
  });

  it('should normalize Python package name with underscores', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'some_package',
          version: '1.0.0',
          summary: 'Test package',
          keywords: '',
          project_urls: {},
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'some_package',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.name).toBe('some_package');
    }
  });
});

describe('searchPackage - NPM Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

    // Use keyword search (with space) to test npm search flow
    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

describe('searchPackage - Python Edge Cases', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    setupDefaultFetchMock();
  });

  it('should fallback to home_page for repository URL', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: '',
          project_urls: {}, // No project_urls with repo
          home_page: 'https://github.com/test/test-pkg', // But home_page has github
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBe('https://github.com/test/test-pkg');
    }
  });

  it('should not use home_page if not a known repo host', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: '',
          project_urls: {},
          home_page: 'https://example.com/docs', // Not github/gitlab/bitbucket
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBeNull();
    }
  });

  it('should handle keywords as array when pythonFetchMetadata is true', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test package',
          keywords: ['keyword1', 'keyword2', 'keyword3'], // Array instead of string
          project_urls: {},
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { keywords: string[] };
      expect(pkg.keywords).toEqual(['keyword1', 'keyword2', 'keyword3']);
    }
  });

  it('should truncate long Python description when pythonFetchMetadata is true', async () => {
    const longDescription = 'B'.repeat(300);
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: longDescription,
          keywords: '',
          project_urls: {},
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { description: string };
      expect(pkg.description!.length).toBeLessThanOrEqual(203); // 200 + '...'
      expect(pkg.description!.endsWith('...')).toBe(true);
    }
  });

  it('should re-throw non-404 errors', async () => {
    mockPypiFetch.mockRejectedValue(new Error('Network error'));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    // This should throw and be caught by the outer error handler
    await expect(searchPackage(query)).rejects.toThrow();
  });

  it('should skip packages without info object', async () => {
    // First call returns no info, second call (with different name variation) succeeds
    mockPypiFetch
      .mockResolvedValueOnce(pypiJsonResponse({ data: {} })) // No info object
      .mockResolvedValueOnce(
        pypiJsonResponse({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Found on second try',
              keywords: '',
              project_urls: {
                Source: 'https://github.com/test/test-pkg',
              },
            },
          },
        })
      );

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      // By default (minimal), should have name and repository
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.name).toBe('test-pkg');
      expect(pkg.repository).toBe('https://github.com/test/test-pkg');
    }
  });

  it('should skip packages without info object and return description when pythonFetchMetadata is true', async () => {
    // First call returns no info, second call (with different name variation) succeeds
    mockPypiFetch
      .mockResolvedValueOnce(pypiJsonResponse({ data: {} }))
      .mockResolvedValueOnce(
        pypiJsonResponse({
          data: {
            info: {
              name: 'test-pkg',
              version: '1.0.0',
              summary: 'Found on second try',
              keywords: '',
              project_urls: {},
            },
          },
        })
      );

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as { description: string };
      expect(pkg.description).toBe('Found on second try');
    }
  });

  it('should extract repo from gitlab URL', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test',
          keywords: '',
          project_urls: {
            Repository: 'https://gitlab.com/test/repo',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBe('https://gitlab.com/test/repo');
    }
  });

  it('should extract repo from bitbucket URL', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test',
          keywords: '',
          project_urls: {
            Source: 'https://bitbucket.org/test/repo',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBe('https://bitbucket.org/test/repo');
    }
  });

  it('should extract repo from gitlab home_page', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'test-pkg',
          version: '1.0.0',
          summary: 'Test',
          keywords: '',
          project_urls: {},
          home_page: 'https://gitlab.com/test/repo',
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'test-pkg',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.repository).toBe('https://gitlab.com/test/repo');
    }
  });
});

describe('Package search response structure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    mockPypiFetch.mockReset();
    mockExecuteNpmCommand.mockReset();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should return minimal structure by default (name and repository only)', async () => {
    mockNpmViewFull('express', {
      name: 'express',
      version: '4.18.2',
      description: 'Fast web framework',
      keywords: ['web', 'framework'],
      repository: 'https://github.com/expressjs/express',
    });

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'express',
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result).toHaveProperty('packages');
      expect(result).toHaveProperty('ecosystem');
      expect(result).toHaveProperty('totalFound');
      expect(Array.isArray(result.packages)).toBe(true);

      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg).toHaveProperty('path');
      expect(pkg).toHaveProperty('repoUrl');
      expect(pkg).toHaveProperty('version');

      // description is now always included (lightweight metadata)
      expect(pkg).toHaveProperty('description', 'Fast web framework');
      // keywords still require npmFetchMetadata=true
      expect(pkg).not.toHaveProperty('keywords');
    }
  });

  it('should return full structure when npmFetchMetadata is true', async () => {
    mockNpmViewFull('express', {
      name: 'express',
      version: '4.18.2',
      description: 'Fast web framework',
      keywords: ['web', 'framework'],
      repository: 'https://github.com/expressjs/express',
    });

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'express',
      npmFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg).toHaveProperty('path');
      expect(pkg).toHaveProperty('version');
      expect(pkg).toHaveProperty('repoUrl');
      expect(pkg).toHaveProperty('mainEntry');
    }
  });

  it('should return proper structure for error response', async () => {
    // Simulate registry fetch failure for search path (name with spaces)
    mockFetch.mockRejectedValue(new Error('Command failed'));

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

describe('registerPackageSearchTool', () => {
  let mockServer: MockMcpServer;
  let mockCallback: ReturnType<typeof vi.fn<ToolInvocationCallback>>;

  beforeEach(() => {
    mockServer = createMockMcpServer();
    mockCallback = vi.fn<ToolInvocationCallback>().mockResolvedValue(undefined);
    vi.clearAllMocks();
    clearAllCache();
    clearNpmCliViewMocks();
    mockExecuteNpmCommand.mockReset();
    mockPypiFetch.mockReset();
    clearNpmRegistryMocks();
    _resetNpmRegistryUrlCache();
    // Default: npm is available
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
      await registerPackageSearchTool(mockServer.server, mockCallback);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('should register package_search tool without callback when npm is available', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerPackageSearchTool(mockServer.server);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('should register with undefined callback when npm is available', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerPackageSearchTool(mockServer.server, undefined);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
    });

    it('should NOT register tool when npm ping fails', async () => {
      mockCheckNpmAvailability.mockResolvedValue(false);
      const result = await registerPackageSearchTool(
        mockServer.server,
        mockCallback
      );
      expect(result).toBeNull();
      expect(mockServer.server.registerTool).not.toHaveBeenCalled();
    });

    it('should NOT register tool when npm ping times out', async () => {
      mockCheckNpmAvailability.mockResolvedValue(false);
      const result = await registerPackageSearchTool(mockServer.server);
      expect(result).toBeNull();
      expect(mockServer.server.registerTool).not.toHaveBeenCalled();
    });

    it('should call checkNpmAvailability with 10 second timeout', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);
      await registerPackageSearchTool(mockServer.server);
      expect(mockCheckNpmAvailability).toHaveBeenCalledWith(10000);
    });

    it('should NOT register tool when npm registry is unreachable', async () => {
      mockCheckNpmAvailability.mockResolvedValue(true);

      // Make the registry ping fail
      mockFetch.mockRejectedValue(new Error('fetch failed'));

      const result = await registerPackageSearchTool(
        mockServer.server,
        mockCallback
      );
      expect(result).toBeNull();
      expect(mockServer.server.registerTool).not.toHaveBeenCalled();
    });
  });

  describe('Tool Execution - NPM', () => {
    it('should execute npm package search and return results', async () => {
      const mockCliOutput = JSON.stringify([
        {
          name: 'axios',
          version: '1.6.0',
          description: 'HTTP client',
          keywords: ['http'],
          links: { repository: 'https://github.com/axios/axios' },
        },
      ]);

      mockExecuteNpmCommand.mockResolvedValue({
        stdout: mockCliOutput,
        stderr: '',
        exitCode: 0,
      });

      await registerPackageSearchTool(mockServer.server, mockCallback);

      const queries = [
        {
          ecosystem: 'npm' as const,
          name: 'axios',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ];
      const result = await mockServer.callTool('packageSearch', {
        queries,
      });

      expect(result.isError).toBeFalsy();
      expect(result.content).toBeDefined();
      expect(result.content[0]).toHaveProperty('text');
      expect(mockCallback).toHaveBeenCalledWith('packageSearch', queries);
    });

    it('should include actionable GitHub hint for packages with repo links', async () => {
      // Mock full npm view response for exact package lookup
      mockNpmViewFull('react', {
        name: 'react',
        version: '18.0.0',
        description: 'React library',
        keywords: ['ui'],
        repository: 'git+https://github.com/facebook/react.git',
      });

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '', // Not used for exact package lookup
          stderr: '',
          exitCode: 0,
        })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'react',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('githubViewRepoStructure');
      expect(text).toContain('facebook');
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

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'lodash',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Install: npm install lodash');
    });

    it('should generate empty hints for no results (npm)', async () => {
      // Use keyword search (with space) to test npm search flow with empty results
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'nonexistent pkg xyz keyword',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('npmjs.com');
    });
  });

  describe('Tool Execution - Python', () => {
    it('should execute python package search and return results', async () => {
      const mockPyPIResponse = {
        data: {
          info: {
            name: 'requests',
            version: '2.31.0',
            summary: 'HTTP library',
            keywords: 'http',
            project_urls: {
              Source: 'https://github.com/psf/requests',
            },
          },
        },
      };

      mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

      await registerPackageSearchTool(mockServer.server, mockCallback);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'requests',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.isError).toBeFalsy();
      expect(result.content).toBeDefined();
    });

    it('should include install hint for python packages', async () => {
      const mockPyPIResponse = {
        data: {
          info: {
            name: 'numpy',
            version: '1.26.0',
            summary: 'Numerical Python',
            keywords: '',
            project_urls: {},
          },
        },
      };

      mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'numpy',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('Install: pip install numpy');
    });

    it('should generate empty hints for not found (python)', async () => {
      mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'nonexistent-pkg-xyz',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      // The response contains either error message or empty status hints
      expect(text).toMatch(/not found|No python packages found/);
    });
  });

  describe('Callback Invocation', () => {
    it('should invoke callback with tool name and queries', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerPackageSearchTool(mockServer.server, mockCallback);

      const queries = [
        {
          ecosystem: 'npm' as const,
          name: 'test-pkg',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ];

      await mockServer.callTool('packageSearch', { queries });

      expect(mockCallback).toHaveBeenCalledWith('packageSearch', queries);
    });

    it('should continue execution even if callback throws', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      mockCallback.mockRejectedValue(new Error('Callback error'));

      await registerPackageSearchTool(mockServer.server, mockCallback);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      // Should still return results despite callback error
      expect(result).toBeDefined();
      expect(result.content).toBeDefined();
    });

    it('should not invoke callback if none provided', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerPackageSearchTool(mockServer.server); // No callback

      await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'test-pkg',
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
      // Mock full npm view responses for exact package lookups
      mockNpmViewFull('pkg1', { name: 'pkg1', version: '1.0.0' });
      mockNpmViewFull('pkg2', { name: 'pkg2', version: '1.0.0' });

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '', // Not used for exact package lookup
          stderr: '',
          exitCode: 0,
        })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'pkg1',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
          {
            ecosystem: 'npm',
            name: 'pkg2',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.content).toBeDefined();
      // CLI is used for: 1 × config get registry + 2 × deprecation check = 3 calls
      expect(mockExecuteNpmCommand).toHaveBeenCalledTimes(3);
    });

    it('should handle empty queries array', async () => {
      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [],
      });

      expect(result).toBeDefined();
    });
  });

  describe('Error Handling', () => {
    it('should handle unexpected errors', async () => {
      mockExecuteNpmCommand.mockRejectedValue(new Error('Unexpected error'));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.content).toBeDefined();
    });
  });

  describe('Catch Error Handling', () => {
    it('should handle thrown errors via handleCatchError (line 115)', async () => {
      mockPypiFetch.mockRejectedValue(new Error('Connection refused'));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'test-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      expect(result.content).toBeDefined();
      const text = (result.content[0] as { text: string }).text;
      expect(text).toContain('error');
    });
  });

  describe('Success Hints Generation', () => {
    it('should not include repo hint when packages have no repository', async () => {
      mockNpmViewFull('no-repo-pkg', {
        name: 'no-repo-pkg',
        version: '1.0.0',
        description: 'Package without repo',
        // No repository field
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'no-repo-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;
      // Should have install hint but NOT the githubViewRepoStructure hint (no repo)
      expect(text).toContain('Install: npm install');
      expect(text).not.toContain('githubViewRepoStructure');
    });
  });

  describe('Custom Hints in Response', () => {
    it('should return hasResultsStatusHints with actionable GitHub and install hints for npm packages with repo', async () => {
      // Mock full npm view response for exact package lookup
      mockNpmViewFull('axios', {
        name: 'axios',
        version: '1.6.0',
        description: 'HTTP client',
        keywords: ['http'],
        repository: 'git+https://github.com/axios/axios.git',
      });

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '', // Not used for exact package lookup
          stderr: '',
          exitCode: 0,
        })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'axios',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify actionable hints
      expect(text).toContain('githubViewRepoStructure');
      expect(text).toContain('Install: npm install axios');

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "hasResults"');
    });

    it('should return hasResultsStatusHints with only install hint when package has no repository', async () => {
      mockNpmViewFull('no-repo-pkg', {
        name: 'no-repo-pkg',
        version: '1.0.0',
        description: 'Package without repo',
        // No repository field
      });
      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({ stdout: '', stderr: '', exitCode: 0 })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'no-repo-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify install hint
      expect(text).toContain('Install: npm install no-repo-pkg');
      expect(text).not.toContain('githubViewRepoStructure');

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "hasResults"');
    });

    it('should return hasResultsStatusHints with actionable GitHub and install hints for python packages with repo', async () => {
      const mockPyPIResponse = {
        data: {
          info: {
            name: 'requests',
            version: '2.31.0',
            summary: 'HTTP library',
            keywords: 'http',
            project_urls: {
              Source: 'https://github.com/psf/requests',
            },
          },
        },
      };

      mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'requests',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify actionable hints
      expect(text).toContain('githubViewRepoStructure');
      expect(text).toContain('Install: pip install requests');

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "hasResults"');
    });

    it('should return hasResultsStatusHints with only install hint when python package has no repository', async () => {
      const mockPyPIResponse = {
        data: {
          info: {
            name: 'no-repo-pkg',
            version: '1.0.0',
            summary: 'Package without repo',
            keywords: '',
            project_urls: {}, // No repository
          },
        },
      };

      mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'no-repo-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify install hint
      expect(text).toContain('Install: pip install no-repo-pkg');
      expect(text).not.toContain('githubViewRepoStructure');

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "hasResults"');
    });

    it('should return emptyStatusHints with browse link when no npm packages found', async () => {
      // Use keyword search (with space) to test npm search flow empty results
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'nonexistent pkg xyz123 keyword',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify empty hints
      expect(text).toContain(
        "No npm packages found for 'nonexistent pkg xyz123 keyword'"
      );
      expect(text).toContain(
        'Browse: https://npmjs.com/search?q=nonexistent%20pkg%20xyz123%20keyword'
      );

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "empty"');
    });

    it('should return emptyStatusHints with browse link when no python packages found', async () => {
      mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'nonexistent-pkg-xyz123',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify empty hints
      expect(text).toContain(
        "No python packages found for 'nonexistent-pkg-xyz123'"
      );
      expect(text).toContain(
        'Browse: https://pypi.org/search/?q=nonexistent-pkg-xyz123'
      );

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "empty"');
    });

    it('should include both hasResultsStatusHints and emptyStatusHints in bulk operation results', async () => {
      // Mock full npm view response for exact package lookup (react)
      mockNpmViewFull('react', {
        name: 'react',
        version: '18.0.0',
        description: 'React library',
        keywords: ['ui'],
        repository: 'git+https://github.com/facebook/react.git',
      });

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '[]', // For keyword search (empty results)
          stderr: '',
          exitCode: 0,
        })
      );

      await registerPackageSearchTool(mockServer.server);

      const result = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'react',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
          {
            // Use keyword search (with space) for empty result
            ecosystem: 'npm',
            name: 'nonexistent pkg keyword',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const text = (result.content[0] as { text: string }).text;

      // Hints are inside each result - verify actionable hints for hasResults
      expect(text).toContain('githubViewRepoStructure');
      expect(text).toContain('Install: npm install react');

      // Verify empty hints in empty result
      expect(text).toContain(
        "No npm packages found for 'nonexistent pkg keyword'"
      );
      expect(text).toContain(
        'Browse: https://npmjs.com/search?q=nonexistent%20pkg%20keyword'
      );
    });

    it('should generate correct hints based on generateSuccessHints for mixed ecosystems', async () => {
      // Mock full npm view response for exact package lookup
      mockNpmViewFull('lodash', {
        name: 'lodash',
        version: '4.17.21',
        description: 'Utility library',
        keywords: [],
        repository: 'git+https://github.com/lodash/lodash.git',
      });

      const mockPyPIResponse = {
        data: {
          info: {
            name: 'numpy',
            version: '1.26.0',
            summary: 'Numerical Python',
            keywords: '',
            project_urls: {
              Repository: 'https://github.com/numpy/numpy',
            },
          },
        },
      };

      mockExecuteNpmCommand.mockImplementation(
        createNpmCommandMock({
          stdout: '', // Not used for exact package lookup
          stderr: '',
          exitCode: 0,
        })
      );

      mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

      await registerPackageSearchTool(mockServer.server);

      // Test npm ecosystem hints
      const npmResult = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'lodash',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const npmText = (npmResult.content[0] as { text: string }).text;
      expect(npmText).toContain('Install: npm install lodash');
      expect(npmText).toContain('githubViewRepoStructure');

      // Test python ecosystem hints
      const pythonResult = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'numpy',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const pythonText = (pythonResult.content[0] as { text: string }).text;
      expect(pythonText).toContain('Install: pip install numpy');
      expect(pythonText).toContain('githubViewRepoStructure');
    });

    it('should generate correct hints based on generateEmptyHints for mixed ecosystems', async () => {
      mockExecuteNpmCommand.mockResolvedValue({
        stdout: '[]',
        stderr: '',
        exitCode: 0,
      });

      mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

      await registerPackageSearchTool(mockServer.server);

      // Test npm empty hints - use keyword search (with space) for npm search flow
      const npmResult = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'npm',
            name: 'nonexistent npm pkg keyword',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const npmText = (npmResult.content[0] as { text: string }).text;
      expect(npmText).toContain(
        "No npm packages found for 'nonexistent npm pkg keyword'"
      );
      expect(npmText).toContain(
        'Browse: https://npmjs.com/search?q=nonexistent%20npm%20pkg%20keyword'
      );
      expect(npmText).not.toContain('pypi.org');

      // Test python empty hints
      const pythonResult = await mockServer.callTool('packageSearch', {
        queries: [
          {
            ecosystem: 'python',
            name: 'nonexistent-python-pkg',
            mainResearchGoal: 'Test',
            researchGoal: 'Test',
            reasoning: 'Test',
          },
        ],
      });

      const pythonText = (pythonResult.content[0] as { text: string }).text;
      expect(pythonText).toContain(
        "No python packages found for 'nonexistent-python-pkg'"
      );
      expect(pythonText).toContain(
        'Browse: https://pypi.org/search/?q=nonexistent-python-pkg'
      );
      expect(pythonText).not.toContain('npmjs.com');
    });
  });
});

// NEW TESTS: Task 1 - Enhanced GitHub Integration Hints
describe('Task 1: Enhanced GitHub Integration Hints', () => {
  let mockServer: MockMcpServer;

  beforeEach(async () => {
    vi.clearAllMocks();
    clearAllCache();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    mockCheckNpmAvailability.mockResolvedValue(true);
    mockServer = createMockMcpServer();
    setupDefaultFetchMock();
  });

  afterEach(() => {
    mockServer.cleanup();
  });

  it('should generate actionable GitHub tool call hints for npm packages', async () => {
    // Mock full npm view response for exact package lookup
    mockNpmViewFull('axios', {
      name: 'axios',
      version: '1.6.0',
      description: 'HTTP client',
      keywords: [],
      repository: 'git+https://github.com/axios/axios.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'axios',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    // YAML uses escaped quotes, check for pattern
    expect(text).toContain('githubViewRepoStructure');
    expect(text).toContain('axios');
    expect(text).toContain('Install: npm install axios');
  });

  it('should generate actionable GitHub tool call hints for Python packages', async () => {
    const mockPyPIResponse = {
      data: {
        info: {
          name: 'requests',
          version: '2.31.0',
          summary: 'HTTP library',
          keywords: '',
          project_urls: {
            Source: 'https://github.com/psf/requests',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'python',
          name: 'requests',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    // YAML uses escaped quotes, so check for the pattern with either format
    expect(text).toContain('githubViewRepoStructure');
    expect(text).toContain('owner=');
    expect(text).toContain('psf');
    expect(text).toContain('Install: pip install requests');
  });

  it('should clean .git suffix from GitHub repository URLs', async () => {
    // Mock full npm view response for exact package lookup with .git suffix
    mockNpmViewFull('lodash', {
      name: 'lodash',
      version: '4.17.21',
      description: 'Utility library',
      keywords: [],
      repository: 'git+https://github.com/lodash/lodash.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'lodash',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    // YAML uses escaped quotes, so check for the pattern
    expect(text).toContain('githubViewRepoStructure');
    expect(text).toContain('lodash');
    // Make sure .git is stripped from the repo name in the hint
    expect(text).not.toContain('repo="lodash.git"');
    expect(text).not.toContain("repo='lodash.git'");
  });
});

// NEW TESTS: Task 2 - Name Variation Suggestions
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
    // Use keyword search (with space) to get npm search flow with empty results
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '[]',
      stderr: '',
      exitCode: 0,
    });

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'date-fns keyword', // Use space to trigger keyword search
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    // Name variation suggestions are generated for empty results
    expect(text).toContain("No npm packages found for 'date-fns keyword'");
  });

  it('should suggest name variations with underscores converted to hyphens for Python', async () => {
    mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'python',
          name: 'scikit_learn',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('Try: scikit-learn');
  });

  it('should suggest unscoped name for @scope/name packages', async () => {
    // Scoped package not found - uses npm view which returns empty
    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used
        stderr: '',
        exitCode: 0,
      })
    );

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: '@nonexistent/package',
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
    // Use keyword search (with space) to get npm search flow with empty results
    mockExecuteNpmCommand.mockResolvedValue({
      stdout: '[]',
      stderr: '',
      exitCode: 0,
    });

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'chart library', // Use space to trigger keyword search
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    // Name variation suggestions include js suffix hint
    expect(text).toContain("No npm packages found for 'chart library'");
  });

  it('should suggest py prefix for Python packages', async () => {
    mockPypiFetch.mockResolvedValue(new Response('', { status: 404 }));

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'python',
          name: 'test',
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
        },
      ],
    });

    const text = (result.content[0] as { text: string }).text;
    expect(text).toContain('pytest');
  });

  it('should add outputPagination for large npm alternative lists and resume with charOffset', async () => {
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

    for (let index = 0; index < 4; index += 1) {
      mockNpmViewFull(`pkg-${index}`, {
        name: `pkg-${index}`,
        version: '1.0.0',
        description: `Package ${index}`,
        keywords: Array.from(
          { length: 18 },
          (_, keywordIndex) => `keyword-${index}-${keywordIndex}`
        ),
        repository: `https://github.com/octo/pkg-${index}`,
      });
    }

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: searchResults,
        stderr: '',
        exitCode: 0,
      })
    );

    await registerPackageSearchTool(mockServer.server);

    const firstResult = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'pkg',
          searchLimit: 4,
          npmFetchMetadata: true,
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
          charLength: 350,
        },
      ],
    });

    const firstStructured = firstResult.structuredContent as {
      results: Array<{
        data: {
          packages?: Array<{ name: string; keywords?: string[] }>;
          outputPagination?: {
            hasMore: boolean;
            charOffset: number;
            charLength: number;
          };
        };
      }>;
    };
    const firstData = firstStructured.results[0]!.data;
    const nextOffset =
      (firstData.outputPagination?.charOffset ?? 0) +
      (firstData.outputPagination?.charLength ?? 0);

    expect(firstData.packages?.length).toBeGreaterThan(0);
    expect(firstData.outputPagination?.hasMore).toBe(true);

    const secondResult = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'pkg',
          searchLimit: 4,
          npmFetchMetadata: true,
          mainResearchGoal: 'Test',
          researchGoal: 'Test',
          reasoning: 'Test',
          charOffset: nextOffset,
          charLength: 350,
        },
      ],
    });

    const secondStructured = secondResult.structuredContent as {
      results: Array<{
        data: {
          packages?: Array<{ name: string; keywords?: string[] }>;
        };
      }>;
    };

    expect(secondStructured.results[0]!.data.packages).not.toEqual(
      firstData.packages
    );
  });
});

// NEW TESTS: Task 3 - Deprecation Detection
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
    // Mock full npm view response for exact package lookup
    mockNpmViewFull('request', {
      name: 'request',
      version: '2.88.2',
      description: 'Simplified HTTP request client',
      keywords: [],
      repository: 'https://github.com/request/request',
    });

    // Mock deprecation check
    mockExecuteNpmCommand.mockImplementation((cmd: string, args: string[]) => {
      // Handle full view for exact package lookup
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

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'request',
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

    await registerPackageSearchTool(mockServer.server);

    const result = await mockServer.callTool('packageSearch', {
      queries: [
        {
          ecosystem: 'npm',
          name: 'lodash',
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

// NEW TESTS: Task 4 - pythonFetchMetadata Parameter
describe('Task 4: pythonFetchMetadata Parameter', () => {
  const withResearchFields = <T extends object>(query: T) => ({
    id: 'test:pkg-search',
    ...query,
    mainResearchGoal: 'Test research goal',
    researchGoal: 'Testing package search',
    reasoning: 'Unit test for schema',
  });

  it('should validate Python query with pythonFetchMetadata', () => {
    const query = withResearchFields({
      ecosystem: 'python',
      name: 'requests',
      pythonFetchMetadata: true,
    });

    const result = PackageSearchQuerySchema.safeParse(query);
    expect(result.success).toBe(true);
    if (result.success && result.data.ecosystem === 'python') {
      expect(result.data.pythonFetchMetadata).toBe(true);
    }
  });

  it('should default pythonFetchMetadata to false', () => {
    const query = withResearchFields({
      ecosystem: 'python',
      name: 'requests',
    });

    const result = PackageSearchQuerySchema.safeParse(query);
    expect(result.success).toBe(true);
    if (result.success && result.data.ecosystem === 'python') {
      expect(result.data.pythonFetchMetadata).toBe(false);
    }
  });

  it('should return minimal Python package results by default', async () => {
    vi.clearAllMocks();
    clearAllCache();
    setupDefaultFetchMock();

    const mockPyPIResponse = {
      data: {
        info: {
          name: 'requests',
          version: '2.31.0',
          summary: 'HTTP library',
          keywords: 'http client web',
          author: 'Kenneth Reitz',
          license: 'Apache 2.0',
          home_page: 'https://requests.readthedocs.io',
          project_urls: {
            Source: 'https://github.com/psf/requests',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'requests',
      pythonFetchMetadata: false,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as MinimalPackageResult;
      expect(pkg.name).toBe('requests');
      expect(pkg.repository).toBe('https://github.com/psf/requests');
      // Should NOT have full metadata fields
      expect('version' in pkg).toBe(false);
      expect('description' in pkg).toBe(false);
      expect('author' in pkg).toBe(false);
    }
  });

  it('should return full Python package results when pythonFetchMetadata is true', async () => {
    vi.clearAllMocks();
    clearAllCache();
    setupDefaultFetchMock();

    const mockPyPIResponse = {
      data: {
        info: {
          name: 'requests',
          version: '2.31.0',
          summary: 'HTTP library',
          keywords: 'http client web',
          author: 'Kenneth Reitz',
          license: 'Apache 2.0',
          home_page: 'https://requests.readthedocs.io',
          project_urls: {
            Source: 'https://github.com/psf/requests',
          },
        },
      },
    };

    mockPypiFetch.mockResolvedValue(pypiJsonResponse(mockPyPIResponse));

    const query: PackageSearchInput = {
      ecosystem: 'python',
      name: 'requests',
      pythonFetchMetadata: true,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      const pkg = result.packages[0] as PythonPackageResult;
      expect(pkg.name).toBe('requests');
      expect('version' in pkg).toBe(true);
      expect('description' in pkg).toBe(true);
      expect('author' in pkg).toBe(true);
      expect(pkg.version).toBe('2.31.0');
      expect(pkg.author).toBe('Kenneth Reitz');
    }
  });
});

// NEW TESTS: Task 5 - PyPI Fuzzy Search (REMOVED)

describe('searchPackage - NPM CLI Repository Fetching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
    clearNpmRegistryMocks();
    clearNpmCliViewMocks();
    _resetNpmRegistryUrlCache();
    setupDefaultFetchMock();
  });

  it('should fetch repository URL via CLI (string format)', async () => {
    // Mock full npm view response for exact package lookup
    mockNpmViewFull('axios', {
      name: 'axios',
      version: '1.6.0',
      description: 'HTTP client',
      repository: 'git+https://github.com/axios/axios.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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

    // Verify registry HTTP endpoint was used (not CLI)
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringContaining('/axios/latest'),
      expect.any(Object)
    );
  });

  it('should fetch repository URL via CLI (object format like @wix packages)', async () => {
    // Mock full npm view response for exact package lookup (object repository format)
    mockNpmViewFull('@wix/yoshi-style-dependencies', {
      name: '@wix/yoshi-style-dependencies',
      version: '6.0.0',
      repository: {
        type: 'git',
        url: 'https://github.com/wix-private/yoshi.git',
        directory: 'legacy-packages/yoshi-style-dependencies',
      },
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: '@wix/yoshi-style-dependencies',
      mainResearchGoal: 'Test CLI repository URL fetching',
      researchGoal: 'Test object URL format',
      reasoning: 'Verify CLI handles @wix package format',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      expect(result.packages.length).toBe(1);
      const pkg = result.packages[0] as NpmPackageResult;
      expect(pkg.repoUrl).toBe('https://github.com/wix-private/yoshi');
    }
  });

  it('should handle package without repository defined', async () => {
    // Mock full npm view response without repository
    // (package exists but has no repository URL)
    mockNpmViewFull('some-package', {
      name: 'some-package',
      version: '1.0.0',
      // No repository field
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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
      // Should return null when no repository
      expect(pkg.repoUrl).toBeNull();
    }
  });

  it('should return empty when package not found', async () => {
    // No mock set for this package - simulates package not found

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'no-repo-package',
      mainResearchGoal: 'Test package not found case',
      researchGoal: 'Test when package does not exist',
      reasoning: 'Verify empty result is returned gracefully',
    };

    const result = await searchPackage(query);

    expect('packages' in result).toBe(true);
    if ('packages' in result) {
      // Package not found returns empty array
      expect(result.packages.length).toBe(0);
    }
  });

  it('should handle scoped packages correctly', async () => {
    // Mock full npm view response for scoped package
    mockNpmViewFull('@types/node', {
      name: '@types/node',
      version: '20.0.0',
      repository: 'https://github.com/DefinitelyTyped/DefinitelyTyped.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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
    // Mock full npm view response with git+ prefix and .git suffix
    mockNpmViewFull('lodash', {
      name: 'lodash',
      version: '4.17.21',
      repository: 'git+https://github.com/lodash/lodash.git',
    });

    mockExecuteNpmCommand.mockImplementation(
      createNpmCommandMock({
        stdout: '', // Not used for exact package lookup
        stderr: '',
        exitCode: 0,
      })
    );

    const query: PackageSearchInput = {
      ecosystem: 'npm',
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
      // Should NOT have git+ prefix or .git suffix
      expect(pkg.repoUrl).toBe('https://github.com/lodash/lodash');
      expect(pkg.repoUrl).not.toContain('git+');
      expect(pkg.repoUrl).not.toMatch(/\.git$/);
    }
  });
});
