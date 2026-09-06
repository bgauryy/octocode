import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { searchContentStructural } from '../../../src/tools/local_ripgrep/structuralSearch.js';
import { cleanJsonObject } from '../../../src/responses.js';

// Real-engine test (no mocks): the natural function pattern an agent writes,
// `function $NAME($$$ARGS) { $$$BODY }`, omits this fixture's return type.
// The fallback remains useful, but the executed query must survive public
// serialization so the caller can distinguish it from the requested shape.
describe('searchContentStructural return-type auto-relax (real engine)', () => {
  let dir: string;
  let filePath: string;

  beforeAll(async () => {
    // Keep the fixture inside cwd so validateToolPath's workspace check passes.
    dir = await mkdtemp(join(process.cwd(), 'tmp-structural-relax-'));
    filePath = join(dir, 'typed.ts');
    await writeFile(
      filePath,
      'export function foo(): number {\n  return 1;\n}\n',
      'utf8'
    );
  });

  afterAll(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  function makeQuery(overrides: Record<string, unknown> = {}) {
    return {
      id: 'structural-relax',
      researchGoal: 'unit-test',
      reasoning: 'validate return-type auto-relax',
      path: filePath,
      mode: 'structural' as const,
      pattern: 'function $NAME($$$ARGS) { $$$BODY }',
      maxFiles: 10,
      ...overrides,
    };
  }

  it('matches a return-typed function from the bare function pattern', async () => {
    const result = await searchContentStructural(makeQuery());

    expect(result.status).not.toBe('error');
    expect(result.files).toHaveLength(1);
    expect(cleanJsonObject(result)).toMatchObject({
      diagnostics: [
        expect.objectContaining({
          code: 'structural.query.rewritten',
          message: expect.stringContaining(
            '"function $NAME($$$ARGS): $R { $$$BODY }"'
          ),
        }),
      ],
    });
  });
});
