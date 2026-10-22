import { describe, it, expect } from 'vitest';
import {
  DIRECT_TOOL_DEFINITIONS,
  formatDirectToolSchemaText,
  getPublicToolCatalogWithAddons,
} from '@octocodeai/config/schema';

const LOSS_LANGUAGE: RegExp[] = [
  /may be truncated/i,
  // Flags a *claim* of silent loss, but not a negated reassurance such as
  // "never silently truncated" (which affirms the paginate-don't-truncate rule).
  /(?<!never )silently (?:dropped|truncated)/i,
  /first \d+ [^."]*only/i,
];

const TOOL_PAGINATION_CONTRACT: Record<
  string,
  { controls: string[]; exemption?: string }
> = {
  ghSearchRepo: { controls: ['page', 'pageSize'] },
  ghSearchCode: { controls: ['page', 'pageSize'] },
  ghStructure: {
    controls: [
      'page',
      'pageSize',
      'materializeOffset',
      'responseOffset',
      'responseLength',
    ],
  },
  ghGetFileContent: { controls: ['unit', 'offset', 'length'] },
  ghSearchHistory: { controls: ['page', 'pageSize'] },
  ghGetHistoryItem: {
    controls: ['page', 'pageSize', 'offset', 'length'],
  },
  artifactSearch: { controls: ['page', 'pageSize'] },
  ghCloneRepo: {
    controls: [],
    exemption: 'bounded clone/materialization operation',
  },
  localSearch: { controls: ['page', 'pageSize'] },
  structureSearch: { controls: ['page', 'pageSize'] },
  astSearch: { controls: ['page', 'pageSize'] },
  localFetch: { controls: ['unit', 'offset', 'length'] },
  lspSearch: { controls: ['page', 'pageSize'] },
  clasify: {
    controls: [],
    exemption: 'bounded typed-judgment operation',
  },
};

/** Scan ceilings that report partial results, by tool. */
const SCAN_CAPS: Record<string, string> = {
  structureSearch: 'maxEntries',
};

type JsonSchema = {
  $ref?: string;
  $defs?: Record<string, JsonSchema>;
  properties?: Record<string, JsonSchema>;
  oneOf?: JsonSchema[];
  anyOf?: JsonSchema[];
  allOf?: JsonSchema[];
};

/** Read root query or envelope fields across variants, without counting nested result fields. */
const declaredProperties = (schema?: JsonSchema): Set<string> => {
  const fields = new Set<string>();
  if (!schema) return fields;
  const visited = new Set<JsonSchema>();
  const visit = (node: JsonSchema): void => {
    if (visited.has(node)) return;
    visited.add(node);
    if (node.$ref?.startsWith('#/$defs/')) {
      const referenced = schema.$defs?.[node.$ref.slice('#/$defs/'.length)];
      if (referenced) visit(referenced);
    }
    for (const name of Object.keys(node.properties ?? {})) {
      fields.add(name);
    }
    for (const child of [
      ...(node.oneOf ?? []),
      ...(node.anyOf ?? []),
      ...(node.allOf ?? []),
    ]) {
      visit(child);
    }
  };
  visit(schema);
  return fields;
};

const catalog = new Map(
  getPublicToolCatalogWithAddons().tools.map(tool => [tool.name, tool])
);

describe('all-tools pagination contract', () => {
  it('covers every tool in the live catalog', () => {
    expect(Object.keys(TOOL_PAGINATION_CONTRACT).sort()).toEqual(
      DIRECT_TOOL_DEFINITIONS.map(tool => tool.name).sort()
    );
  });

  describe.each(Object.entries(TOOL_PAGINATION_CONTRACT))(
    '%s',
    (toolName, contract) => {
      const schemaText = formatDirectToolSchemaText(toolName);
      const tool = catalog.get(toolName);
      const declared = new Set([
        ...declaredProperties(tool?.querySchema as JsonSchema | undefined),
        ...declaredProperties(tool?.inputSchema as JsonSchema | undefined),
      ]);

      it('declares real pagination controls or a bounded-operation exemption', () => {
        expect(tool, `missing canonical schema for ${toolName}`).toBeDefined();
        expect(
          contract.controls.length > 0 || contract.exemption,
          'missing pagination controls or exemption reason'
        ).toBeTruthy();
        for (const knob of contract.controls) {
          expect(declared.has(knob), `missing schema property "${knob}"`).toBe(
            true
          );
        }
      });

      it('names scan caps explicitly and windows content with length', () => {
        expect(declared.has('limit')).toBe(false);
        const cap = SCAN_CAPS[toolName];
        if (cap) expect(declared.has(cap)).toBe(true);
        if (['localFetch', 'ghGetFileContent'].includes(toolName)) {
          expect(declared.has('length')).toBe(true);
        }
      });

      it('schema is free of silent-loss language (paginates, never truncates)', () => {
        for (const re of LOSS_LANGUAGE) {
          expect(schemaText, `loss-language matched ${re}`).not.toMatch(re);
        }
      });
    }
  );
});
