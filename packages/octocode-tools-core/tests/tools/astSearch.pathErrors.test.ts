import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import {
  afterAll,
  afterEach,
  beforeAll,
  describe,
  expect,
  it,
  vi,
} from 'vitest';
import { executeDirectTool } from '../../src/tools/directToolCatalog.exec.js';
import { contextUtils } from '../../src/utils/contextUtils.js';

describe('astSearch structural path errors', () => {
  let root: string;
  beforeAll(async () => {
    const temporary = join(process.cwd(), '.octocode', 'tmp');
    await mkdir(temporary, { recursive: true });
    root = await mkdtemp(join(temporary, 'ast-path-errors-'));
  });
  afterAll(async () => rm(root, { recursive: true, force: true }));
  afterEach(() => vi.restoreAllMocks());

  it('keeps runtime failures free of syntax repair after public shaping', async () => {
    vi.spyOn(contextUtils, 'structuralSearchFiles').mockRejectedValue(
      new Error('native worker unavailable')
    );
    const result = await executeDirectTool('astSearch', {
      queries: [
        {
          operation: 'match',
          path: root,
          pattern: 'target($X)',
          langType: 'ts',
        },
      ],
    });
    const row = (
      result.structuredContent as {
        results: { status: string; data: Record<string, unknown> }[];
      }
    ).results[0]!;
    expect(row).toMatchObject({
      status: 'error',
      data: {
        errorCode: 'toolExecutionFailed',
        error: 'native worker unavailable',
      },
    });
    expect(JSON.stringify(row)).not.toMatch(
      /Invalid structural|\$\$\$BODY|--scheme|Broaden the syntax/
    );
  });

  it.each([undefined, 'ts'])(
    'classifies a missing path with langType:%s as file access',
    async langType => {
      const result = await executeDirectTool('astSearch', {
        queries: [
          {
            operation: 'match',
            path: join(root, 'missing'),
            pattern: 'target($X)',
            ...(langType ? { langType } : {}),
          },
        ],
      });
      const row = (
        result.structuredContent as {
          results: { status: string; data: Record<string, unknown> }[];
        }
      ).results[0]!;
      expect(row).toMatchObject({
        status: 'error',
        data: { errorCode: 'fileAccessFailed' },
      });
      expect(JSON.stringify(row)).not.toMatch(
        /Invalid structural|\$\$\$BODY|localSearch --scheme|Broaden the syntax/
      );
    }
  );
});
