import { chromeCommands, chromeRecipeOptionNames } from './chrome-contract.mjs';
// Option names come from the typed recipe options in the shared contract.
const recipe = (file: string, description: string) => ({
  file: `cdp-checks/${file}.mjs`,
  description,
  options: chromeRecipeOptionNames[file] ?? [],
});
export const checks = {
  'page-snapshot': recipe(
    'page-snapshot',
    'Capture current controls, regions and frame refs'
  ),
  'page-screenshot': recipe(
    'page-screenshot',
    'Capture viewport, element or all full-page tiles'
  ),
  'dom-operations-check': recipe(
    'dom-operations-check',
    'Legacy DOM actions; prefer step or run plans'
  ),
  'live-har-monitor': recipe(
    'live-har-monitor',
    'Observe network and console during a bounded window'
  ),
  'network-body-har-fetch-check': recipe(
    'network-body-har-fetch-check',
    'Capture matching response bodies and HAR'
  ),
  'network-measure-check': recipe(
    'network-measure-check',
    'Measure network health'
  ),
  'performance-measure-check': recipe(
    'performance-measure-check',
    'Measure rendering and performance'
  ),
  'storage-measure-check': recipe(
    'storage-measure-check',
    'Inventory storage names and cookie flags'
  ),
  'webmcp-tools': recipe(
    'webmcp-tools',
    'Discover or invoke page-declared WebMCP tools'
  ),
  'actionability-diagnostics': recipe(
    'actionability-diagnostics',
    'Diagnose blocked or thin pages'
  ),
  'graph-actionability-check': recipe(
    'graph-actionability-check',
    'Inspect graph selectors and destinations'
  ),
  'stealth-check': recipe(
    'stealth-check',
    'Inspect optional emulation and detector evidence'
  ),
  'affiliates-stealth-check': recipe(
    'affiliates-stealth-check',
    'Inspect optional emulation on affiliate detector'
  ),
};
const helperFiles = {
  artifact: 'artifact-query.mjs',
  skill: 'guide.mjs',
  query: 'evidence-query.mjs',
  'snapshot-query': 'cdp-checks/snapshot-query.mjs',
  'measure-query': 'cdp-checks/measure-query.mjs',
  'har-pager': 'cdp-checks/har-pager.mjs',
  'har-redact': 'cdp-checks/har-redact.mjs',
  'api-replay': 'cdp-checks/api-replay.mjs',
  cookies: 'cookie-bridge.mjs',
  prune: 'prune-artifacts.mjs',
  'protocol-corpus': 'protocol-corpus.mjs',
  'har-ingest': 'har-ingest-to-scrape.mjs',
  'corpus-query': 'corpus-run-local.mjs',
  session: 'session-cli.mjs',
  'dev-check': 'dev-check.mjs',
};
export const helpers: Record<string, [file: string, description: string]> =
  Object.fromEntries(
    Object.entries(helperFiles).map(([name, file]) => [
      name,
      [file, chromeCommands[name as keyof typeof chromeCommands]],
    ])
  );
export const connection = {
  '--port': 'CDP port (9222)',
  '--target': 'Exact target id',
  '--target-url': 'Unique URL substring',
  '--target-type': 'Target type (page by default)',
  '--new-tab': 'Open a tab at URL',
  '--browser': 'Use browser WebSocket',
  '--keep-tab': 'Retain new tab (CLI default)',
  '--close-tab': 'Close only a tab opened by this call',
  '--no-reload': 'Preserve attached state (CLI default)',
  '--stealth': 'Opt-in emulation experiment',
  '--no-stealth': 'Disable emulation',
  '--timeout': 'Protocol request deadline in ms (60000)',
  '--script-timeout': 'Whole run deadline in ms (300000)',
  '--verbose': 'Sandbox diagnostics',
  '--dry-run': 'Validate and print invocation without contacting Chrome',
};

export const launchValues = [
  '--port',
  '--url',
  '--profile',
  '--chromePath',
  '--windowSize',
  '--enableFeatures',
  '--userAgent',
  '--proxyServer',
  '--proxyBypassList',
  '--proxyPacUrl',
  '--config',
];
export const planInputs = {
  run: ['--plan', '--json'],
  step: ['--json'],
  cdp: ['--params'],
  protocol: [],
};
