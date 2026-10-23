// Generates the config contract outputs from config-contract.json (validated against
// config-contract.schema.json): the TypeScript field metadata and the settings reference.
// `--check` fails when either committed output is stale.
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { renderDocumentation } from './config-contract/documentation.ts';
import { loadContract } from './config-contract/model.ts';
import { renderTypeScript } from './config-contract/typescript.ts';

const contractPath = fileURLToPath(new URL('../config-contract.json', import.meta.url));
const schemaPath = fileURLToPath(
  new URL('../config-contract.schema.json', import.meta.url)
);
const outputPath = fileURLToPath(
  new URL('../src/config/contract.generated.ts', import.meta.url)
);
const documentationPath = fileURLToPath(
  new URL('../../../docs/generated/CONFIG_SETTINGS.md', import.meta.url)
);

export async function generateConfigContract(): Promise<{
  source: string;
  documentation: string;
}> {
  const contract = await loadContract(contractPath, schemaPath);
  return { source: renderTypeScript(contract), documentation: renderDocumentation(contract) };
}

async function main(): Promise<void> {
  const generated = await generateConfigContract();
  if (process.argv.includes('--check')) {
    const [currentSource, currentDocumentation] = await Promise.all([
      readFile(outputPath, 'utf8').catch(() => ''),
      readFile(documentationPath, 'utf8').catch(() => ''),
    ]);
    if (
      currentSource !== generated.source ||
      currentDocumentation !== generated.documentation
    ) {
      throw new Error(
        'Generated config contract is stale. Run: yarn workspace @octocodeai/config generate:config-contract'
      );
    }
    return;
  }
  await Promise.all([
    writeFile(outputPath, generated.source),
    writeFile(documentationPath, generated.documentation),
  ]);
}

await main();
