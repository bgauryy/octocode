/**
 * Minimal terminal color helpers.
 * Stripped when NO_COLOR is set or stdout is not a TTY.
 */

const enabled = !process.env['NO_COLOR'] && process.stdout.isTTY;

type Color = 'green' | 'yellow' | 'red' | 'cyan' | 'blue' | 'dim' | 'bold';

const CODES: Record<Color, [number, number]> = {
  bold:   [1, 22],
  dim:    [2, 22],
  red:    [31, 39],
  green:  [32, 39],
  yellow: [33, 39],
  blue:   [34, 39],
  cyan:   [36, 39],
};

export function color(name: Color, text: string): string {
  if (!enabled) return text;
  const [open, close] = CODES[name]!;
  return `\x1b[${open}m${text}\x1b[${close}m`;
}

export const bold  = (t: string) => color('bold',   t);
export const dim   = (t: string) => color('dim',    t);
export const green = (t: string) => color('green',  t);
export const yellow= (t: string) => color('yellow', t);
export const red   = (t: string) => color('red',    t);
export const cyan  = (t: string) => color('cyan',   t);
