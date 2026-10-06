import { describe, expect, it } from 'vitest';
import { visibleWidth } from '@earendil-works/pi-tui';
import { BANNER_HEIGHT, bannerLines, FULL_WIDTH, glowAt, type BannerPaint } from '../src/ui/banner.js';

const plain: BannerPaint = { fg: (_color, text) => text };

describe('banner "OCTOCODE CODE"', () => {
  it('draws both words in the 6-row block font when the terminal is wide enough', () => {
    const lines = bannerLines(plain, 120, { version: '19.2.0', hints: ['/octocode help', 'ctrl+o expand'] });
    expect(lines).toHaveLength(BANNER_HEIGHT);
    expect(FULL_WIDTH).toBeLessThanOrEqual(106);
    expect(lines[0]).toBe(' ██████╗  ██████╗████████╗ ██████╗  ██████╗ ██████╗ ██████╗ ███████╗    ██████╗ ██████╗ ██████╗ ███████╗');
    expect(lines[5]).toBe(' ╚═════╝  ╚═════╝   ╚═╝    ╚═════╝  ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝    ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝');
    expect(lines[6]).toBe('◆ research-driven coding agent · v19.2.0');
    expect(lines[7]).toContain('/octocode help');
    expect(lines[8]).toBe('');
  });

  it('falls back to theme tokens without a gradient: OCTOCODE accent, CODE mdLink, shadow dim', () => {
    const painted: Array<[string, string]> = [];
    bannerLines({ fg: (color, text) => (painted.push([color, text]), text) }, 120);
    expect(painted.slice(0, 4).map(([color]) => color)).toEqual(['accent', 'dim', 'accent', 'dim']);
    expect(painted.some(([color, text]) => color === 'mdLink' && text.includes('██'))).toBe(true);
    expect(painted.filter(([color]) => color === 'dim').every(([, text]) => !text.includes('█'))).toBe(true);
  });

  it('paints strokes and shadows along a left-to-right gradient, and lights the shine band only near its position', () => {
    const calls: Array<{ t: number; shade: string; glow: number; text: string }> = [];
    const paint: BannerPaint = { fg: (_c, text) => text, ramp: (t, shade, glow, text) => (calls.push({ t, shade, glow, text }), text) };
    bannerLines(paint, 120);
    const strokes = calls.filter((call) => call.shade === 'stroke');
    expect(strokes[0]!.t).toBe(0);
    expect(Math.max(...strokes.map((call) => call.t))).toBeGreaterThan(0.95);
    expect(calls.filter((call) => call.shade === 'shadow').every((call) => !call.text.includes('█'))).toBe(true);
    expect(calls.every((call) => call.glow === 0)).toBe(true);
    calls.length = 0;
    bannerLines(paint, 120, {}, 0.5);
    const lit = calls.filter((call) => call.glow > 0);
    expect(lit.length).toBeGreaterThan(0);
    expect(lit.every((call) => Math.abs(call.t - 0.5) < 0.15)).toBe(true);
    // The band enters and leaves off the art: nothing is lit at either end of the sweep.
    expect(glowAt(0, 0)).toBe(0);
    expect(glowAt(1, 1)).toBe(0);
    expect(glowAt(0.5, undefined)).toBe(0);
  });

  it('falls back to a compact 2-row font, then plain text, keeping the height and the width', () => {
    const compact = bannerLines(plain, 80);
    expect(compact).toHaveLength(BANNER_HEIGHT);
    const art = compact.filter((line) => line.trim() !== '').slice(0, 2);
    expect(art[0]).toBe('█▀█ █▀▀ ▀█▀ █▀█ █▀▀ █▀█ █▀▄ █▀▀   █▀▀ █▀█ █▀▄ █▀▀');
    expect(art[1]).toBe('█▄█ █▄▄  █  █▄█ █▄▄ █▄█ █▄▀ ██▄   █▄▄ █▄█ █▄▀ ██▄');
    const tiny = bannerLines(plain, 30, { version: '1.0.0' });
    expect(tiny).toHaveLength(BANNER_HEIGHT);
    expect(tiny.join('\n')).toContain('OCTOCODE CODE');
    for (const width of [4, 20, 49, 50, 80, 103, 104, 200]) {
      for (const shine of [undefined, 0.3]) {
        const lines = bannerLines(plain, width, { version: '1.0.0', hints: ['/octocode help', 'ctrl+o expand'] }, shine);
        expect(lines, `width ${width}`).toHaveLength(BANNER_HEIGHT);
        expect(lines.every((line) => visibleWidth(line) <= width)).toBe(true);
      }
    }
  });
});
