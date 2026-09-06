import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { executeDirectTool } from '../../../src/tools/directToolCatalog.exec.js';
import { findDirectToolDefinition } from '../../../src/tools/directToolCatalog/toolCatalogDefinitions.js';

type View = {
  content: string;
  returnedChars: number;
  errorCode?: string;
  sourceChars?: number;
  totalLines?: number;
  terminalLimit?: boolean;
  next?: { continueChars?: { tool: string; query: Record<string, unknown> } };
};
async function run(
  query: Record<string, unknown>,
  allowError = false
): Promise<View> {
  expect(
    findDirectToolDefinition('localGetFileContent')!.schema.safeParse(query)
      .success
  ).toBe(true);
  const result = await executeDirectTool('localGetFileContent', {
    queries: [query],
  });
  const row = (
    result.structuredContent as {
      results: Array<{ status?: string; data: View }>;
    }
  ).results[0]!;
  if (!allowError) expect(row.status, JSON.stringify(row)).not.toBe('error');
  return row.data;
}

describe('local secret redaction precedes character pagination', () => {
  let root = '';
  let path = '';
  // Deliberately synthetic recognizable shape; never a real credential.
  const token = `ghp_${'A'.repeat(36)}`;
  beforeAll(async () => {
    const parent = join(process.cwd(), '.octocode', 'tmp');
    await mkdir(parent, { recursive: true });
    root = await mkdtemp(join(parent, 'secret-pagination-'));
    path = join(root, 'fixture.txt');
    await writeFile(path, `prefix ${token} suffix`);
  });
  afterAll(async () => {
    if (root) await rm(root, { recursive: true, force: true });
  });

  it.each(['none', 'standard'] as const)(
    '%s windows reconstruct the sanitized view',
    async minify => {
      const whole = await run({ path, minify, fullContent: true });
      expect(whole.content).not.toContain(token);
      expect(whole.content).toContain('REDACTED');
      let query: Record<string, unknown> = { path, minify, charLength: 7 };
      let joined = '';
      for (let page = 0; page < 50; page++) {
        const view = await run(query);
        joined += view.content;
        const next = view.next?.continueChars;
        if (!next) break;
        query = next.query;
      }
      expect(joined).not.toContain(token);
      expect(joined).toBe(whole.content);
    }
  );

  it('reports the actual returned character count after redaction', async () => {
    const view = await run({ path, minify: 'none', fullContent: true });
    expect(view.returnedChars).toBe(view.content.length);
  });

  it('fails closed for an oversized view while preserving executable bounded source reads', async () => {
    const largePath = join(root, 'large.txt');
    const source = `safe first line\n${'x'.repeat(999)}\n`.padEnd(
      10_000_001,
      'x'
    );
    await writeFile(largePath, source);
    const rejected = await run(
      { path: largePath, minify: 'none', charLength: 7 },
      true
    );
    expect(rejected.errorCode).toBe('contentSecurityLimit');
    expect(rejected.terminalLimit).toBe(true);
    expect(rejected.sourceChars).toBe(source.length);
    expect(rejected.totalLines).toBe(3);
    expect(rejected.content).toBeUndefined();
    const recovery = (
      rejected.next as Record<
        string,
        { tool: string; query: Record<string, unknown> }
      >
    ).readBoundedLines;
    expect(recovery.tool).toBe('localGetFileContent');
    expect(recovery.query).not.toHaveProperty('charOffset');
    const bounded = await run(recovery.query);
    expect(bounded.content).toContain('safe first line');
    expect(bounded.sourceChars).toBe(source.length);
    const matched = await run({
      path: largePath,
      matchString: 'safe first line',
      contextLines: 0,
    });
    expect(matched.content).toContain('safe first line');
  });

  it('retains the scanner exact byte boundary and exposes no looping recovery for an oversized single line', async () => {
    const boundaryPath = join(root, 'boundary.txt');
    await writeFile(boundaryPath, 'safe value'.padEnd(10_000_000, ' '));
    const atLimit = await run({
      path: boundaryPath,
      minify: 'none',
      charLength: 7,
    });
    expect(atLimit.errorCode).not.toBe('contentSecurityLimit');
    expect(atLimit.content).toContain('safe');
    await writeFile(boundaryPath, 'x'.repeat(10_000_001));
    const overLimit = await run(
      { path: boundaryPath, minify: 'none', charLength: 7 },
      true
    );
    expect(overLimit.errorCode).toBe('contentSecurityLimit');
    expect(overLimit.next).toBeUndefined();
  });
});
