import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';

type RoutingCase = {
  id: string;
  heldOut: false;
  capability: string;
  unified: { tool: string; query: Record<string, unknown> };
  retired: { tool: string };
};

type RoutingFixture = {
  kpiContract: {
    primary: {
      name: string;
      direction: 'lower';
      baseline: number;
      target: number;
    };
    leading: {
      toolCalls: { baseline: number; max: number };
      schemaBytes: { baseline: number; max: number };
      promptBytes: { baseline: number; max: number };
    };
    guardrails: { correctnessCases: number };
    budget: { cases: number; trials: 1 };
    decisionRule: string;
  };
  retiredSurface: {
    tools: Array<{
      name: string;
      title: string;
      description: string;
      schemaBytes: number;
      capabilities: string[];
    }>;
  };
  cases: RoutingCase[];
};

const fixture = JSON.parse(
  readFileSync(
    new URL('../fixtures/unified-routing-held-out.json', import.meta.url),
    'utf8'
  )
) as RoutingFixture;

describe('public routing schema regressions', () => {
  it.each(fixture.cases)('$id stays correct on both surfaces', testCase => {
    expect(testCase.heldOut).toBe(false);

    const unified = DIRECT_TOOL_DEFINITIONS.find(
      tool => tool.name === testCase.unified.tool
    );
    expect(
      unified,
      `missing unified route ${testCase.unified.tool}`
    ).toBeDefined();
    expect(
      unified!.inputSchema.safeParse({ queries: [testCase.unified.query] })
        .success
    ).toBe(true);

    const retired = fixture.retiredSurface.tools.find(
      tool => tool.name === testCase.retired.tool
    );
    expect(
      retired,
      `missing retired route ${testCase.retired.tool}`
    ).toBeDefined();
    expect(retired!.capabilities).toContain(testCase.capability);
  });
});

describe('routing serialization budget', () => {
  const unifiedNames = [
    ...new Set(fixture.cases.map(testCase => testCase.unified.tool)),
  ];
  const unifiedTools = DIRECT_TOOL_DEFINITIONS.filter(tool =>
    unifiedNames.includes(tool.name)
  );

  it('does not reintroduce any retired public runtime alias', () => {
    const publicNames = DIRECT_TOOL_DEFINITIONS.map(tool => tool.name);
    const retiredNames = fixture.retiredSurface.tools.map(tool => tool.name);
    // ghSearchCode kept its canonical name; the remaining names are retired aliases.
    for (const name of retiredNames.filter(name => name !== 'ghSearchCode')) {
      expect(publicNames).not.toContain(name);
    }
  });

  it('accepts every fixture and stays within the serialized catalog budget', () => {
    const unifiedCorrect = fixture.cases.filter(testCase => {
      const tool = unifiedTools.find(
        item => item.name === testCase.unified.tool
      );
      return (
        tool?.inputSchema.safeParse({ queries: [testCase.unified.query] })
          .success === true
      );
    }).length;
    const retiredCorrect = fixture.cases.filter(testCase => {
      const tool = fixture.retiredSurface.tools.find(
        item => item.name === testCase.retired.tool
      );
      return tool?.capabilities.includes(testCase.capability) === true;
    }).length;

    const unifiedCalls = fixture.cases.length;
    const retiredCalls = fixture.cases.length;
    const unifiedSchemaBytes = unifiedTools.reduce(
      (sum, tool) =>
        sum +
        Buffer.byteLength(
          JSON.stringify(z.toJSONSchema(tool.inputSchema, { io: 'input' })),
          'utf8'
        ),
      0
    );
    const retiredSchemaBytes = fixture.retiredSurface.tools.reduce(
      (sum, tool) => sum + tool.schemaBytes,
      0
    );
    const unifiedPromptBytes = unifiedTools.reduce(
      (sum, tool) =>
        sum +
        Buffer.byteLength(
          `${tool.name}\n${tool.title}\n${tool.description}`,
          'utf8'
        ),
      0
    );
    const retiredPromptBytes = fixture.retiredSurface.tools.reduce(
      (sum, tool) =>
        sum +
        Buffer.byteLength(
          `${tool.name}\n${tool.title}\n${tool.description}`,
          'utf8'
        ),
      0
    );
    const unifiedTotalBytes = unifiedSchemaBytes + unifiedPromptBytes;
    const retiredTotalBytes = retiredSchemaBytes + retiredPromptBytes;
    const contract = fixture.kpiContract;

    expect(contract.budget).toEqual({ cases: fixture.cases.length, trials: 1 });
    expect(contract.guardrails.correctnessCases).toBe(fixture.cases.length);
    expect(contract.primary).toMatchObject({
      name: 'totalRoutingBytes',
      direction: 'lower',
      baseline: retiredTotalBytes,
    });
    expect(contract.leading.toolCalls.baseline).toBe(retiredCalls);
    expect(contract.leading.schemaBytes.baseline).toBe(retiredSchemaBytes);
    expect(contract.leading.promptBytes.baseline).toBe(retiredPromptBytes);

    expect(unifiedCorrect).toBe(contract.guardrails.correctnessCases);
    expect(retiredCorrect).toBe(contract.guardrails.correctnessCases);
    expect(unifiedCorrect).toBe(retiredCorrect);
    expect(unifiedCalls).toBeLessThanOrEqual(contract.leading.toolCalls.max);
    expect(unifiedSchemaBytes).toBeLessThanOrEqual(
      contract.leading.schemaBytes.max
    );
    expect(unifiedPromptBytes).toBeLessThanOrEqual(
      contract.leading.promptBytes.max
    );
    expect(unifiedTotalBytes).toBeLessThanOrEqual(contract.primary.target);
    expect(contract.decisionRule).toContain('correctness unchanged');
  });
});
