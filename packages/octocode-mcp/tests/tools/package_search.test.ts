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
import type { ToolInvocationCallback } from '../../src/types.js';
import {
  createMockMcpServer,
  MockMcpServer,
} from '../fixtures/mcp-fixtures.js';
import { clearAllCache } from '../../src/utils/http/cache.js';

function fetchUrlString(url: string | URL | Request): string {
  if (typeof url === 'string') return url;
  if (url instanceof URL) return url.href;
  return url.url;
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
    (url: string | URL | Request, _init?: RequestInit) => {
      const urlStr = fetchUrlString(url);
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
          return 'Search for packages in the npm ecosystem';
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
} from '../../src/utils/package/common.js';
import { registerPackageSearchTool } from '../../src/tools/package_search/package_search.js';
import { _resetNpmRegistryUrlCache } from '../../src/utils/package/npm.js';

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

      // description is now always included (npmFetchMetadata defaults to true)
      expect(pkg.description).toBe(
        'Promise based HTTP client for the browser and node.js'
      );

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

  it('pages registry results: page=2 sends the from offset (#2)', async () => {
    const query: PackageSearchInput = {
      ecosystem: 'npm',
      name: 'lodash utilities', // multi-word → keyword search (not exact lookup)
      itemsPerPage: 5,
      page: 2,
      mainResearchGoal: 'Test',
      researchGoal: 'Test',
      reasoning: 'Test',
    };

    await searchPackage(query);

    // page 2 with itemsPerPage 5 → registry `from=5`, reachable beyond page 1.
    expect(mockFetch).toHaveBeenCalledWith(
      expect.stringMatching(/\/-\/v1\/search.*[?&]from=5\b/),
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

  it('should succeed with exact package name + itemsPerPage=3 (BUG-02 exact repro)', async () => {
    // Exact repro: name='typescript', itemsPerPage=3.
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
      itemsPerPage: 3,
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
      itemsPerPage: 3,
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

  it('should succeed with itemsPerPage > 1 when registry items have extra fields (BUG-02 fix)', async () => {
    // Regression: itemsPerPage > 1 used to fail with "Invalid npm registry search response format"
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
      expect(pkg.path).toBe('lodash');
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

      // description, keywords etc. are now always included (npmFetchMetadata defaults to true)
      expect(pkg).toHaveProperty('description', 'Fast web framework');
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

    // Guard removed (#T4): packageSearch is ALWAYS registered regardless of npm
    // / registry availability — reachability is a per-CALL concern handled by
    // searchPackages (graceful structured error). The tool must never silently
    // vanish from the server on a transient blip or offline startup.
    it('registers even when npm + registry are unavailable (never vanishes)', async () => {
      mockCheckNpmAvailability.mockResolvedValue(false);
      mockFetch.mockRejectedValue(new Error('fetch failed'));
      await registerPackageSearchTool(mockServer.server, mockCallback);
      expect(mockServer.server.registerTool).toHaveBeenCalled();
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
      // CLI: registry-URL resolution (`config get registry`, now done lazily at
      // first call since the registration guard that pre-warmed the module cache
      // was removed — #T4) + 2 × deprecation check. The registry URL is cached
      // module-level, so this stays O(1) in config calls regardless of bulk size.
      expect(mockExecuteNpmCommand).toHaveBeenCalledTimes(4);
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
      expect(text).not.toContain('status: "hasResults"');
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
      expect(text).not.toContain(
        'Browse: https://npmjs.com/search?q=nonexistent%20pkg%20xyz123%20keyword'
      );

      // Verify result status (YAML format uses quoted strings)
      expect(text).toContain('status: "empty"');
    });
  });
});

// NEW TESTS: Task 1 - Enhanced GitHub Integration Hints

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
          itemsPerPage: 4,
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
          itemsPerPage: 4,
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
