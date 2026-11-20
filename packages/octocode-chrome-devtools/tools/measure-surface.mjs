#!/usr/bin/env node
// Prints the MCP tool-schema cost per preset: what a client loads on connect
// (name + description + inputSchema, plus server instructions). Tokens are an
// ESTIMATE (characters / 4), not a tokenizer count.
// Usage: node tools/measure-surface.mjs [--json]   (needs `yarn build`)
import { resolve } from 'node:path';
import { connectStdio } from '../dist/runtime.js';
import { presets } from '../dist/runtime.js';

const bin = resolve(import.meta.dirname, '../bin/octocode-chrome-devtools.mjs');
const tokens = chars => Math.ceil(chars / 4);

export async function measure(preset) {
  const client = await connectStdio({
    command: process.execPath,
    args: [bin, '--mcp', '--preset', preset],
  });
  try {
    const { tools } = await client.listTools();
    const rows = tools.map(tool => ({
      name: tool.name,
      chars: JSON.stringify({
        name: tool.name,
        description: tool.description ?? '',
        inputSchema: tool.inputSchema,
      }).length,
      schemaChars: JSON.stringify(tool.inputSchema).length,
    }));
    const instructionChars = (client.getInstructions() ?? '').length;
    const chars = rows.reduce((sum, row) => sum + row.chars, 0);
    return { preset, rows, chars, instructionChars };
  } finally {
    await client.close();
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const results = [];
  for (const preset of Object.keys(presets))
    results.push(await measure(preset));
  if (process.argv.includes('--json')) console.log(JSON.stringify(results));
  else {
    console.log('preset   tools  tokens(est)  instructions(chars)');
    for (const r of results)
      console.log(
        `${r.preset.padEnd(8)} ${String(r.rows.length).padStart(5)}  ${String(tokens(r.chars)).padStart(11)}  ${String(r.instructionChars).padStart(19)}`
      );
    const full = results.find(r => r.preset === 'full');
    console.log('\ntool         tokens(est)  schema chars');
    for (const row of [...full.rows].sort((a, b) => b.chars - a.chars))
      console.log(
        `${row.name.padEnd(12)} ${String(tokens(row.chars)).padStart(11)}  ${String(row.schemaChars).padStart(12)}`
      );
  }
}
