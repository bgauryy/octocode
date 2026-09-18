import { expect, it } from 'vitest';
import { paginateContentWindow } from '../../src/utils/file/contentPagination.js';
import { expectExecutableNext } from '../helpers/executableNext.js';

it.each(['localFetch', 'ghGetFileContent'] as const)('preserves selectors and caller metadata while stripping goal and undefined fields from %s continuations', async tool => {
  const query = {
    ...(tool === 'ghGetFileContent' ? { owner: 'octo', repo: 'fixture', branch: 'main' } : {}),
    path: tool === 'localFetch' ? '/fixture/source.ts' : 'source.ts',
    matchString: 'needle', contextLines: 2, minify: 'none' as const,
    chunkType: 'bytes' as const, limit: 7,
    startLine: undefined, endLine: undefined,
    goal: 'Read the first page', reasoning: 'Continue the selected content window', debug: true,
  };
  const page = await paginateContentWindow('needle first\nneedle second\n', query, tool);
  expectExecutableNext(page.next);
  const next = page.next!.continue!.query;
  expect(next).toMatchObject({ path: query.path, matchString: 'needle', contextLines: 2, minify: 'none', chunkType: 'bytes', limit: 7, offset: 7 });
  expect(next).toMatchObject({
    reasoning: 'Continue the selected content window',
    debug: true,
  });
  for (const key of ['goal', 'startLine', 'endLine']) expect(next).not.toHaveProperty(key);
});
