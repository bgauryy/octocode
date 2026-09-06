import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GitHubProvider } from '../../src/providers/github/GitHubProvider.js';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import { FileContentQueryLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import type { FileEntry } from '../../src/tools/github_fetch_content/finalizer/types.js';
import { cleanup } from '../../src/serverConfig.js';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const fixture = vi.hoisted(() => ({ source: '' }));
vi.mock('../../src/providers/factory.js', async importOriginal => ({
  ...(await importOriginal<typeof import('../../src/providers/factory.js')>()),
  getProvider: () => new GitHubProvider(),
}));
// Only acquisition is stubbed: the native transform, real provider mappings,
// direct execution, envelope, and executable continuations remain in the path.
vi.mock('../../src/github/fileContentRaw/cache.js', () => ({
  fetchCachedRawGitHubFileContent: vi.fn(async () => ({
    auth: 'fixture',
    rawResult: { data: { rawContent: fixture.source, branch: 'main' } },
  })),
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: vi.fn(async () => ({
    rest: { repos: { listCommits: vi.fn(async () => ({ data: [] })) } },
  })),
}));

const base = {
  owner: 'octo', repo: 'fixture', path: 'source.ts', branch: 'main',
};
beforeEach(() => {
  vi.stubEnv('ENABLE_CLONE', 'false');
  cleanup();
});
afterEach(() => {
  vi.unstubAllEnvs();
  cleanup();
});

async function read(query: Record<string, unknown>): Promise<FileEntry> {
  expect(FileContentQueryLocalSchema.safeParse(query).success).toBe(true);
  const out = await executeDirectTool('ghGetFileContent', { queries: [query] });
  expect(
    (out.structuredContent as { results: unknown[] }).results,
    JSON.stringify(out)
  ).toHaveLength(1);
  const row = (out.structuredContent as {
    results: Array<{ status?: string; data: { files?: FileEntry[] } }>;
  }).results[0]!;
  expect(row.status, JSON.stringify(row)).not.toBe('error');
  expect(row.data.files, JSON.stringify(row)).toHaveLength(1);
  return row.data.files![0]!;
}

async function collect(initial: Record<string, unknown>) {
  let query = initial;
  let content = '';
  const pages: FileEntry[] = [];
  for (;;) {
    const page = await read(query);
    pages.push(page);
    content += page.content;
    expect(pages.length).toBeLessThan(200);
    const next = page.next?.continueChars;
    if (!next) break;
    expect(page.content.length).toBeGreaterThan(0);
    expect(next.tool).toBe('ghGetFileContent');
    expect(next.query.charOffset).toBe(content.length);
    expect(next.query.charLength).toBe(initial.charLength);
    for (const key of [
      'minify', 'matchString', 'matchStringIsRegex', 'fullContent',
    ]) {
      expect(next.query[key]).toEqual(initial[key]);
    }
    query = next.query;
  }
  return { content, pages };
}

describe('GitHub evidence through the complete public provider path', () => {
  it.each(['py', 'rs', 'html', 'vue', 'svelte'])(
    'preserves independent literal payloads through local and GitHub %s pages',
    async extension => {
      const payload = 'alpha  \n\n\nbeta\t\n';
      fixture.source = extension === 'py'
        ? `value = """${payload}"""\n`
        : extension === 'rs'
          ? `pub const VALUE: &str = r#"${payload}"#;\n`
          : `<script>const marker = "<!-- retained literal -->"; console.log(marker);</script>\n<pre>${payload}</pre>\n`;
      const remote = await collect({ ...base, path: `literal.${extension}`, minify: 'standard', charLength: 7 });
      expect(remote.content).toContain(payload);
      if (!['py', 'rs'].includes(extension)) expect(remote.content).toContain('<!-- retained literal -->');
      const parent = join(process.cwd(), '.octocode', 'tmp');
      await mkdir(parent, { recursive: true });
      const root = await mkdtemp(join(parent, 'literal-fidelity-'));
      try {
        const path = join(root, `literal.${extension}`);
        await writeFile(path, fixture.source);
        let query: Record<string, unknown> = { path, minify: 'standard', charLength: 7 };
        let content = '';
        for (let page = 0; ; page++) {
          expect(page).toBeLessThan(100);
          const out = await executeDirectTool('localGetFileContent', { queries: [query] });
          const row = (out.structuredContent as { results: Array<{ status?: string; data: FileEntry }> }).results[0]!;
          expect(row.status, JSON.stringify(row)).not.toBe('error');
          content += row.data.content;
          const next = row.data.next?.continueChars;
          if (!next) break;
          expect(next.query.charOffset).toBe(content.length);
          query = next.query;
        }
        expect(content).toContain(payload);
        expect(content).toBe(remote.content);
      } finally { await rm(root, { recursive: true, force: true }); }
    }
  );

  const anchors = [
    { line: '// only retries once', matchString: 'only retries once' },
    { line: 'import type { Anchor } from "./types";', matchString: 'import type' },
    { line: 'interface Anchor { value: number }', matchString: 'interface Anchor' },
    { line: 'type Anchor = { value: number };', matchString: 'type Anchor' },
    { line: '  // anchor\t  spacing  ', matchString: 'anchor\t  spacing' },
    { line: '// anchor 123', matchString: 'anchor\\s+\\d+', matchStringIsRegex: true },
  ];
  describe.each(anchors)('$line', ({ line, ...selector }) => {
    it.each([undefined, 'none', 'standard'] as const)(
      'preserves exact selected source with minify=%s across pages',
      async minify => {
        fixture.source = [line, 'const unrelated = 1;', line].join('\n');
        const initial = {
          ...base, ...selector, contextLines: 0, charLength: 7,
          ...(minify === undefined ? {} : { minify }),
        };
        const { content, pages } = await collect(initial);
        const whole = await read({ ...initial, charLength: 50000 });
        expect(content).toBe(whole.content);
        expect(pages.length).toBeGreaterThan(1);
        for (const page of pages) {
          expect(page.contentView).toBe('none');
          expect(page.matchedLines).toEqual([1, 3]);
        }
        expect(content.split(line)).toHaveLength(3);
        expect(content).not.toContain('unrelated');
      }
    );
  });

  it('preserves research declarations and bindings in a standard full-file view', async () => {
    fixture.source = [
      'import type { Input } from "./input";',
      'interface LocalShape { value: number }',
      'type LocalAlias = LocalShape;',
      'export function target(input: Input): LocalAlias {',
      '  const retainedBinding = input.value + 1;',
      '  return { value: retainedBinding };',
      '}',
    ].join('\n');
    const whole = await read({ ...base, fullContent: true, minify: 'standard' });
    expect(whole.contentView).toBe('standard');
    for (const anchor of [
      'import type', 'interface LocalShape', 'type LocalAlias', 'retainedBinding',
    ]) {
      expect(whole.content).toContain(anchor);
    }
    const { content } = await collect({ ...base, minify: 'standard', charLength: 7 });
    expect(content).toBe(whole.content);
  });

  it('redacts a matching secret before paging, retaining the surrounding anchor', async () => {
    const secret = 'ghp_' + 'AbCdEf0123456789'.repeat(2) + 'abcdef';
    fixture.source = `// anchor ${secret}\n// anchor visible`;
    const { content, pages } = await collect({
      ...base, matchString: 'anchor', contextLines: 0, charLength: 7,
    });
    expect(pages.length).toBeGreaterThan(1);
    expect(content).not.toContain(secret);
    expect(content).not.toContain(secret.slice(10, 25));
    expect(content).toContain('anchor');
    expect(content).toContain('REDACTED');
    expect(pages.every(page => page.contentView === 'none')).toBe(true);
  });

  it.each([false, true])('reconstructs symbol windows with fullContent=%s', async fullContent => {
    fixture.source = Array.from({ length: 20 }, (_, i) =>
      `export function target${i}(value: number): number {\n  const result = value + ${i};\n  return result;\n}`
    ).join('\n');
    const whole = await read({ ...base, minify: 'symbols', fullContent: true });
    const { content, pages } = await collect({
      ...base, minify: 'symbols', fullContent, charLength: 40,
    });
    expect(pages.length).toBeGreaterThan(1);
    expect(
      pages.every(page => page.contentView === 'symbols'),
      JSON.stringify(pages[0])
    ).toBe(true);
    expect(content).toBe(whole.content);
    expect(content).toContain('target19');
  });

  it.each(['none', 'standard', 'symbols'] as const)(
    'retains typed scanner limits and executable recovery in %s mode',
    async minify => {
      fixture.source = 'first line\n' + 'x'.repeat(10_000_001);
      const page = await read({ ...base, path: 'source.txt', minify, charLength: 7 });
      expect(page.errorCode).toBe('contentSecurityLimit');
      expect(page.terminalLimit).toBe(true);
      expect(page.partialReasons).toContain('security-selected-view-size-limit');
      expect(page.contentView).toBeUndefined();
      expect(page.next?.continueChars).toBeUndefined();
      const next = page.next?.readBoundedLines;
      expect(next?.tool).toBe('ghGetFileContent');
      const recovered = await read(next!.query);
      expect(recovered.content).toBe('first line');
      expect(recovered.errorCode).toBeUndefined();
    }
  );
});
