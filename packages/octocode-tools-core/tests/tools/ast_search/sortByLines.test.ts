import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { findFiles } from '../../../src/tools/ast_search/filesystem/files.js';
import { AstSearchQuerySchema } from '@octocodeai/octocode-core/schema';

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(
    roots.splice(0).map(path => rm(path, { recursive: true, force: true }))
  );
});

describe('sort:lines for astSearch operation:files', () => {
  it('sorts files descending by line count and populates lineCount in detail:full output', async () => {
    const path = await mkdtemp(join(process.cwd(), '.tmp-sort-lines-'));
    roots.push(path);

    // Create files with different line counts
    await writeFile(join(path, 'small.ts'), 'const a = 1;\n');
    await writeFile(
      join(path, 'large.ts'),
      Array.from({ length: 10 }, (_, i) => `const line${i} = ${i};`).join('\n') + '\n'
    );
    await writeFile(
      join(path, 'medium.ts'),
      Array.from({ length: 5 }, (_, i) => `const m${i} = ${i};`).join('\n') + '\n'
    );

    const result = await findFiles(
      AstSearchQuerySchema.parse({
        operation: 'files',
        path,
        entryType: 'f',
        sort: 'lines',
        detail: 'full',
      }) as never
    );

    expect(result.status).not.toBe('error');
    const files = result.files.filter(f => f.type !== 'directory');

    // Should be sorted descending by line count
    expect(files[0].path).toContain('large.ts');
    expect(files[1].path).toContain('medium.ts');
    expect(files[2].path).toContain('small.ts');

    // lineCount should be populated in detail:full mode
    expect(files[0].lineCount).toBe(11);
    expect(files[1].lineCount).toBe(6);
    expect(files[2].lineCount).toBe(2);
  });

  it('omits lineCount from output when detail is not full', async () => {
    const path = await mkdtemp(join(process.cwd(), '.tmp-sort-lines-nodetail-'));
    roots.push(path);
    await writeFile(join(path, 'file.ts'), 'const a = 1;\n');

    const result = await findFiles(
      AstSearchQuerySchema.parse({
        operation: 'files',
        path,
        entryType: 'f',
        sort: 'lines',
        // no detail: 'full' — lineCount should not appear in output
      }) as never
    );

    expect(result.status).not.toBe('error');
    const files = result.files.filter(f => f.type !== 'directory');
    expect(files.length).toBeGreaterThan(0);
    // lineCount must not leak into non-detail output
    expect(files[0].lineCount).toBeUndefined();
  });

  it('sort:lines is a valid schema enum value', () => {
    const result = AstSearchQuerySchema.safeParse({
      operation: 'files',
      path: '/any',
      sort: 'lines',
    });
    expect(result.success).toBe(true);
  });

  it('sort:lines continuation query remains executable', async () => {
    const path = await mkdtemp(join(process.cwd(), '.tmp-sort-lines-cont-'));
    roots.push(path);
    await Promise.all(
      ['a.ts', 'b.ts', 'c.ts'].map((name, i) =>
        writeFile(
          join(path, name),
          Array.from({ length: i + 1 }, () => 'const x = 1;').join('\n') + '\n'
        )
      )
    );

    let query = AstSearchQuerySchema.parse({
      operation: 'files',
      path,
      entryType: 'f',
      sort: 'lines',
      detail: 'full',
      pageSize: 1,
    });
    const paths: string[] = [];
    for (;;) {
      const result = await findFiles(query as never);
      expect(result.status).not.toBe('error');
      paths.push(...result.files.map(f => f.path));
      const next = result.next?.nextPage;
      if (!next) break;
      // Continuation query must preserve sort:lines
      expect(next.query).toMatchObject({ sort: 'lines', pageSize: 1 });
      query = AstSearchQuerySchema.parse(next.query);
    }
    // Should have collected all 3 + parent directory entry
    expect(paths.some(p => p.endsWith('c.ts'))).toBe(true);
    expect(paths.some(p => p.endsWith('b.ts'))).toBe(true);
    expect(paths.some(p => p.endsWith('a.ts'))).toBe(true);
  });
});
