import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import {
  getSupportedSignatureExtensions,
  getSupportedStructuralExtensions,
} from '@octocodeai/octocode-engine';
import { grammarFixtures } from '../fixtures/grammarFixtures.js';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { readFileEntry } from '../../src/tools/github_fetch_content/finalizer/entryParsers.js';
import { FileContentBulkQueryLocalSchema } from '../../src/tools/github_fetch_content/scheme.js';
import type { FileContentExecutionQuery } from '../../src/tools/github_fetch_content/types.js';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import { findDirectToolDefinition } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';

const fixture = vi.hoisted(() => ({ source: '' }));
vi.mock('../../src/github/fileContentRaw/cache.js', () => ({
  fetchCachedRawGitHubFileContent: vi.fn(async () => ({
    auth: 'fixture',
    rawResult: { data: { rawContent: fixture.source, branch: 'main' } },
  })),
}));

const signatureExtensions = new Set(getSupportedSignatureExtensions());
const cases = getSupportedStructuralExtensions().map(extension => {
  const source = grammarFixtures.find(item =>
    item.extensions.includes(extension)
  )?.source;
  if (!source) throw new Error(`Missing grammar fixture: ${extension}`);
  return { extension, source };
});
const modes = ['none', 'standard', 'symbols'] as const;
const windows = modes.flatMap(mode => [
  { mode, fullContent: false },
  { mode, fullContent: true },
]);
type Mode = (typeof modes)[number];
type Continuation = { tool: string; query: Record<string, unknown> };
type FileView = {
  content: string;
  contentView?: string;
  next?: { continueChars?: Continuation };
};

async function local(query: Record<string, unknown>): Promise<FileView> {
  const schema = findDirectToolDefinition('localGetFileContent')!.schema;
  expect(schema.safeParse(query).success, JSON.stringify(query)).toBe(true);
  const out = await executeDirectTool('localGetFileContent', {
    queries: [query],
  });
  const row = (
    out.structuredContent as {
      results: Array<{ status?: string; data: FileView }>;
    }
  ).results[0]!;
  expect(row.status, JSON.stringify(row)).not.toBe('error');
  return row.data;
}

async function github(query: Record<string, unknown>): Promise<FileView> {
  expect(
    FileContentBulkQueryLocalSchema.safeParse({ queries: [query] }).success,
    JSON.stringify(query)
  ).toBe(true);
  const out = await fetchGitHubFileContentAPI({
    ...query,
    noTimestamp: true,
  } as FileContentExecutionQuery);
  if (!('data' in out) || !out.data) throw new Error(JSON.stringify(out));
  return readFileEntry(out.data as Record<string, unknown>, query);
}

async function assertWindows(
  run: (query: Record<string, unknown>) => Promise<FileView>,
  tool: string,
  base: Record<string, unknown>,
  extension: string,
  source: string,
  mode: Mode,
  fullContent: boolean
) {
  const whole = await run({ ...base, minify: mode, fullContent: true });
  const expectedMode =
    mode === 'symbols' && !signatureExtensions.has(extension)
      ? 'standard'
      : mode;
  expect(whole.contentView).toBe(expectedMode);
  expect(whole.content).toContain('target');
  if (mode === 'none') expect(whole.content).toBe(source);
  if (mode === 'symbols' && signatureExtensions.has(extension)) {
    expect(whole.content).not.toContain('body_marker');
  } else if (source.includes('body_marker')) {
    expect(whole.content).toContain('body_marker');
  }
  let query: Record<string, unknown> = {
    ...base,
    minify: mode,
    charLength: 7,
    fullContent,
  };
  let joined = '';
  let pages = 0;
  const visited = new Set<number>();
  for (;;) {
    const page = await run(query);
    expect(page.content.length).toBeGreaterThan(0);
    expect(page.contentView).toBe(expectedMode);
    joined += page.content;
    pages++;
    expect(pages).toBeLessThan(100);
    const next = page.next?.continueChars;
    if (!next) break;
    expect(next.tool).toBe(tool);
    expect(next.query.charOffset).toBe(joined.length);
    expect(visited.has(next.query.charOffset as number)).toBe(false);
    visited.add(next.query.charOffset as number);
    query = next.query;
  }
  expect(joined).toBe(whole.content);
  // Semantic local windows may expand to a complete declaration. GitHub's
  // character windows must exercise actual continuations for every fixture.
  if (tool === 'ghGetFileContent') expect(pages).toBeGreaterThan(1);
}

describe('local/GitHub minification and executable pagination across every grammar', () => {
  let root = '';
  beforeAll(async () => {
    const parent = join(process.cwd(), '.octocode', 'tmp');
    await mkdir(parent, { recursive: true });
    root = await mkdtemp(join(parent, 'minification-matrix-'));
    for (const { extension, source } of cases)
      await writeFile(join(root, `fixture.${extension}`), source);
  });
  afterAll(async () => {
    if (root) await rm(root, { recursive: true, force: true });
  });

  it('covers all 42 structural extensions and 33 outline extensions', () => {
    expect(cases).toHaveLength(42);
    expect(signatureExtensions.size).toBe(33);
    expect(new Set(cases.map(item => item.extension)).size).toBe(42);
  });
  describe.each(cases)('.$extension', ({ extension, source }) => {
    it.each(windows)(
      'GitHub $mode (fullContent: $fullContent) preserves its view through executable continuations',
      async ({ mode, fullContent }) => {
        fixture.source = source;
        await assertWindows(
          github,
          'ghGetFileContent',
          {
            owner: 'octo',
            repo: 'fixture',
            path: `fixture.${extension}`,
            branch: 'main',
          },
          extension,
          source,
          mode,
          fullContent
        );
      }
    );
    it.each(windows)(
      'local $mode (fullContent: $fullContent) preserves its view through executable continuations',
      async ({ mode, fullContent }) => {
        await assertWindows(
          local,
          'localGetFileContent',
          { path: join(root, `fixture.${extension}`) },
          extension,
          source,
          mode,
          fullContent
        );
      }
    );
  });
});
