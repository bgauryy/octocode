import { describe, expect, it } from 'vitest';
import { BODY, mask, unhide } from '../src/permission/shell-parse.js';

/** The masked command with quoted operators shifted back, and the heredoc bodies it collected. */
function masked(command: string): { text: string; bodies: string[] } {
  const bodies: string[] = [];
  return { text: unhide(mask(command, bodies)), bodies };
}

describe('mask', () => {
  it('keeps quoted text as one word whose operators split nothing', () => {
    expect(mask("x 'y;z' \"a|b\"", [])).not.toMatch(/[;|]/);
    expect(masked("x 'y;z' \\\nw")).toEqual({ text: 'x y;z  w', bodies: [] });
    // An empty quoted string is still a word.
    expect(masked("echo '' \"\"")).toEqual({ text: 'echo    ', bodies: [] });
  });

  it('checks the substitutions inside double quotes as separate commands', () => {
    expect(masked('a "b $(c "d") e" f')).toEqual({ text: 'a b  e;c d; f', bodies: [] });
  });

  it('replaces each heredoc by a body marker and collects the bodies in order', () => {
    const { text, bodies } = masked('cat <<A <<B\n1\nA\n2\nB\nend');
    expect(text).toBe('cat  \u00020   \u00021 \nend');
    expect(bodies).toEqual(['1', '2']);
    expect(BODY.exec('\u00021')?.[1]).toBe('1');
    // A herestring is not a heredoc.
    expect(masked('a <<<b')).toEqual({ text: 'a <<<b', bodies: [] });
  });

  it('runs the substitutions of an unquoted heredoc body, not of a quoted one', () => {
    expect(masked('cat <<EOF\n$(rm x)\nEOF\nls')).toEqual({ text: 'cat  \u00020 \n;rm x;ls', bodies: ['$(rm x)'] });
    expect(masked("cat <<'EOF'\n$(rm x)\nEOF")).toEqual({ text: 'cat  \u00020 \n', bodies: ['$(rm x)'] });
    expect(masked('cat <<-EOF\n\t`id`\n\tEOF')).toEqual({ text: 'cat  \u00020 \n;id;', bodies: ['\t`id`'] });
  });

  it('gives an unterminated heredoc the rest of the command', () => {
    expect(masked('cat <<EOF\nno end')).toEqual({ text: 'cat  \u00020 \n', bodies: ['no end'] });
  });
});
