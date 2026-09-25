// The ONE place tool contracts are generated. Every consumer (MCP, CLI, and
// octocode-native) reads these outputs; nothing else generates or copies them.
//
//   core Zod schemas ── buildEnforcementContractIr ──▶ contract/tool-contract.json
//        │                                             (native embed: schemas, rules, defaults)
//        ├── buildNativeParityFixtures ───────────────▶ contract/contract-fixtures.json
//        └── tool-contract/bundle.ts ──▶ contract/tool-types.schema.json
//                 ├── tool-contract/typescript.ts ──▶ src/contracts/toolTypes.generated.ts
//                 └── tool-contract/rust.ts ────────▶ contract/tool_types.rs
//
// octocode-native's build.rs embeds contract/ in place, so regenerating here
// is the whole change. Outputs are committed and never hand-edited; `--check`
// (no cargo needed) fails when any is stale.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  buildEnforcementContractIr,
  buildNativeParityFixtures,
  canonicalizeNativeContractIr,
} from '@octocodeai/octocode-core/schema';
import { buildToolTypesBundle } from './tool-contract/bundle.ts';
import { renderRust, rustHeader } from './tool-contract/rust.ts';
import { renderTypeScript } from './tool-contract/typescript.ts';

export { buildToolTypesBundle } from './tool-contract/bundle.ts';

const contractFile = (name: string): string =>
  fileURLToPath(new URL(`../contract/${name}`, import.meta.url));
const rustPath = contractFile('tool_types.rs');
const tsPath = fileURLToPath(new URL('../src/contracts/toolTypes.generated.ts', import.meta.url));

const sha256 = (text: string): string => createHash('sha256').update(text).digest('hex');

function corePackageVersion(): string {
  // `./schema` resolves to <core>/dist/schema.js; the manifest sits one level up.
  const manifest = new URL('../package.json', import.meta.resolve('@octocodeai/octocode-core/schema'));
  return (JSON.parse(readFileSync(manifest, 'utf8')) as { version: string }).version;
}

/** Every generated file except the Rust body, plus the header it must start with. */
export async function generateToolContract(): Promise<{
  files: Map<string, string>;
  bundleJson: string;
  rustHeader: string;
}> {
  const ir = buildEnforcementContractIr();
  const contractJson = canonicalizeNativeContractIr(ir);
  const { fingerprint, bundle } = buildToolTypesBundle(ir);
  const bundleJson = `${JSON.stringify(bundle, null, 2)}\n`;
  const provenance = {
    sourcePackage: '@octocodeai/octocode-core',
    sourceVersion: corePackageVersion(),
    contractFormatVersion: ir.contractFormatVersion,
    contractFingerprint: fingerprint,
    contractSha256: sha256(contractJson),
  };
  return {
    files: new Map([
      [contractFile('tool-contract.json'), contractJson],
      [contractFile('contract-fixtures.json'), `${JSON.stringify(buildNativeParityFixtures(), null, 2)}\n`],
      [contractFile('provenance.json'), `${JSON.stringify(provenance, null, 2)}\n`],
      [contractFile('tool-types.schema.json'), bundleJson],
      [tsPath, await renderTypeScript(fingerprint, bundle)],
    ]),
    bundleJson,
    rustHeader: rustHeader(fingerprint, sha256(bundleJson)),
  };
}

async function main(): Promise<void> {
  const { files, bundleJson, rustHeader: header } = await generateToolContract();
  if (!process.argv.includes('--check')) {
    for (const [path, content] of files) writeFileSync(path, content);
    writeFileSync(rustPath, renderRust(header, bundleJson));
    return;
  }
  const read = (path: string): string => {
    try {
      return readFileSync(path, 'utf8');
    } catch {
      return '';
    }
  };
  const stale = [...files].filter(([path, content]) => read(path) !== content).map(([path]) => path);
  if (!read(rustPath).startsWith(header)) stale.push(rustPath);
  if (stale.length > 0) {
    throw new Error(
      `Generated tool contract is stale (${stale.join(', ')}). Run: yarn contracts:regen`
    );
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
