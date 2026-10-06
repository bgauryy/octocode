import { truncateToWidth } from '@earendil-works/pi-tui';

/**
 * The "OCTOCODE CODE" banner drawn as Pi's header. Pure in (paint, width, version, shine) and always BANNER_HEIGHT
 * rows: a height change on resize makes Pi redraw (and clear) the whole scrollback. Three layouts by width: the 6-row
 * ANSI Shadow block font, a 2-row half-block font, then plain text. Strokes run along a left-to-right gradient taken
 * from the theme, box-drawing shadows are a darker shade of the same hue, and `shine` (0..1) is the position of a
 * light band that sweeps across once on start.
 */

export type Shade = 'stroke' | 'shadow';

export interface BannerPaint {
  /** Text in a theme colour token (`accent`, `muted`, `dim`, …). */
  fg(token: string, text: string): string;
  /**
   * Text at gradient position `t` (0 = left edge, 1 = right edge), lit by `glow` (0..1) of the shine band. Without it
   * strokes fall back to one theme colour per word.
   */
  ramp?(t: number, shade: Shade, glow: number, text: string): string;
}

/** ANSI Shadow glyphs: `█` strokes and box-drawing shadow. */
const SHADOW: Record<string, string[]> = {
  O: [' ██████╗ ', '██╔═══██╗', '██║   ██║', '██║   ██║', '╚██████╔╝', ' ╚═════╝ '],
  C: [' ██████╗', '██╔════╝', '██║     ', '██║     ', '╚██████╗', ' ╚═════╝'],
  T: ['████████╗', '╚══██╔══╝', '   ██║   ', '   ██║   ', '   ██║   ', '   ╚═╝   '],
  D: ['██████╗ ', '██╔══██╗', '██║  ██║', '██║  ██║', '██████╔╝', '╚═════╝ '],
  E: ['███████╗', '██╔════╝', '█████╗  ', '██╔══╝  ', '███████╗', '╚══════╝'],
};

/** Half-block glyphs, two rows high. */
const COMPACT: Record<string, string[]> = {
  O: ['█▀█', '█▄█'],
  C: ['█▀▀', '█▄▄'],
  T: ['▀█▀', ' █ '],
  D: ['█▀▄', '█▄▀'],
  E: ['█▀▀', '██▄'],
};

const WORDS = [
  { text: 'OCTOCODE', color: 'accent' },
  { text: 'CODE', color: 'mdLink' },
] as const;
const WORD_GAP = '   ';
const ART_ROWS = 6;
/** Art, tagline, hints and a blank separator. */
export const BANNER_HEIGHT = ART_ROWS + 3;
/** Gradient steps: neighbouring cells of one step share one escape sequence. */
const STEPS = 32;
/** Half-width of the shine band, as a fraction of the art width. */
const BAND = 0.09;

const word = (font: Record<string, string[]>, text: string, row: number, gap: string) => [...text].map((letter) => font[letter]![row]).join(gap);
const artLines = (font: Record<string, string[]>, rows: number, letterGap: string) =>
  Array.from({ length: rows }, (_, row) => WORDS.map(({ text }) => word(font, text, row, letterGap)).join(WORD_GAP));

const FULL = artLines(SHADOW, ART_ROWS, '');
const SMALL = artLines(COMPACT, 2, ' ');
export const FULL_WIDTH = Math.max(...FULL.map((line) => line.length));
const SMALL_WIDTH = Math.max(...SMALL.map((line) => line.length));
/** Column where the second word starts, for the per-word fallback colours. */
const splitAt = (font: Record<string, string[]>, gap: string) => word(font, WORDS[0].text, 0, gap).length + 1;

const isStroke = (char: string) => /[█▀▄]/.test(char);

/** How lit a column is by a band centred at `shine` (outside 0..1: no band). */
export function glowAt(t: number, shine: number | undefined): number {
  if (shine === undefined || shine < 0 || shine > 1) return 0;
  // The band enters from beyond the left edge and leaves beyond the right one.
  const centre = -BAND + shine * (1 + 2 * BAND);
  const distance = Math.abs(t - centre);
  return distance >= BAND ? 0 : Math.round((1 - distance / BAND) * 8) / 8;
}

/** One art line, cut into runs that share a paint (gradient step, shade, glow). */
function paintArt(paint: BannerPaint, line: string, width: number, split: number, shine: number | undefined): string {
  let out = '';
  let run = '';
  let key = '';
  let style: ((text: string) => string) | undefined;
  const flush = () => {
    if (run) out += style ? style(run) : run;
    run = '';
  };
  [...line].forEach((char, column) => {
    let nextKey: string;
    let nextStyle: ((text: string) => string) | undefined;
    if (char === ' ') {
      nextKey = ' ';
      nextStyle = undefined;
    } else {
      const shade: Shade = isStroke(char) ? 'stroke' : 'shadow';
      if (paint.ramp) {
        const t = Math.round((column / Math.max(1, width - 1)) * STEPS) / STEPS;
        const glow = glowAt(t, shine);
        nextKey = `${shade}:${t}:${glow}`;
        nextStyle = (text) => paint.ramp!(t, shade, glow, text);
      } else {
        const color = shade === 'shadow' ? 'dim' : column < split ? WORDS[0].color : WORDS[1].color;
        nextKey = color;
        nextStyle = (text) => paint.fg(color, text);
      }
    }
    if (nextKey !== key) {
      flush();
      key = nextKey;
      style = nextStyle;
    }
    run += char;
  });
  flush();
  return out;
}

export interface BannerInfo {
  version?: string | undefined;
  /** Short key hints for the second line, e.g. `ctrl+o expand`. */
  hints?: string[] | undefined;
}

export function bannerLines(paint: BannerPaint, width: number, info: BannerInfo = {}, shine?: number): string[] {
  let art: string[];
  if (width >= FULL_WIDTH) art = FULL.map((line) => paintArt(paint, line, FULL_WIDTH, splitAt(SHADOW, ''), shine));
  else if (width >= SMALL_WIDTH) art = ['', ...SMALL.map((line) => paintArt(paint, line, SMALL_WIDTH, splitAt(COMPACT, ' '), shine))];
  else art = ['', WORDS.map(({ text, color }) => paint.fg(color, text)).join(' ')];
  const dot = paint.fg('dim', ' · ');
  const tagline = `${paint.fg('accent', '◆')} ${paint.fg('muted', 'research-driven coding agent')}${info.version ? `${dot}${paint.fg('dim', `v${info.version}`)}` : ''}`;
  const hints = (info.hints ?? []).map((hint) => {
    const [key = '', ...label] = hint.split(' ');
    return `${paint.fg('muted', key)}${label.length > 0 ? ` ${paint.fg('dim', label.join(' '))}` : ''}`;
  });
  const all = [...art, tagline, hints.join(dot)];
  while (all.length < BANNER_HEIGHT) all.push('');
  // Every line must fit the width, however narrow the terminal.
  return all.slice(0, BANNER_HEIGHT).map((line) => truncateToWidth(line, Math.max(1, width), ''));
}
