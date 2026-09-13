import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { LocalSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';

type SearchRow = {
  status?: string;
  data: {
    error?: string;
    errorCode?: string;
    hints?: string[];
    stats?: { totalOccurrences?: number };
    next?: Record<
      string,
      {
        tool: string;
        query: Record<string, unknown>;
        why?: string;
      }
    >;
  };
};

async function search(query: Record<string, unknown>): Promise<SearchRow> {
  const result = await executeDirectTool('localSearch', { queries: [query] });
  return (result.structuredContent as { results: SearchRow[] }).results[0]!;
}

describe('localSearch regex error recovery', () => {
  let root = '';
  beforeAll(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'regex-repair-')
    );
    await writeFile(join(root, 'sample.py'), 'def invoke():\n    foofoo\n');
    await writeFile(join(root, 'ignored.py'), 'def invoke():\n    foofoo\n');
  });
  afterAll(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it.each([
    ['rust', 'def invoke(', 'literal', 'invalidRegex'],
    ['pcre2', 'def invoke(', 'literal', 'invalidRegex'],
    ['rust', '(?<=def )invoke', 'pcre2', 'unsupportedRegex'],
    ['rust', '(foo)\\1', 'pcre2', 'unsupportedRegex'],
  ] as const)(
    'repairs %s %s without silently changing matcher semantics',
    async (regex, searchText, repairedRegex, errorCode) => {
      const query = {
        path: root,
        searchText,
        regex,
        include: ['*.py'],
        exclude: ['ignored.py'],
        maxDepth: 1,
        hidden: true,
        noIgnore: true,
        caseMode: 'sensitive',
        resultView: 'countMatches',
        pageSize: 7,
        reverse: true,
      };
      const row = await search(query);
      expect(row.status).toBe('error');
      expect(row.data.errorCode).toBe(errorCode);
      expect(row.data.stats).toBeUndefined();
      expect(row.data.hints?.join(' ')).toContain(`regex:"${repairedRegex}"`);
      const repair = row.data.next?.repair;
      expect(repair?.tool).toBe('localSearch');
      expect(repair?.why).toContain('new search');
      expect(repair?.query).toMatchObject({ ...query, regex: repairedRegex });
      expect(LocalSearchQuerySchema.safeParse(repair?.query).success).toBe(
        true
      );
      const recovered = await search(repair!.query);
      expect(recovered.status).not.toBe('error');
      expect(recovered.data.stats?.totalOccurrences).toBe(1);
    }
  );

  it('does not interpret literal metacharacters as a broken regex', async () => {
    const row = await search({
      path: root,
      searchText: 'def invoke(',
      regex: 'literal',
    });
    expect(row.status).not.toBe('error');
    expect(row.data.stats?.totalOccurrences).toBe(2);
    expect(row.data.next?.repair).toBeUndefined();
  });

  it('does not mistake pattern text for an unsupported-feature diagnostic', async () => {
    const row = await search({
      path: root,
      searchText: 'backreferences are not supported(',
    });
    expect(row.data.errorCode).toBe('invalidRegex');
    expect(row.data.next?.repair?.query.regex).toBe('literal');
  });
});
