import { describe, expect, it } from 'vitest';
import { resultBlock, resultText, toolHeader } from '../src/shared/render.js';
import { sanitizeTerminalText } from '../src/shared/sanitize.js';
import { rendered, theme } from './fake-pi.js';

describe('sanitizeTerminalText', () => {
  const cases: Array<[string, string, string]> = [
    ['plain text is unchanged', 'hello world', 'hello world'],
    ['keeps newlines and tabs', 'a\n\tb', 'a\n\tb'],
    ['folds CRLF and lone CR into LF', 'a\r\nb\rc', 'a\nb\nc'],
    ['strips CSI colour and cursor codes', '\u001b[31mred\u001b[0m \u001b[2J\u001b[10;5H!', 'red !'],
    ['strips OSC title (BEL)', '\u001b]0;pwned\u0007after', 'after'],
    ['strips OSC 8 links (ST)', '\u001b]8;;https://evil.test\u001b\\click\u001b]8;;\u001b\\', 'click'],
    ['strips an unterminated OSC to the end', 'ok\u001b]52;c;Zm9v', 'ok'],
    ['strips DCS strings', 'x\u001bPqpayload\u001b\\y', 'xy'],
    ['strips 8-bit CSI', 'a\u009b31mb', 'ab'],
    ['strips lone ESC sequences', 'a\u001bcb\u001b(Bc', 'abc'],
    ['strips C0 controls and DEL', 'a\u0000b\u0007c\u0008d\u007fe', 'abcde'],
    ['strips bidi overrides and isolates', 'file\u202egnp.exe\u2066x\u2069', 'filegnp.exex'],
    ['turns line/paragraph separators into LF', 'a\u2028b\u2029c', 'a\nb\nc'],
    ['keeps other unicode', 'café 日本 🚀', 'café 日本 🚀'],
    ['strips Unicode tag characters', 'ok\u{e0001}\u{e0069}\u{e0067}\u{e006e}\u{e007f}!', 'ok!'],
    ['strips zero-width characters, word joiner and BOM', '\ufeffa\u200bb\u200cc\u200dd\u2060e\u2063f', 'abcdef'],
    ['strips bidi embeddings, overrides and isolates', 'a\u202ab\u202bc\u202cd\u202de\u2067f\u2068g', 'abcdefg'],
    ['keeps a ZWJ that joins emoji', '👩‍💻 👩🏽‍💻 ❤️‍🔥 👨‍👩‍👧', '👩‍💻 👩🏽‍💻 ❤️‍🔥 👨‍👩‍👧'],
    ['strips a ZWJ next to plain text', 'x\u200d👩 👩\u200dy', 'x👩 👩y'],
  ];
  it.each(cases)('%s', (_name, input, expected) => {
    expect(sanitizeTerminalText(input)).toBe(expected);
  });

  it('is applied by the shared renderers that draw subagent and MCP text', () => {
    expect(resultText({ content: [{ type: 'text', text: '\u001b]0;title\u0007ok\u001b[1m!' }] })).toBe('ok!');
    expect(rendered(resultBlock(theme, { expanded: true }, { summary: '\u001b]0;t\u0007done', body: '\u001b[31mred\u001b[0m\n\u202eevil' }))).toBe('  ⎿  done\n     red\n     evil');
    expect(rendered(toolHeader(theme, {}, 'Mcp', '\u001b[2Jx\u202ey'))).toBe('○ Mcp(xy)');
  });
});
