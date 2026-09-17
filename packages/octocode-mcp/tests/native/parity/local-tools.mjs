import assert from 'node:assert/strict';
import process from 'node:process';
import { Harness } from './harness.mjs';

const [referenceServer, nativeServer, addon, regexWorker] = process.argv.slice(2);
assert.ok(
  referenceServer && nativeServer && addon && regexWorker,
  'usage: node local-tools.mjs <reference> <native> <addon> <regex-worker>',
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
    'src/main.rs',
    'mod lib;\nfn main() { println!("{}", lib::call_greet()); }\n',
  );
  await harness.writeFixture(
    'Cargo.toml',
    '[package]\nname = "parity-fixture"\nversion = "0.1.0"\nedition = "2024"\n',
  );

  await harness.testTool('localFetch', {
    queries: [{ path: source, startLine: 1, endLine: 3 }],
  });

  await harness.testTool('astSearch', {
    queries: [{ operation: 'files', path: harness.workspaceDir, extensions: ['rs'], sort: 'path' }],
  });
  await harness.testTool('astSearch', {
    queries: [{ operation: 'match', path: source, langType: 'rust', pattern: 'greet("octocode")' }],
  });
  await harness.testTool('astSearch', {
    queries: [{ operation: 'symbols', path: source, name: 'greet' }],
  });
  await harness.testTool('astSearch', {
    queries: [{
      operation: 'topology',
      analysis: 'dependencies',
      path: harness.workspaceDir,
      file: 'src/main.rs',
      depth: 2,
    }],
  });

  await harness.testTool('astRewrite', {
    queries: [{
      path: source,
      langType: 'rust',
      ruleKind: 'pattern',
      pattern: 'greet($A)',
      rewrite: 'welcome($A)',
      apply: false,
    }],
  });

  for (const name of ['localFetch', 'astSearch', 'astRewrite']) {
    await harness.testTool(name, { queries: [{}] });
  }

  console.log(JSON.stringify({
    tools: ['localFetch', 'astSearch', 'astRewrite'],
    cases: 10,
    status: 'covered',
  }));
} finally {
  await harness.close();
}
