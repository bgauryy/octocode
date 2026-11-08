import { describe, expect, it } from 'vitest';
import { MCP_INSTRUCTION_CORE_CHARS, MCP_INSTRUCTION_PARTS, mcpInstructions } from '@octocodeai/octocode-core/mcp';
import { instructionFixtures } from '../scripts/tool-contract/instruction-fixtures.ts';

describe('instruction fixtures', () => {
  const { coreChars, sets: fixtures } = instructionFixtures();

  it('cover all on, none optional, local only, GitHub only and each optional tool', () => {
    const names = fixtures.map(fixture => fixture.name);
    expect(names.slice(0, 4)).toEqual(['all on', 'no optional tool', 'local only', 'github only']);
    expect(fixtures).toHaveLength(4 + MCP_INSTRUCTION_PARTS.blocks.length);
    expect(new Set(fixtures.map(fixture => fixture.text)).size).toBe(fixtures.length);
  });

  it("carry core's own ceiling and stay within it", () => {
    expect(coreChars).toBe(MCP_INSTRUCTION_CORE_CHARS);
    for (const fixture of fixtures) expect(fixture.text.length, fixture.name).toBeLessThanOrEqual(coreChars);
  });

  it("hold core's composition for each enabled set", () => {
    for (const fixture of fixtures) expect(fixture.text).toBe(mcpInstructions(fixture.enabled));
  });
});
