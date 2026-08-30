import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { localCompleteMetadata } from '../../src/toolContract/metadata.js';
import { DESCRIPTIONS } from '../../src/tools/toolMetadata/descriptions.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';

// Ownership contract: descriptions are served directly from tools-core's local
// tool contract. These tests keep both the truth of the excludeDir prose and
// the single-local-owner contract.
describe('localFindFiles excludeDir description contract', () => {
  it('the local contract ships the pruned-by-default truth directly', () => {
    const description = localCompleteMetadata.tools.localFindFiles.description;
    expect(description).toMatch(
      /prunes common generated\/vendor dirs by default/i
    );
    expect(description).not.toMatch(/Nothing is excluded by default/i);
  });

  it('served DESCRIPTIONS are byte-identical to the local contract', () => {
    for (const [name, spec] of Object.entries(localCompleteMetadata.tools)) {
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
