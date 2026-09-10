// Core owns context wording; this adapter supplies runtime availability.
import { buildCliToolContext } from '@octocodeai/octocode-core/mcp';
import { getGrammarCapabilities } from '@octocodeai/octocode-tools-core';
import { getToolAvailability } from '@octocodeai/octocode-tools-core/schema';
import { TOOL_DEFINITIONS, getToolEnableInstruction } from './registry.js';

export async function getToolsContextString(
  options: { full?: boolean; minimal?: boolean } = {}
): Promise<string> {
  const availability = Object.fromEntries(
    TOOL_DEFINITIONS.map(({ name }) => {
      const result = getToolAvailability(name);
      return [
        name,
        {
          enabled: result.enabled,
          hint: getToolEnableInstruction(name) ?? result.envVar,
        },
      ];
    })
  );
  return buildCliToolContext({
    ...options,
    availability,
    ...(availability.astSearch?.enabled
      ? { grammarCapabilities: getGrammarCapabilities() }
      : {}),
  });
}

export async function printToolsContext(
  options: { full?: boolean; minimal?: boolean } = {}
): Promise<void> {
  console.log(await getToolsContextString(options));
}
