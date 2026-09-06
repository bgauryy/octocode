import {
  afterAll,
  afterEach,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from 'vitest';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { searchPackages } from '../../../src/tools/package_search/execution.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../src/tools/package_search/scheme.js';
import { resolveNpmRegistryContext } from '../../../src/utils/package/npm/npmRegistry.js';

const receipts: Array<{ path: string; authorized: boolean }> = [];
const names = ['private-one', 'private-two', 'private-three'];
let root: string;
let base: string;
let config: string;
const server = createServer((req, res) => {
  const url = new URL(req.url!, 'http://localhost');
  const authorized = req.headers.authorization === 'Bearer fixture-only';
  receipts.push({ path: url.pathname, authorized });
  res.setHeader('content-type', 'application/json');
  if (url.pathname.startsWith('/private/') && !authorized) {
    res.writeHead(401);
    res.end('{"error":"authentication required"}');
    return;
  }
  const version = url.pathname.startsWith('/scoped/') ? '2.0.0' : '1.0.0';
  if (url.pathname.endsWith('/-/v1/search')) {
    const from = Number(url.searchParams.get('from') ?? 0);
    const size = Number(url.searchParams.get('size'));
    res.end(
      JSON.stringify({
        total: names.length,
        objects: names
          .slice(from, from + size)
          .map(name => ({ package: { name, version } })),
      })
    );
    return;
  }
  const name = decodeURIComponent(
    url.pathname
      .replace(/^\/(public|private|scoped)\//, '')
      .replace(/\/latest$/, '')
  );
  res.end(
    JSON.stringify(
      url.pathname.endsWith('/latest')
        ? { name, version }
        : {
            name,
            'dist-tags': { latest: version },
            versions: { [version]: { name, version } },
          }
    )
  );
});

beforeAll(async () => {
  root = await mkdtemp(join(tmpdir(), 'octocode-npm-private-'));
  config = join(root, 'npmrc');
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
});
afterEach(() => vi.unstubAllEnvs());
afterAll(async () => {
  await new Promise<void>(resolve => server.close(() => resolve()));
  await rm(root, { recursive: true, force: true });
});

async function configure(auth = true) {
  await writeFile(
    config,
    `@fixture:registry=${base}/scoped/\n${auth ? `//127.0.0.1:${(server.address() as { port: number }).port}/private/:_authToken=fixture-only\n` : ''}`
  );
  vi.stubEnv('NPM_CONFIG_USERCONFIG', config);
  vi.stubEnv('npm_config_userconfig', config);
  vi.stubEnv('NPM_CONFIG_REGISTRY', base + '/private/');
  vi.stubEnv('npm_config_registry', base + '/private/');
  vi.stubEnv('OCTOCODE_DISK_CACHE', 'false');
  receipts.length = 0;
}

async function query(input: Record<string, unknown>) {
  const parsed = NpmSearchBulkQueryLocalSchema.parse({ queries: [input] });
  const result = await searchPackages({ queries: parsed.queries });
  return (
    result.structuredContent as {
      results: Array<{ data: Record<string, any> }>;
    }
  ).results[0]!.data;
}

describe('private registry public tool contract', () => {
  it('authenticates every executable keyword page and recovers the complete union', async () => {
    await configure();
    let input: Record<string, unknown> = { keywords: ['private'], pageSize: 2 };
    const found: string[] = [];
    for (let page = 0; page < 3; page++) {
      const data = await query(input);
      expect(data.error).toBeUndefined();
      found.push(...data.packages.map((p: { name: string }) => p.name));
      if (!data.pagination.hasMore) break;
      input = data.next.nextPage.query;
    }
    expect(found).toEqual(names);
    expect(receipts).toHaveLength(2);
    expect(receipts.every(r => r.authorized)).toBe(true);
  });

  it('reads exact private metadata in one authenticated request', async () => {
    await configure();
    expect(
      (await query({ packageName: 'private-exact' })).packages[0].version
    ).toBe('1.0.0');
    expect(receipts).toHaveLength(1);
    expect(receipts[0]!.authorized).toBe(true);
  });

  it('honors the scope registry even when the default registry has a matching name', async () => {
    await configure();
    const data = await query({ packageName: '@fixture/collision' });
    expect(data.packages[0].version).toBe('2.0.0');
    expect(receipts).toEqual([
      { path: '/scoped/@fixture%2Fcollision/latest', authorized: false },
    ]);
  });

  it('reloads credentials and does not reuse authenticated results after their removal', async () => {
    await configure();
    expect(
      (await query({ packageName: 'private-cache' })).packages
    ).toHaveLength(1);
    await configure(false);
    const data = await query({ packageName: 'private-cache' });
    expect(data.error).toBeDefined();
    expect(receipts.length).toBeGreaterThan(0);
  });

  it('lets an explicit registry override scope routing without forwarding another path credentials', async () => {
    await configure();
    const data = await query({
      packageName: '@fixture/override',
      registry: base + '/public/',
    });
    expect(data.packages[0].version).toBe('1.0.0');
    expect(receipts).toEqual([
      { path: '/public/@fixture%2Foverride/latest', authorized: false },
    ]);
  });

  it('pins keyword continuations to the resolved registry when the environment changes', async () => {
    await configure();
    const first = await query({ keywords: ['pin-registry'], pageSize: 2 });
    expect(first.next.nextPage.query.registry).toBe(base + '/private');
    vi.stubEnv('NPM_CONFIG_REGISTRY', base + '/public/');
    vi.stubEnv('npm_config_registry', base + '/public/');
    const second = await query(first.next.nextPage.query);
    expect(second.packages.map((p: { name: string }) => p.name)).toEqual([
      'private-three',
    ]);
    expect(
      receipts.every(r => r.path.startsWith('/private/') && r.authorized)
    ).toBe(true);
  });

  it('observes registry and custom credential interpolation changes without a process restart', async () => {
    await configure();
    vi.stubEnv('OCTOCODE_FIXTURE_NPM_TOKEN', 'fixture-only');
    await writeFile(
      config,
      `//127.0.0.1:${(server.address() as { port: number }).port}/private/:_authToken=\${OCTOCODE_FIXTURE_NPM_TOKEN}\n`
    );
    expect(
      (await query({ packageName: 'interpolated' })).packages
    ).toHaveLength(1);
    vi.stubEnv('NPM_CONFIG_REGISTRY', base + '/public/');
    vi.stubEnv('npm_config_registry', base + '/public/');
    expect((await resolveNpmRegistryContext()).registry).toBe(base + '/public');
    expect(receipts[0]!.authorized).toBe(true);
  });

  it.each([
    'https://user:secret@example.test',
    'file:///tmp/npm',
    'https://example.test?token=secret',
    'https://example.test/#secret',
  ])(
    'rejects unsafe registry input %s before making a request',
    async registry => {
      await configure();
      expect(() =>
        NpmSearchBulkQueryLocalSchema.parse({
          queries: [{ packageName: 'fixture', registry }],
        })
      ).toThrow();
      expect(receipts).toEqual([]);
    }
  );
});
