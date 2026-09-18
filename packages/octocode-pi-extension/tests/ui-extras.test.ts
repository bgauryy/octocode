import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  formatCompact,
  formatDurationShort,
  buildWorkingIndicator,
  buildCapabilitySegments,
  getFooterDensity,
  setFooterDensity,
  parseFooterDensity,
  resolveSystemTheme,
  resolveSystemThemeName,
  deriveSessionName,
  OCTOCODE_SPINNER_FRAMES,
  OCTOCODE_THEME_DARK,
  OCTOCODE_THEME_LIGHT,
} from '../src/ui-extras.js';

test('formatCompact abbreviates thousands and millions', () => {
  assert.equal(formatCompact(950), '950');
  assert.equal(formatCompact(1234), '1.2k');
  assert.equal(formatCompact(45_000_000), '45M');
});

test('formatDurationShort renders s / m s / h m', () => {
  assert.equal(formatDurationShort(0), '0s');
  assert.equal(formatDurationShort(12_000), '12s');
  assert.equal(formatDurationShort(63_000), '1m 3s');
  assert.equal(formatDurationShort(3_600_000 + 120_000), '1h 2m');
  assert.equal(formatDurationShort(undefined), '—');
});

test('spinner frames are non-empty and richer than a static pulse', () => {
  assert.ok(OCTOCODE_SPINNER_FRAMES.length >= 6);
  assert.ok(new Set(OCTOCODE_SPINNER_FRAMES).size >= 4);
});

test('buildWorkingIndicator paints spinner frames with a fast semantic color pulse', () => {
  const calls: Array<[string, string]> = [];
  const indicator = buildWorkingIndicator({
    fg: (color: string, text: string) => {
      calls.push([color, text]);
      return `<${color}:${text}>`;
    },
    bold: (text: string) => text,
  });

  assert.equal(indicator.intervalMs, 120);
  assert.equal(indicator.frames.length, OCTOCODE_SPINNER_FRAMES.length);
  // Brand-metallic pulse: teal tick then lavender→white shimmer — never
  // warning/success, which would read as status changes.
  assert.equal(indicator.frames[0], '<accent:✦>');
  assert.equal(indicator.frames[2], '<text:✶>');
  assert.equal(indicator.frames[3], '<mdLink:✺>');
  assert.deepEqual(calls.slice(0, 4), [
    ['accent', '✦'],
    ['mdLink', '✧'],
    ['text', '✶'],
    ['mdLink', '✺'],
  ]);
});

test('theme name constants are the shipped theme ids', () => {
  assert.equal(OCTOCODE_THEME_DARK, 'octocode-dark');
  assert.equal(OCTOCODE_THEME_LIGHT, 'octocode-light');
});

test('resolveSystemTheme maps macOS appearance to our themes', () => {
  assert.equal(resolveSystemTheme('Dark'), OCTOCODE_THEME_DARK);
  assert.equal(resolveSystemTheme(''), OCTOCODE_THEME_LIGHT); // AppleInterfaceStyle unset => light
  assert.equal(resolveSystemTheme(null), OCTOCODE_THEME_LIGHT);
});

test('resolveSystemThemeName: macOS resolves from AppleInterfaceStyle (always decidable)', () => {
  assert.equal(
    resolveSystemThemeName({ platform: 'darwin', appleInterfaceStyle: 'Dark' }),
    OCTOCODE_THEME_DARK
  );
  assert.equal(
    resolveSystemThemeName({ platform: 'darwin', appleInterfaceStyle: '' }),
    OCTOCODE_THEME_LIGHT
  );
  assert.equal(
    resolveSystemThemeName({
      platform: 'darwin',
      appleInterfaceStyle: undefined,
    }),
    OCTOCODE_THEME_LIGHT
  );
});

test('resolveSystemThemeName: non-macOS uses COLORFGBG background heuristic', () => {
  // COLORFGBG is "fg;bg"; bg 0-6 = dark terminal, 7/15 = light.
  assert.equal(
    resolveSystemThemeName({ platform: 'linux', colorfgbg: '15;0' }),
    OCTOCODE_THEME_DARK
  );
  assert.equal(
    resolveSystemThemeName({ platform: 'linux', colorfgbg: '0;15' }),
    OCTOCODE_THEME_LIGHT
  );
  assert.equal(
    resolveSystemThemeName({ platform: 'linux', colorfgbg: '0;7' }),
    OCTOCODE_THEME_LIGHT
  );
});

test('resolveSystemThemeName: returns null when undetectable (keep current theme)', () => {
  assert.equal(resolveSystemThemeName({ platform: 'linux' }), null);
  assert.equal(
    resolveSystemThemeName({ platform: 'win32', colorfgbg: 'garbage' }),
    null
  );
  assert.equal(
    resolveSystemThemeName({ platform: 'linux', colorfgbg: '' }),
    null
  );
});

test('deriveSessionName cleans and truncates the first line', () => {
  assert.equal(
    deriveSessionName('  Fix the auth  bug\nmore'),
    'Fix the auth bug'
  );
  assert.equal(deriveSessionName(''), '');
  const long = deriveSessionName('x'.repeat(80));
  assert.ok(long.length <= 48);
  assert.match(deriveSessionName('add feature'.repeat(20)), /…$/);
});

test('capability diagnostics keep estimates separate and obey density', () => {
  const metrics = {
    dial: 'deep',
    overhead: {
      totalChars: 4000,
      sysChars: 2000,
      mcpServers: 3,
      mcpTools: 18,
      skills: 13,
    },
  };
  assert.deepEqual(buildCapabilitySegments(metrics, 'compact'), []);
  const normal = buildCapabilitySegments(metrics, 'default')
    .map(segment => segment.text)
    .join(' | ');
  assert.match(normal, /initial ~1\.0k/);
  assert.match(normal, /mcp 3 · skills 13/);
  assert.match(normal, /dial deep/);
  assert.doesNotMatch(normal, /session |turn |context |perm |agents /);
  const expanded = buildCapabilitySegments(metrics, 'full')
    .map(segment => segment.text)
    .join(' | ');
  assert.match(expanded, /sys 500 · mcp 3\/18 · skills 13/);
  assert.deepEqual(buildCapabilitySegments({}), []);
});

test('footer density follows configuration and rejects unknown modes', () => {
  try {
    setFooterDensity('compact');
    assert.equal(getFooterDensity(), 'compact');
    assert.deepEqual(buildCapabilitySegments({ dial: 'deep' }), []);
  } finally {
    setFooterDensity('default');
  }
  assert.equal(parseFooterDensity('full'), 'full');
  assert.equal(parseFooterDensity(' Compact '), 'compact');
  assert.equal(parseFooterDensity('bogus'), undefined);
  assert.equal(parseFooterDensity(''), undefined);
});
