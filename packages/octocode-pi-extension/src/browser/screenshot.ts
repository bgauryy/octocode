import type { BrowserSession } from './session-state.js';
import { imageBudget, saveImageFile } from '../shared/spill.js';
import type { ImageLimits } from '../shared/vision.js';

type ShotResult = { content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>; details: undefined };

const SHOT_QUALITY = 70;
/** Retakes for a screenshot over the context budget: viewport scale and jpeg quality, each smaller than the last. */
const RETAKES = [{ scale: 0.5, quality: 60 }, { scale: 0.25, quality: 50 }];

/**
 * A screenshot of the visible page the model can afford: the first capture (jpeg unless `png` was asked for) is saved
 * whole under the session's `output/` folder; when it is over the budget (`imageBudget(limits)`: the image cap, or the
 * model's smaller `maxBytes`) the page is retaken at a smaller scale (a png as jpeg), and when no retake fits only the
 * saved file's path is returned. Both say the budget used. A screenshot of a few hundred KB costs few tokens but many
 * request bytes, and a gateway rejects a request of a few MB.
 */
export async function screenshot(page: BrowserSession['page'], format: 'jpeg' | 'png', signal: AbortSignal | undefined, limits?: ImageLimits): Promise<ShotResult> {
  const budget = imageBudget(limits);
  const limit = `limit ${kb(budget)}`;
  const capture = async (options: Record<string, unknown>) => (await page.send<{ data: string }>('Page.captureScreenshot', { captureBeyondViewport: false, ...options }, signal)).data;
  const first = await capture(format === 'png' ? { format } : { format, quality: SHOT_QUALITY });
  const mimeType = `image/${format}`;
  const file = saveImageFile(first, mimeType, 'browser_screenshot');
  const saved = file ? ` Full-size file: ${file}` : '';
  if (first.length <= budget) {
    return { content: [{ type: 'image', data: first, mimeType }, { type: 'text', text: `Screenshot of the visible page (${format}, ${kb(first)}).${saved}` }], details: undefined };
  }
  const view = await viewport(page, signal);
  for (const retake of view ? RETAKES : []) {
    const data = await capture({ format: 'jpeg', quality: retake.quality, clip: { ...view!, scale: retake.scale } });
    if (data.length > budget) continue;
    const text = `Screenshot of the visible page, retaken smaller to fit (jpeg at ${retake.scale}x scale, ${kb(data)}; ${limit}).${saved}`;
    return { content: [{ type: 'image', data, mimeType: 'image/jpeg' }, { type: 'text', text }], details: undefined };
  }
  return { content: [{ type: 'text', text: `Screenshot too large for the context, not shown (${format}, ${kb(first)}; ${limit}).${saved || ' It could not be saved.'}` }], details: undefined };
}

/** Decoded size of `data` (base64 text, or a count of its characters), in whole KB (at least 1). */
function kb(data: string | number): string {
  const bytes = typeof data === 'number' ? Math.floor((data * 3) / 4) : Buffer.byteLength(data, 'base64');
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** The visible viewport's document origin and size in CSS pixels, or undefined when Chrome does not report it. */
async function viewport(page: BrowserSession['page'], signal: AbortSignal | undefined): Promise<{ x: number; y: number; width: number; height: number } | undefined> {
  const metrics = await page.send<{ cssVisualViewport?: { clientWidth?: number; clientHeight?: number; pageX?: number; pageY?: number } }>('Page.getLayoutMetrics', {}, signal).catch(() => undefined);
  const { clientWidth: width, clientHeight: height, pageX = 0, pageY = 0 } = metrics?.cssVisualViewport ?? {};
  return width && height ? { x: pageX, y: pageY, width, height } : undefined;
}
