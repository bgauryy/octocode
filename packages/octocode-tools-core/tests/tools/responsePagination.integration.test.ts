import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';

describe('whole-response pagination', () => {
  let root = '';

  beforeAll(async () => {
    await mkdir(join(process.cwd(), '.octocode', 'tmp'), { recursive: true });
    root = await mkdtemp(
      join(process.cwd(), '.octocode', 'tmp', 'ast-response-')
    );
    await Promise.all(
      Array.from({ length: 12 }, (_, index) =>
        writeFile(
          join(root, `fixture-${index}.ts`),
          `export const target${index} = ${index};\n`.repeat(8)
        )
      )
    );
  });

  afterAll(async () => rm(root, { recursive: true, force: true }));

  it('reconstructs exact Unicode content by executing every fetch continuation', async () => {
    const file = join(root, 'unicode.txt');
    await writeFile(file, '😀🧪'.repeat(80));
    const input = { queries: [{ path: file }] };
    const complete = await executeDirectTool('localFetch', input);
    const expected = complete.content
      .filter(item => item.type === 'text')
      .map(item => item.text)
      .join('');
    expect(expected).toContain('😀🧪');
    let next: Record<string, unknown> | undefined = {
      ...input,
      responseCharLength: 33,
    };
    const chunks: string[] = [];
    while (next) {
      const result = await executeDirectTool('localFetch', next);
      expect(result.isError).not.toBe(true);
      const page = (
        result.structuredContent as {
          responsePagination: {
            restart?: boolean;
            next?: { query: Record<string, unknown> };
          };
        }
      ).responsePagination;
      expect(page.restart).not.toBe(true);
      const text = result.content
        .filter(item => item.type === 'text')
        .map(item => item.text)
        .join('');
      chunks.push(
        Buffer.from(text.slice(text.indexOf('\n') + 1), 'utf8').toString('utf8')
      );
      next = page.next?.query;
      expect(chunks.length).toBeLessThan(expected.length);
    }
    expect(chunks.join('')).toBe(expected);
  });

  it('replays a structural directory continuation without a false restart', async () => {
    const first = await executeDirectTool('astSearch', {
      queries: [
        {
          operation: 'match',
          path: root,
          langType: 'typescript',
          pattern: 'const $NAME = $VALUE',
          resultView: 'content',
        },
      ],
      responseCharLength: 100,
    });
    const structured = first.structuredContent as {
      responsePagination?: { next?: { query: Record<string, unknown> } };
    };
    const continuation = structured.responsePagination?.next?.query;
    expect(continuation).toBeDefined();
    const replay = await executeDirectTool('astSearch', continuation!);
    const replayPagination = (
      replay.structuredContent as {
        responsePagination?: { restart?: boolean; snapshot?: string };
      }
    ).responsePagination;
    expect(replay.isError).not.toBe(true);
    expect(replayPagination?.restart).not.toBe(true);
    expect(replayPagination?.snapshot).toBe(
      structured.responsePagination?.next?.query.responseSnapshot as string
    );
  });
});
