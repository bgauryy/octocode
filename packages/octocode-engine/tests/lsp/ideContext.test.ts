import { describe, expect, it } from 'vitest';

import { detectIdeContext } from '../../src/lsp/ideContext.js';

describe('detectIdeContext', () => {
  it('detects VS Code via TERM_PROGRAM', () => {
    expect(detectIdeContext({ TERM_PROGRAM: 'vscode' })).toEqual({
      host: 'vscode',
      isIde: true,
    });
  });

  it('detects VS Code via VSCODE_PID even without TERM_PROGRAM', () => {
    expect(detectIdeContext({ VSCODE_PID: '123' })).toEqual({
      host: 'vscode',
      isIde: true,
    });
  });

  it('disambiguates Cursor from upstream VS Code', () => {
    expect(
      detectIdeContext({ TERM_PROGRAM: 'vscode', CURSOR_TRACE_ID: 'x' })
    ).toEqual({ host: 'cursor', isIde: true });
  });

  it('disambiguates Windsurf', () => {
    expect(
      detectIdeContext({ TERM_PROGRAM: 'vscode', WINDSURF_PID: '9' })
    ).toEqual({ host: 'windsurf', isIde: true });
  });

  it('detects Zed', () => {
    expect(detectIdeContext({ TERM_PROGRAM: 'zed' })).toEqual({
      host: 'zed',
      isIde: true,
    });
  });

  it('detects JetBrains via the JediTerm emulator', () => {
    expect(
      detectIdeContext({ TERMINAL_EMULATOR: 'JetBrains-JediTerm' })
    ).toEqual({ host: 'jetbrains', isIde: true });
  });

  it('classifies a plain terminal as not-an-IDE', () => {
    expect(detectIdeContext({ TERM_PROGRAM: 'Apple_Terminal' })).toEqual({
      host: 'terminal',
      isIde: false,
    });
  });

  it('returns unknown when no signal is present', () => {
    expect(detectIdeContext({})).toEqual({ host: 'unknown', isIde: false });
  });

  it('ignores TERM (terminfo is not an editor identity)', () => {
    expect(detectIdeContext({ TERM: 'xterm-256color' })).toEqual({
      host: 'unknown',
      isIde: false,
    });
  });
});
