import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';

type Fixture = {
  kpiContract: {
    primary: {
      name: string;
      direction: 'higher';
      baseline: number;
      target: number;
    };
    guardrails: {
      publicLocalTools: number;
      retiredNamesRejected: number;
      topologyAlgorithms: number;
      localSearchViews: number;
      localSearchRegexModes: number;
      fetchViews: number;
      exactContentPreserved: boolean;
      continuationQueriesExecutable: boolean;
    };
    decisionRule: string;
  };
  publicSurface: { tools: string[] };
  retiredSurface: {
    tools: Array<{
      name: string;
      replacedBy: string;
      expectedRejection: string;
      fixtureRole: 'negative-rejection';
    }>;
  };
  cases: Array<{
    id: string;
    heldOut: true;
    after: {
      tool: string;
      query?: Record<string, unknown>;
    };
  }>;
};

const fixture = JSON.parse(
  readFileSync(
    new URL('../fixtures/local-tool-removal-held-out.json', import.meta.url),
    'utf8'
  )
) as Fixture;

describe('held-out local tool removal contract', () => {
  it('publishes exactly the four local tools', () => {
    const actual = DIRECT_TOOL_DEFINITIONS.map(
      tool => tool.name
    ).filter(name => fixture.publicSurface.tools.includes(name));
    expect(actual).toEqual(fixture.publicSurface.tools);
    expect(new Set(actual).size).toBe(
      fixture.kpiContract.guardrails.publicLocalTools
    );
  });

  it('keeps retired names as negative rejection fixtures only', () => {
    const publicNames = DIRECT_TOOL_DEFINITIONS.map(
      tool => tool.name
    );
    const retiredNames = fixture.retiredSurface.tools.map(tool => tool.name);
    expect(
      fixture.retiredSurface.tools.every(
        tool => tool.fixtureRole === 'negative-rejection'
      )
    ).toBe(true);
    expect(publicNames).not.toEqual(expect.arrayContaining(retiredNames));
  });

  it.each(fixture.cases)('$id has a valid post-migration route', testCase => {
    expect(testCase.heldOut).toBe(true);
    const tool = DIRECT_TOOL_DEFINITIONS.find(
      item => item.name === testCase.after.tool
    );
    expect(tool).toBeDefined();
    if (testCase.after.query) {
      expect(tool!.schema.safeParse(testCase.after.query).success).toBe(true);
    }
  });

  it('freezes the behavior guardrails without a speedup claim', () => {
    expect(fixture.kpiContract.guardrails).toMatchObject({
      publicLocalTools: 4,
      retiredNamesRejected: 2,
      topologyAlgorithms: 6,
      localSearchViews: 9,
      localSearchRegexModes: 3,
      fetchViews: 3,
      exactContentPreserved: true,
      continuationQueriesExecutable: true,
    });
    expect(fixture.kpiContract.primary).toEqual({
      name: 'heldOutBehaviorCasesPassing',
      direction: 'higher',
      baseline: 0,
      target: 1,
    });
    expect(fixture.kpiContract.decisionRule).toContain(
      'not used to claim a speedup'
    );
  });
});
