import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';

// Provenance contract: descriptions are served from @octocodeai/octocode-core
// VERBATIM (the temporary descriptionOverrides patch layer was removed once
// core's prose was fixed at source). These tests keep both the truth of the
// excludeDir prose AND the no-patching contract.
describe('localFindFiles excludeDir description contract', () => {
  it('core ships the pruned-by-default truth directly (no patch layer needed)', () => {
    const core = completeMetadata.tools.localFindFiles.description;
    expect(core).toMatch(/prunes common generated\/vendor dirs by default/);
    expect(core).not.toMatch(/Nothing is excluded by default/i);
  });

  it('served DESCRIPTIONS are byte-identical to core (no interface-level patching)', () => {
    for (const [name, spec] of Object.entries(completeMetadata.tools)) {
      expect(DESCRIPTIONS[name]).toBe(spec.description);
    }
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
