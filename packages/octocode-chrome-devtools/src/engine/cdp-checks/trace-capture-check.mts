/**
 * Trace capture: Tracing.start with categories, wait TRACE_MS (loading TRACE_URL after the start
 * when given), Tracing.end, then stream the trace to trace.json. The file is Chrome trace JSON
 * ({"traceEvents": [...]}), loadable in Perfetto and the DevTools Performance panel, and holds
 * every chunk Chrome produced. Options: TRACE_URL, TRACE_MS (required), TRACE_CATEGORIES.
 */
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import { pumpStream } from './stream-io.mjs';
import {
  countOption,
  sleep,
  stringsOption,
  textOption,
  say,
} from './stream-options.mjs';

/**
 * Categories when TRACE_CATEGORIES is not given: the DevTools Performance panel's timeline set
 * (main-thread tasks, layout, paint, network, user timing, V8 execution, loading, input latency).
 */
export const DEFAULT_TRACE_CATEGORIES = [
  'devtools.timeline',
  'disabled-by-default-devtools.timeline',
  'v8.execute',
  'blink.user_timing',
  'loading',
  'latencyInfo',
];

export async function run(cdp: Session) {
  const windowMs = countOption('TRACE_MS');
  if (windowMs === undefined) throw new Error('TRACE_MS is required');
  const url = textOption('TRACE_URL');
  const categories =
    stringsOption('TRACE_CATEGORIES') ?? DEFAULT_TRACE_CATEGORIES;
  if (!categories.length) throw new Error('TRACE_CATEGORIES is empty');

  await cdp.send('Page.enable');
  const complete = new Promise<Payload>(resolve => {
    const onComplete = (event: Payload) => {
      cdp.off('Tracing.tracingComplete', onComplete);
      resolve(event);
    };
    cdp.on('Tracing.tracingComplete', onComplete);
  });
  await cdp.send('Tracing.start', {
    transferMode: 'ReturnAsStream',
    streamFormat: 'json',
    traceConfig: {
      recordMode: 'recordAsMuchAsPossible',
      includedCategories: categories,
    },
  });
  try {
    if (url) await cdp.send('Page.navigate', { url });
    await sleep(windowMs);
  } finally {
    await cdp.send('Tracing.end');
  }
  const done = await complete;
  if (!done.stream) throw new Error('Chrome returned no trace stream');

  const file = join(cdp.outputDir, 'trace.json');
  const { bytes, chunks } = await pumpStream(cdp, done.stream, file);
  say(
    `[METRIC] TRACE bytes=${bytes} chunks=${chunks} categories=${categories.length} windowMs=${windowMs} dataLoss=${Boolean(done.dataLossOccurred)}`
  );
  if (done.dataLossOccurred)
    say(
      '[FINDING] TRACE_DATA_LOSS Chrome dropped events (buffer full); shorten TRACE_MS or narrow TRACE_CATEGORIES'
    );
  say(`[ARTIFACT] TRACE ${file}`);
}
