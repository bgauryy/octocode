/** ANSI-safe terminal width primitives below both components and render adapters. */
import {
  truncateToWidth as piTruncateToWidth,
  visibleWidth as piVisibleWidth,
} from '@earendil-works/pi-tui';

import { sanitizeLine } from './palette.js';

export function visibleWidth(str: string): number {
  return piVisibleWidth(sanitizeLine(str));
}

export function truncateToWidth(
  str: string,
  maxWidth: number,
  ellipsis = '\u2026',
): string {
  return piTruncateToWidth(sanitizeLine(str), maxWidth, ellipsis);
}
