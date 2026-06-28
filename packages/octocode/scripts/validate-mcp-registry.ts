#!/usr/bin/env npx tsx

import { getOctocodeServerConfig } from '../src/utils/mcp-config.js';

const EXPECTED_OCTOCODE_CONFIG = {
  command: 'npx',
  args: ['-y', '@octocodeai/mcp@latest'],
} as const;

const jsonOutput = process.argv.includes('--json');

function validateOctocodeConfig(): { valid: boolean; errors: string[] } {
  const config = getOctocodeServerConfig('npx');
  const errors: string[] = [];

  if (config.command !== EXPECTED_OCTOCODE_CONFIG.command) {
    errors.push(
      `Expected command ${EXPECTED_OCTOCODE_CONFIG.command}, got ${config.command}`
    );
  }

  if (
    config.args.length !== EXPECTED_OCTOCODE_CONFIG.args.length ||
    config.args.some((arg, index) => arg !== EXPECTED_OCTOCODE_CONFIG.args[index])
  ) {
    errors.push(
      `Expected args ${JSON.stringify(EXPECTED_OCTOCODE_CONFIG.args)}, got ${JSON.stringify(config.args)}`
    );
  }

  return { valid: errors.length === 0, errors };
}

const result = validateOctocodeConfig();

if (jsonOutput) {
  console.log(
    JSON.stringify({
      valid: result.valid,
      config: { octocode: EXPECTED_OCTOCODE_CONFIG },
      errors: result.errors,
    })
  );
} else {
  console.log('Octocode MCP config validation');
  console.log(JSON.stringify({ octocode: EXPECTED_OCTOCODE_CONFIG }, null, 2));
  if (result.valid) {
    console.log('✓ Octocode MCP config is valid');
  } else {
    for (const error of result.errors) {
      console.error(`✗ ${error}`);
    }
  }
}

if (!result.valid) {
  process.exitCode = 1;
}
