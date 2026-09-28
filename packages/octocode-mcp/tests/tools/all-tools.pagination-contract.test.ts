import { describe, it, expect } from 'vitest';
import {
  DIRECT_TOOL_DEFINITIONS,
  formatDirectToolSchemaText,
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
  ghStructure: { controls: ['page', 'pageSize', 'metadataPage'] },
  ghGetFileContent: { controls: ['chunkType', 'offset', 'chunkSize'] },
  ghSearchHistory: { controls: ['page', 'pageSize'] },
  ghGetHistoryItem: {
    controls: [
      'filePage',
      'commentPage',
      'commitPage',
      'pageSize',
      'charOffset',
      'commentBodyOffset',
      'charLength',
    ],
  },
  artifactSearch: { controls: ['cursor', 'pageSize'] },
  ghCloneRepo: {
    controls: [],
    exemption: 'bounded clone/materialization operation',
  },
  localSearch: { controls: ['page', 'pageSize'] },
  structureSearch: { controls: ['page', 'pageSize'] },
  astSearch: { controls: ['page', 'pageSize'] },
  astTopology: {
    controls: ['page', 'pageSize', 'diagnosticPage', 'diagnosticPageSize'],
  },
  astRewrite: { controls: ['page', 'pageSize'] },
  localFetch: { controls: ['chunkType', 'offset', 'chunkSize'] },
  lspSearch: { controls: ['page', 'pageSize'] },
  clasify: {
    controls: [],
    exemption: 'bounded typed-judgment operation',
  },
};

const TOTAL_CAP_TOOLS = new Set(['structureSearch', 'astTopology']);

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

      it('declares real pagination controls or a bounded-operation exemption', () => {
        expect(
          contract.controls.length > 0 || contract.exemption,
          'missing pagination controls or exemption reason'
        ).toBeTruthy();
        for (const knob of contract.controls) {
          expect(schemaText, `missing knob "${knob}"`).toContain(`"${knob}"`);
        }
      });

      it('reserves limit for explicit total caps and chunkSize for content windows', () => {
        if (TOTAL_CAP_TOOLS.has(toolName)) {
          expect(schemaText).toContain('"limit"');
        } else {
          expect(schemaText).not.toContain('"limit"');
        }
        if (['localFetch', 'ghGetFileContent'].includes(toolName)) {
          expect(schemaText).toContain('"chunkSize"');
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
