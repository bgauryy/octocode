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

  it('ensures the tool-level description states the runtime excludeDir default', () => {
    const patched =
      getPatchedToolMetadata(completeMetadata).tools.localFindFiles.description;
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
