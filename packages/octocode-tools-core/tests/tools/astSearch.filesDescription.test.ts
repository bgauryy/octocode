import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  AstSearchQuerySchema,
  PUBLIC_TOOL_DESCRIPTIONS,
} from '@octocodeai/octocode-core/schema';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-core/schema';

describe('local search and AST description contracts', () => {
  it('distinguishes lexical matching from structure and semantic identity', () => {
    expect(PUBLIC_TOOL_DESCRIPTIONS.localSearch).toMatch(
      /literal or regex matches/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.localSearch).toMatch(
      /astSearch for syntax or topology/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.astSearch).toMatch(
      /files.*tree.*symbols.*match.*topology/i
    );
    expect(PUBLIC_TOOL_DESCRIPTIONS.astSearch).toMatch(
      /lspSearch for semantic identity/i
    );
  });

  it('serves core descriptions directly for every public tool', () => {
    for (const definition of DIRECT_TOOL_DISCOVERY_DEFINITIONS) {
      expect(PUBLIC_TOOL_DESCRIPTIONS[definition.name]).toBe(
        definition.description
      );
    }
  });

  it('describes the actual files operation exclusion defaults', () => {
    const json = z.toJSONSchema(AstSearchQuerySchema, { io: 'input' }) as {
      anyOf?: Array<{
        properties?: {
          operation?: { const?: string };
          excludeDir?: { description?: string };
        };
      }>;
      oneOf?: Array<{
        properties?: {
          operation?: { const?: string };
          excludeDir?: { description?: string };
        };
      }>;
    };
    const files = (json.anyOf ?? json.oneOf ?? []).find(
      variant => variant.properties?.operation?.const === 'files'
    );
    const fieldDesc = files?.properties?.excludeDir?.description ?? '';
    expect(fieldDesc).toMatch(/pruned by default/i);
    expect(fieldDesc).toMatch(/pass \[\] to prune nothing/i);
    expect(fieldDesc).not.toMatch(/NOTHING is excluded by default/i);
  });
});
