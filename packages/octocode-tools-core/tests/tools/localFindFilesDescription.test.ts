import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { PUBLIC_TOOL_DESCRIPTIONS } from '../../src/toolContract/descriptions.js';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';

// Ownership contract: descriptions are served directly from tools-core's local
// tool contract. These tests keep both the truth of the excludeDir prose and
// the single-local-owner contract.
describe('localSearch excludeDir description contract', () => {
  it('the local contract ships the pruned-by-default truth directly', () => {
    const description = PUBLIC_TOOL_DESCRIPTIONS.localSearch;
    expect(description).toMatch(
      /prune common generated and vendor directories by default/i
    );
    expect(description).not.toMatch(/Nothing is excluded by default/i);
  });

  it('served DESCRIPTIONS are byte-identical to the local contract', () => {
    for (const definition of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
      expect(PUBLIC_TOOL_DESCRIPTIONS[definition.name]).toBe(
        definition.description
      );
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
