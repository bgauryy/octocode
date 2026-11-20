/**
 * Page PDF: Page.printToPDF of the current page to page.pdf, streamed (transferMode
 * ReturnAsStream) when Chrome supports it, else decoded from the base64 reply.
 * Options: PDF_LANDSCAPE, PDF_PRINT_BACKGROUND (1/0), PDF_SCALE (0.1 to 2).
 */
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import { openSink, pumpStream } from './stream-io.mjs';
import { flagOption, textOption, say } from './stream-options.mjs';

export async function run(cdp: Session) {
  const landscape = flagOption('PDF_LANDSCAPE', false);
  const printBackground = flagOption('PDF_PRINT_BACKGROUND', false);
  const scaleText = textOption('PDF_SCALE');
  const scale = scaleText === undefined ? 1 : Number(scaleText);
  if (!Number.isFinite(scale) || scale < 0.1 || scale > 2)
    throw new Error('PDF_SCALE must be between 0.1 and 2');

  const params = { landscape, printBackground, scale };
  const file = join(cdp.outputDir, 'page.pdf');
  let bytes: number;
  let transfer: 'stream' | 'base64';
  let printed: Payload;
  try {
    printed = await cdp.send('Page.printToPDF', {
      ...params,
      transferMode: 'ReturnAsStream',
    });
  } catch {
    printed = await cdp.send('Page.printToPDF', params);
  }
  if (printed.stream) {
    transfer = 'stream';
    ({ bytes } = await pumpStream(cdp, printed.stream, file));
  } else {
    transfer = 'base64';
    const sink = openSink(file);
    try {
      sink.write(Buffer.from(String(printed.data ?? ''), 'base64'));
    } finally {
      sink.close();
    }
    bytes = sink.bytes;
  }
  say(
    `[METRIC] PAGE_PDF bytes=${bytes} transfer=${transfer} landscape=${landscape} printBackground=${printBackground} scale=${scale}`
  );
  say(`[ARTIFACT] PAGE_PDF ${file}`);
}
