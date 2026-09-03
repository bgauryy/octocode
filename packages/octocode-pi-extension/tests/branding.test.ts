import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, it, expect } from 'vitest';

import { renderBannerLines, renderTagline, renderBannerWithTagline } from '../src/branding/banner.js';

// ─── Stub theme ───────────────────────────────────────────────────────────────

const stubTheme = {
  fg: (color: string, text: string) => `[${color}:${text}]`,
  bold: (text: string) => `**${text}**`,
};

// ─── Required tokens ─────────────────────────────────────────────────────────
// All 51 tokens that every pi theme must define (from themes.md).

const REQUIRED_TOKENS: readonly string[] = [
  // Core UI (11)
  'accent', 'border', 'borderAccent', 'borderMuted',
  'success', 'error', 'warning', 'muted', 'dim', 'text', 'thinkingText',
  // Backgrounds & Content (11)
  'selectedBg', 'userMessageBg', 'userMessageText',
  'customMessageBg', 'customMessageText', 'customMessageLabel',
  'toolPendingBg', 'toolSuccessBg', 'toolErrorBg', 'toolTitle', 'toolOutput',
  // Markdown (10)
  'mdHeading', 'mdLink', 'mdLinkUrl', 'mdCode', 'mdCodeBlock',
  'mdCodeBlockBorder', 'mdQuote', 'mdQuoteBorder', 'mdHr', 'mdListBullet',
  // Tool Diffs (3)
  'toolDiffAdded', 'toolDiffRemoved', 'toolDiffContext',
  // Syntax Highlighting (9)
  'syntaxComment', 'syntaxKeyword', 'syntaxFunction', 'syntaxVariable',
  'syntaxString', 'syntaxNumber', 'syntaxType', 'syntaxOperator', 'syntaxPunctuation',
  // Thinking Level Borders (6)
  'thinkingOff', 'thinkingMinimal', 'thinkingLow',
  'thinkingMedium', 'thinkingHigh', 'thinkingXhigh',
  // Bash mode (1)
  'bashMode',
] as const;

const __dirname = dirname(fileURLToPath(import.meta.url));

function loadTheme(filename: string): Record<string, unknown> {
  const p = resolve(__dirname, '..', 'themes', filename);
  return JSON.parse(readFileSync(p, 'utf8')) as Record<string, unknown>;
}

// ─── Theme JSON completeness ──────────────────────────────────────────────────

describe('theme JSON completeness', () => {
  for (const filename of ['octocode-dark.json', 'octocode-light.json']) {
    it(`${filename} contains all 51 required tokens`, () => {
      const theme = loadTheme(filename);
      const colors = theme['colors'] as Record<string, unknown> | undefined;
      expect(colors).toBeDefined();
      const missing = REQUIRED_TOKENS.filter((t) => !(t in (colors ?? {})));
      expect(missing, `Missing tokens: ${missing.join(', ')}`).toHaveLength(0);
    });

    it(`${filename} has exactly 51 or more tokens (no undercounting)`, () => {
      const theme = loadTheme(filename);
      const colors = theme['colors'] as Record<string, unknown> | undefined;
      expect(Object.keys(colors ?? {}).length).toBeGreaterThanOrEqual(51);
    });

    it(`${filename} has a non-empty name`, () => {
      const theme = loadTheme(filename);
      expect(typeof theme['name']).toBe('string');
      expect((theme['name'] as string).length).toBeGreaterThan(0);
    });
  }

  it('theme names are unique', () => {
    const dark = loadTheme('octocode-dark.json');
    const light = loadTheme('octocode-light.json');
    expect(dark['name']).not.toBe(light['name']);
  });

  for (const filename of ['octocode-dark.json', 'octocode-light.json']) {
    it(`${filename} keeps every tool state on the terminal background`, () => {
      const theme = loadTheme(filename);
      const colors = theme['colors'] as Record<string, unknown>;
      expect(colors['toolPendingBg']).toBe('');
      expect(colors['toolSuccessBg']).toBe('');
      expect(colors['toolErrorBg']).toBe('');
    });

    it(`${filename} uses an explicit high-contrast output color distinct from metadata`, () => {
      const theme = loadTheme(filename);
      const vars = theme['vars'] as Record<string, unknown>;
      const colors = theme['colors'] as Record<string, unknown>;
      expect(colors['toolOutput']).toBe('ink');
      expect(typeof vars['ink']).toBe('string');
      expect(vars['ink']).not.toBe(vars['dim']);
      expect(colors['toolTitle']).not.toBe(colors['toolOutput']);
    });
  }
});

// ─── Banner rendering ─────────────────────────────────────────────────────────

describe('renderBannerLines', () => {
  it('returns at least one line', () => {
    const lines = renderBannerLines(stubTheme, 80);
    expect(lines.length).toBeGreaterThanOrEqual(1);
  });

  it('paints the static math-driven purple gradient on the wordmark', () => {
    const lines = renderBannerLines(stubTheme, 80);
    const joined = lines.join('');
    expect(joined).toContain('[mdLink:');    // lavender highlight (brightest stop reached by gradient)
    expect(joined).toContain('[accent:');    // purple brand body
    expect(joined).toContain('[toolTitle:'); // saturated purple title stop
    expect(joined).toContain('[muted:');     // violet-gray shadow tail
    expect(joined).not.toContain('[syntaxOperator:'); // no teal/cyan detour
  });

  it('includes version when provided', () => {
    const lines = renderBannerLines(stubTheme, 80, '1.2.3');
    const joined = lines.join('');
    expect(joined).toContain('1.2.3');
  });

  it('omits version string when not provided', () => {
    const lines = renderBannerLines(stubTheme, 80);
    const joined = lines.join('');
    expect(joined).not.toContain(' v');
  });

  it('truncates to width — line visible length does not exceed width', () => {
    // Use a narrow width; strip ANSI sequences to count visible chars.
    const width = 15;
    const lines = renderBannerLines(stubTheme, width, '9.9.9');
    for (const line of lines) {
      // Strip ANSI escape sequences for measurement.
      const visible = line.replace(/\x1B\[[0-9;]*m/g, '');
      // Also strip our stub markers for this measurement
      const stripped = visible.replace(/\[[\w]+:/g, '').replace(/\]/g, '').replace(/\*\*/g, '');
      expect(stripped.length).toBeLessThanOrEqual(width + 1); // +1 for ellipsis
    }
  });

  it('handles very narrow width (1 col) without throwing', () => {
    expect(() => renderBannerLines(stubTheme, 1)).not.toThrow();
  });

  it('handles zero width without throwing', () => {
    expect(() => renderBannerLines(stubTheme, 0)).not.toThrow();
  });
});

describe('renderBannerLines is static', () => {
  it('is pure in (theme, width) — byte-identical across repaints, never bold', () => {
    // The banner is a transcript entry; time-varying bytes there would
    // invalidate pi-tui's line diff for the whole scrollback on each repaint.
    const first = renderBannerLines(stubTheme, 100).join('\n');
    expect(renderBannerLines(stubTheme, 100).join('\n')).toBe(first);
    expect(first).not.toContain('**');
  });
});

describe('renderTagline', () => {
  it('returns a string', () => {
    expect(typeof renderTagline(stubTheme, 80)).toBe('string');
  });

  it('applies fg muted', () => {
    const line = renderTagline(stubTheme, 80);
    expect(line).toContain('muted');
  });

  it('truncates to width at narrow terminal', () => {
    const width = 10;
    const line = renderTagline(stubTheme, width);
    const visible = line.replace(/\x1B\[[0-9;]*m/g, '');
    const stripped = visible.replace(/\[[\w]+:/g, '').replace(/\]/g, '');
    expect(stripped.length).toBeLessThanOrEqual(width + 1);
  });
});

describe('renderBetaNotice', () => {
  it('shows the beta label and issue-tracker URL under the banner', () => {
    const lines = renderBannerWithTagline(stubTheme, 120, '1.0.0');
    const last = lines.at(-1) ?? '';
    expect(last).toContain('BETA VERSION');
    expect(last).toContain('for issues:');
    expect(last).toContain('[warning:');
    expect(last).toContain('[mdLink:');
    expect(last).not.toContain('\x1b]8;;');
  });
});

describe('renderBannerWithTagline', () => {
  it('returns banner lines followed by tagline', () => {
    const lines = renderBannerWithTagline(stubTheme, 80, '2.0.0');
    expect(lines.length).toBeGreaterThanOrEqual(2);
    const joined = lines.join('\n');
    expect(joined).toContain('2.0.0');
    expect(joined).toContain('muted');
  });
});

