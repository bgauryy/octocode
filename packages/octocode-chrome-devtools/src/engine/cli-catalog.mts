import {
  chromeCommands,
  chromeHelperDescriptions,
  chromeRecipeOptionNames,
} from './chrome-contract.mjs';
// Option names come from the typed recipe options in the shared contract.
const recipe = (file: string, description: string, name = file) => ({
  file: `cdp-checks/${file}.mjs`,
  description,
  options: chromeRecipeOptionNames[name] ?? [],
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
    'Legacy DOM actions; prefer run plans'
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
  'script-sources': recipe(
    'script-sources',
    'Save every script a page parses, with its source'
  ),
  'source-map': recipe(
    'source-map-check',
    'Resolve a script source map and map positions to original sources',
    'source-map'
  ),
  'storage-dump': recipe(
    'storage-dump-check',
    'Dump storage, cache and service worker inventories with values redacted',
    'storage-dump'
  ),
  'network-events': recipe(
    'network-events-check',
    'Record WebSocket, SSE and HTTP events in a bounded window',
    'network-events'
  ),
  'cdp-domain': recipe(
    'cdp-domain-check',
    'Enable one CDP domain; dump its getters and record its events to files',
    'cdp-domain'
  ),
  'page-html': recipe(
    'page-html-check',
    'Save page or element HTML, frames and shadow roots as files',
    'page-html'
  ),
  'page-archive': recipe(
    'page-archive-check',
    'Save the page as one MHTML archive',
    'page-archive'
  ),
  'page-resources': recipe(
    'page-resources-check',
    'Save the page resources with an index',
    'page-resources'
  ),
  'console-log': recipe(
    'console-log-check',
    'Record console messages in a bounded window to a jsonl file',
    'console-log'
  ),
  downloads: recipe(
    'downloads-check',
    'Save files a page downloads (needs user consent)',
    'downloads'
  ),
  'network-conditions': recipe(
    'network-conditions-check',
    'Load a page offline, throttled, blocked or mocked',
    'network-conditions'
  ),
  coverage: recipe(
    'coverage-check',
    'Measure used and unused JS and CSS bytes',
    'coverage'
  ),
  'trace-capture': recipe(
    'trace-capture-check',
    'Capture a Chrome trace to a file',
    'trace-capture'
  ),
  'heap-snapshot': recipe(
    'heap-snapshot-check',
    'Stream a heap snapshot to a file',
    'heap-snapshot'
  ),
  'page-pdf': recipe(
    'page-pdf-check',
    'Print the current page to a PDF file',
    'page-pdf'
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
  proxy: 'cdp-proxy.mjs',
  flow: 'flow-command.mjs',
  group: 'group-command.mjs',
  'dev-check': 'dev-check.mjs',
};
/** Every command the engine CLI runs: the typed commands plus the helpers the `helper` tool reaches. */
export const rawCommands: Record<string, string> = {
  ...chromeCommands,
  ...chromeHelperDescriptions,
};
export const helpers: Record<string, [file: string, description: string]> =
  Object.fromEntries(
    Object.entries(helperFiles).map(([name, file]) => [
      name,
      [file, rawCommands[name]],
    ])
  );
export const connection = {
  '--port': 'CDP port (9222)',
  '--target': 'Exact target id',
  '--target-url': 'Unique URL substring',
  '--target-type': 'Target type (page by default)',
  '--group': 'Select a tab of a group made with the group command',
  '--group-index': 'Zero-based tab within --group (0)',
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
  '--disableFeatures',
  '--blinkFeatures',
  '--chrome-flags',
  '--userAgent',
  '--proxyServer',
  '--proxyBypassList',
  '--proxyPacUrl',
  '--config',
  '--allow-origin',
  '--emulation',
];
/** Boolean `open` flags; --allow-origin takes a comma list. */
export const launchBooleans = [
  '--headless',
  '--dry-run',
  '--attach',
  '--acknowledge',
  '--no-sandbox',
];
export const planInputs = {
  run: ['--plan', '--json'],
  cdp: ['--params'],
  protocol: [],
};
