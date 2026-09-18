import assert from 'node:assert/strict';
import process from 'node:process';
import { Harness } from './harness.mjs';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer && addon && regexWorker,
  'usage: node lsp-search.mjs <reference> <native> <addon> <regex-worker>',
);

const harness = await Harness.connect({
  reference: referenceServer,
  native: nativeServer,
  addon,
  regexWorker,
});
try {
  const source = await harness.writeFixture(
    'src/lib.rs',
    [
      'pub fn greet(name: &str) -> String {',
      '    name.to_owned()',
      '}',
      '',
      'pub fn call_greet() -> String {',
      '    greet("octocode")',
      '}',
      '',
    ].join('\n'),
  );
  await harness.writeFixture(
    'Cargo.toml',
    '[package]\nname = "lsp-parity-fixture"\nversion = "0.1.0"\nedition = "2024"\n',
  );

  await harness.testTool('lspSearch', {
    queries: [{ uri: source, operation: 'documentSymbols' }],
  });
  await harness.testTool('lspSearch', {
    queries: [{
      uri: source,
      operation: 'definition',
      symbolName: 'greet',
      lineHint: 6,
    }],
  });
  await harness.testTool('lspSearch', { queries: [{}] });

  console.log(JSON.stringify({ tool: 'lspSearch', cases: 3, status: 'covered' }));
} finally {
  await harness.close();
}
