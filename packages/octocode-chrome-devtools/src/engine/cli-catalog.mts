import { chromeCommands, chromeGuideTopics } from './chrome-contract.mjs';
export const guideTopics = chromeGuideTopics;
const recipe = (file: string, description: string, options: string[] = []) => ({
  file: `cdp-checks/${file}.mjs`,
  description,
  options,
});
export const checks = {
  'page-snapshot': recipe(
    'page-snapshot',
    'Capture current controls, regions and frame refs',
    [
      'SNAPSHOT_DEPTH',
      'SNAPSHOT_WAIT_SELECTOR',
      'SNAPSHOT_WAIT_TEXT',
      'SNAPSHOT_WAIT_MS',
      'SNAPSHOT_MAX',
      'SNAPSHOT_TEXT',
      'SNAPSHOT_ROOT',
      'SNAPSHOT_VIEWPORT',
      'SNAPSHOT_OUTLINE',
      'SNAPSHOT_CONTEXT',
      'SNAPSHOT_URLS',
      'SNAPSHOT_CLICKABLE',
      'SNAPSHOT_STDOUT',
    ]
  ),
  'page-screenshot': recipe(
    'page-screenshot',
    'Capture viewport, element or all full-page tiles',
    [
      'SHOT_FULL',
      'SHOT_SELECTOR',
      'SHOT_FORMAT',
      'SHOT_QUALITY',
      'SHOT_SCALE',
      'SHOT_ANNOTATE',
    ]
  ),
  'dom-operations-check': recipe(
    'dom-operations-check',
    'Legacy DOM actions; prefer step or run plans',
    [
      'DOM_SELECTOR',
      'DOM_REF',
      'DOM_ROLE',
      'DOM_NAME',
      'DOM_ACTION',
      'DOM_VALUE',
      'DOM_STABILITY_MS',
      'DOM_INPUT',
      'DOM_KEY',
      'DOM_SETTLE_MS',
      'DOM_DIALOG',
      'DOM_STEPS',
      'DOM_WAIT_TEXT',
      'DOM_WAIT_MS',
      'DOM_TO_REF',
      'DOM_TO_SELECTOR',
      'DOM_DIFF',
      'DOM_TRACE_EVENTS',
    ]
  ),
  'live-har-monitor': recipe(
    'live-har-monitor',
    'Observe network and console during a bounded window',
    ['MONITOR_MS', 'MONITOR_URL', 'SLOW_MS', 'MAX_STDOUT_ITEMS']
  ),
  'network-body-har-fetch-check': recipe(
    'network-body-har-fetch-check',
    'Capture matching response bodies and HAR',
    ['BODY_URL', 'BODY_MATCH', 'BODY_WAIT_MS']
  ),
  'network-measure-check': recipe(
    'network-measure-check',
    'Measure network health',
    ['MEASURE_URL', 'MEASURE_EXISTING', 'NET_WAIT_MS', 'NET_SLOW_MS']
  ),
  'performance-measure-check': recipe(
    'performance-measure-check',
    'Measure rendering and performance',
    ['MEASURE_URL', 'MEASURE_EXISTING', 'PERF_WAIT_MS', 'PERF_SLOW_RESOURCE_MS']
  ),
  'storage-measure-check': recipe(
    'storage-measure-check',
    'Inventory storage names and cookie flags',
    ['MEASURE_URL', 'MEASURE_EXISTING', 'STORAGE_WAIT_MS']
  ),
  'webmcp-tools': recipe(
    'webmcp-tools',
    'Discover or invoke page-declared WebMCP tools',
    [
      'WEBMCP_ACTION',
      'WEBMCP_TOOL',
      'WEBMCP_INPUT',
      'WEBMCP_FRAME',
      'WEBMCP_WAIT_MS',
    ]
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
    'Inspect optional emulation and detector evidence',
    ['STEALTH_CHECK_URL']
  ),
  'affiliates-stealth-check': recipe(
    'affiliates-stealth-check',
    'Inspect optional emulation on affiliate detector',
    ['AFFILIATES_CHECK_URL']
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
};
export const helpers: Record<string, [file: string, description: string]> =
  Object.fromEntries(
    Object.entries(helperFiles).map(([name, file]) => [
      name,
      [file, chromeCommands[name as keyof typeof chromeCommands]],
    ])
  );
export const commands = chromeCommands;
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
