import { beforeEach, describe, expect, it } from 'vitest';
import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import {
  _resetDescriptionOverrideCache,
  getPatchedToolMetadata,
} from '../../src/tools/toolMetadata/descriptionOverrides.js';
import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';

describe('localFindFiles excludeDir description contract', () => {
  beforeEach(() => {
    _resetDescriptionOverrideCache();
  });

  it('rewrites stale core tool-level excludeDir default claim', () => {
    const raw = completeMetadata.tools.localFindFiles.description;
    expect(raw).toMatch(/Default excludeDir skips/);

    const patched =
      getPatchedToolMetadata(completeMetadata).tools.localFindFiles
        .description;
    expect(patched).not.toMatch(/Default excludeDir skips/);
    expect(patched).toMatch(/Nothing is excluded by default/);
    expect(DESCRIPTIONS.localFindFiles).toMatch(
      /Nothing is excluded by default/
    );
  });

  it('field schema describe still says NOTHING is excluded by default', () => {
    const json = z.toJSONSchema(LocalFindFilesQuerySchema, { io: 'input' }) as {
      properties?: { excludeDir?: { description?: string } };
    };
    const fieldDesc = json.properties?.excludeDir?.description ?? '';
    expect(fieldDesc).toMatch(/NOTHING is excluded by default/i);
  });
});
