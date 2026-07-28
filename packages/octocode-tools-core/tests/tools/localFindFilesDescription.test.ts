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

  it('ensures the tool-level description states generated/vendor dirs are pruned by default', () => {
    const patched =
      getPatchedToolMetadata(completeMetadata).tools.localFindFiles.description;
    expect(patched).toMatch(/prunes common generated\/vendor dirs by default/);
    expect(patched).not.toMatch(/Nothing is excluded by default/i);
    expect(DESCRIPTIONS.localFindFiles).toMatch(
      /prunes common generated\/vendor dirs by default/
    );
  });

  it('field schema describe matches runtime excludeDir defaults', () => {
    const json = z.toJSONSchema(LocalFindFilesQuerySchema, { io: 'input' }) as {
      properties?: { excludeDir?: { description?: string } };
    };
    const fieldDesc = json.properties?.excludeDir?.description ?? '';
    expect(fieldDesc).toMatch(/pruned by default/i);
    expect(fieldDesc).toMatch(/pass \[\] to prune nothing/i);
    expect(fieldDesc).not.toMatch(/NOTHING is excluded by default/i);
  });
});
