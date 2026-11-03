// Generates every config contract output from the Zod declaration in config-contract/: the
// contract JSON and its JSON Schema, the TypeScript field metadata, and the settings reference.
// `--check` fails when a committed output is stale.
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { contractFromDeclaration } from './config-contract/declaration/index.ts';
import { renderDocumentation } from './config-contract/documentation.ts';
import { renderContractJson, renderContractSchema } from './config-contract/emit.ts';
import { renderTypeScript } from './config-contract/typescript.ts';

const path = (relative: string): string => fileURLToPath(new URL(relative, import.meta.url));

export type GeneratedFile = { path: string; content: string };

/** Every generated file with the content its committed copy must have. */
export function generateConfigContract(): GeneratedFile[] {
  const contract = contractFromDeclaration();
  return [
    { path: path('../config-contract.json'), content: renderContractJson() },
    { path: path('../config-contract.schema.json'), content: renderContractSchema() },
    { path: path('../src/config/contract.generated.ts'), content: renderTypeScript(contract) },
    {
      path: path('../../../docs/generated/CONFIG_SETTINGS.md'),
      content: renderDocumentation(contract),
    },
  ];
}

async function main(): Promise<void> {
  const files = generateConfigContract();
  if (process.argv.includes('--check')) {
    const stale: string[] = [];
    for (const file of files) {
      const current = await readFile(file.path, 'utf8').catch(() => '');
      if (current !== file.content) stale.push(file.path);
    }
    if (stale.length > 0) {
      throw new Error(
        `Generated config contract is stale: ${stale.join(', ')}. Run: yarn workspace @octocodeai/config generate:config-contract`
      );
    }
    return;
  }
  await Promise.all(files.map(file => writeFile(file.path, file.content)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
