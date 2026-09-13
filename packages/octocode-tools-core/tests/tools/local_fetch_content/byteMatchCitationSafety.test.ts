import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { prepareDirectToolInput } from '@octocodeai/octocode-core/schema';
import { executeFetchContent } from '../../../src/tools/local_fetch_content/execution.js';
import { countLines } from '../../../src/utils/core/lines.js';
import { selectMatchingBytes } from '../../../src/utils/file/byteMatchSelection.js';

describe('localFetch byte-match citation safety', () => {
  let directory: string | undefined;
  afterEach(async () => {
    if (directory) await rm(directory, { recursive: true, force: true });
  });

  it('does not restore source anchors discarded by multiline secret redaction', async () => {
    directory = await mkdtemp(join(process.cwd(), '.tmp-byte-citations-'));
    const path = join(directory, 'source.txt');
    const key = [
      ['-----BEGIN', 'RSA PRIVATE KEY-----'].join(' '),
      'A'.repeat(64),
      'B'.repeat(64),
      ['-----END', 'RSA PRIVATE KEY-----'].join(' '),
    ].join('\n');
    const source = `before\n${key}\nneedle after key\n`;
    const sanitized = ContentSanitizer.sanitizeContent(source, path);
    expect(sanitized.hasSecrets).toBe(true);
    expect(sanitized.content).not.toContain(key);
    expect(countLines(sanitized.content)).toBeLessThan(countLines(source));
    await writeFile(path, source);

    const result = await executeFetchContent(
      prepareDirectToolInput('localFetch', {
        path,
        matchString: 'needle',
        contextBytes: 0,
        chunkType: 'bytes',
        limit: 100,
      }) as Parameters<typeof executeFetchContent>[0]
    );
    const row = (
      result.structuredContent as {
        results: Array<{ status?: string; data: Record<string, unknown> }>;
      }
    ).results[0]!;
    expect(row.status).not.toBe('error');
    expect(row.status).not.toBe('empty');
    expect(row.data.content).toBe('needle');
    expect(row.data.sourceLineRanges).toBeUndefined();
    const text = result.content
      .filter(item => item.type === 'text')
      .map(item => item.text)
      .join('');
    expect(text).not.toContain('content (source lines)');
    expect(text).toContain('content (copy-safe)');
  });

  it.each([0, 1])(
    'maps only source records across disjoint byte windows (context %s)',
    async contextBytes => {
      directory = await mkdtemp(join(process.cwd(), '.tmp-byte-citations-'));
      const path = join(directory, 'source.txt');
      const source = 'skip\nneedle\nnot blank\nneedle\n';
      await writeFile(path, source);
      const selected = selectMatchingBytes(
        source,
        'needle',
        contextBytes,
        false,
        true,
        path
      );
      if (contextBytes === 1) {
        expect(selected.warnings.join(' ')).toContain('synthetic separator');
      }
      const expected =
        contextBytes === 0 ? 'needle\nneedle' : '\nneedle\n\n\nneedle\n';
      let query: Record<string, unknown> = {
        path,
        matchString: 'needle',
        contextBytes,
        chunkType: 'bytes',
        limit: 3,
      };
      const chunks: string[] = [];
      let finished = false;
      for (let page = 0; page < 10; page++) {
        const result = await executeFetchContent(
          prepareDirectToolInput('localFetch', query) as Parameters<
            typeof executeFetchContent
          >[0]
        );
        const row = (
          result.structuredContent as {
            results: Array<{
              status?: string;
              data: {
                content: string;
                sourceLineRanges?: Array<{ start: number; end: number }>;
                warnings?: string[];
                next?: {
                  continue?: { tool: string; query: Record<string, unknown> };
                };
              };
            }>;
          }
        ).results[0]!;
        expect(row.status).not.toBe('error');
        expect(row.status).not.toBe('empty');
        chunks.push(row.data.content);
        const text = result.content
          .filter(item => item.type === 'text')
          .map(item => item.text)
          .join('');
        if (contextBytes === 1) {
          expect(row.data.sourceLineRanges).toBeUndefined();
          expect(text).not.toContain('content (source lines)');
        } else {
          expect(row.data.sourceLineRanges?.length).toBeGreaterThan(0);
          expect(text).toContain('content (source lines)');
        }
        const next = row.data.next?.continue;
        if (!next) {
          finished = true;
          break;
        }
        expect(next.tool).toBe('localFetch');
        expect(next.query.contextBytes).toBe(contextBytes);
        query = next.query;
      }
      expect(finished).toBe(true);
      expect(chunks.join('')).toBe(expected);
    }
  );
});
