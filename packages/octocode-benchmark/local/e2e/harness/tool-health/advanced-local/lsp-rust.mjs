import { lspLocations } from '../../responses.mjs';
import { equal, fixturePath, relative, scenario } from './helpers.mjs';

const workspaceRoot = fixturePath('rust');
const path = `${workspaceRoot}/src/lib.rs`;

export async function rustContexts(ctx, surface, checks) {
  const variants = [
    { label: 'omitted', expected: ['base_feature', 'always'] },
    { label: 'empty-features', rustContext: { features: [] }, expected: ['base_feature', 'always'] },
    { label: 'named-feature', rustContext: { features: ['bright'] }, expected: ['base_feature', 'bright_feature', 'always'] },
    { label: 'all-features', rustContext: { features: 'all' }, expected: ['base_feature', 'bright_feature', 'dim_feature', 'always'] },
    { label: 'no-defaults', rustContext: { noDefaultFeatures: true }, expected: ['always'] },
    { label: 'named-without-defaults', rustContext: { features: ['dim'], noDefaultFeatures: true }, expected: ['dim_feature', 'always'] },
    { label: 'custom-cfg', rustContext: { cfgs: ['transport_cfg'] }, expected: ['base_feature', 'custom_cfg', 'always'] },
    { label: 'disabled-cfg', rustContext: { cfgs: ['transport_cfg', '!transport_cfg'] }, expected: ['base_feature', 'always'] },
    { label: 'omitted-after-overrides', expected: ['base_feature', 'always'] },
  ];
  for (const { label, rustContext, expected } of variants) {
    const query = { workspaceRoot, path, operation: 'references', symbolName: 'target', line: 1, includeDeclaration: false, ...(rustContext === undefined ? {} : { rustContext }) };
    const result = await scenario(ctx, 'lspSearch', surface, { label: `lsp.rust.${label}`, query, checks });
    const found = result.data.flatMap(data => lspLocations({ sc: { results: [{ data }] } }))
      .map(item => [relative(ctx, item.path), item.displayRange.startLine]);
    equal(checks, `lsp.rust.references.${label}.${surface}`, found, expected.map(module => [`rust/src/${module}.rs`, 1]));
  }
}
