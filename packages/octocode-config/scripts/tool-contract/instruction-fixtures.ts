// Core's own composed instructions for chosen enabled-tool sets: the expected
// text of native's composer, never copied by hand. Native's test composes the
// presentation parts for each set and compares byte for byte.
import { MCP_INSTRUCTION_CORE_CHARS, MCP_INSTRUCTION_PARTS, mcpInstructions } from '@octocodeai/octocode-core/mcp';
import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/octocode-core/schema';

export interface InstructionFixture {
  readonly name: string;
  readonly enabled: readonly string[];
  /** Settings the set holds (path → value); a settings block is served for each match. */
  readonly settings: Readonly<Record<string, string>>;
  readonly text: string;
}

export interface InstructionFixtures {
  /** The host's cut: every enabled set's text, all on included, fits it. */
  readonly coreChars: number;
  readonly sets: readonly InstructionFixture[];
}

/**
 * One fixture per enabled-tool set: all on, none optional, local only, GitHub only, and each
 * optional tool alone added to the local set. The local set, not the plain one, keeps each such set
 * distinct from all on even when one tool is the only optional one. Then all on with every settings
 * block served: the longest text a session can get.
 */
export function instructionFixtures(): InstructionFixtures {
  const tools = DIRECT_TOOL_DEFINITIONS.map(definition => definition.name);
  const optional = new Set<string>(MCP_INSTRUCTION_PARTS.blocks.map(block => block.tool));
  const plain = tools.filter(tool => !optional.has(tool));
  const local = plain.filter(tool => /^(local|lsp|ast|structure)/.test(tool));
  const everySetting = Object.fromEntries(MCP_INSTRUCTION_PARTS.settings.map(({ setting, value }) => [setting, value]));
  const sets: Array<[string, string[], Record<string, string>]> = [
    ['all on', tools, {}],
    ['no optional tool', plain, {}],
    ['local only', local, {}],
    ['github only', plain.filter(tool => tool.startsWith('gh')), {}],
    ...[...optional].map((tool): [string, string[], Record<string, string>] => [
      `${tool} added to the local set`,
      [...local, tool],
      {},
    ]),
    ['all on, every settings block', tools, everySetting],
  ];
  return {
    coreChars: MCP_INSTRUCTION_CORE_CHARS,
    sets: sets.map(([name, enabled, settings]) => ({
      name,
      enabled,
      settings,
      text: mcpInstructions(enabled, settings),
    })),
  };
}
