/**
 * Detects which host the octocode engine is running under, so LSP server
 * resolution can prefer the editor's own servers when hosted in an IDE and
 * fall back to standalone discovery in a plain terminal.
 *
 * Signals are environment variables the editor sets deliberately (see
 * jonschlinkert/detect-terminal and VS Code's `terminalEnvironment.ts`).
 * `TERM_PROGRAM` is the primary signal; the vendor `*_PID` / IPC-handle vars
 * corroborate it. `TERM` is intentionally ignored — it is terminfo, not an
 * editor identity.
 */
export type IdeHost =
  | 'vscode'
  | 'cursor'
  | 'windsurf'
  | 'zed'
  | 'jetbrains'
  | 'terminal'
  | 'unknown';

export interface IdeContext {
  host: IdeHost;
  /** True when running inside an editor that ships language servers we could reuse. */
  isIde: boolean;
}

function vscodeFork(env: NodeJS.ProcessEnv): IdeHost {
  // Cursor / Windsurf are VS Code forks and also report TERM_PROGRAM=vscode;
  // disambiguate on their vendor markers before defaulting to upstream VS Code.
  if (Object.keys(env).some(key => key.startsWith('CURSOR'))) return 'cursor';
  if (
    'WINDSURF_PID' in env ||
    Object.keys(env).some(key => key.startsWith('WINDSURF') || key.startsWith('CODEIUM'))
  ) {
    return 'windsurf';
  }
  return 'vscode';
}

export function detectIdeContext(
  env: NodeJS.ProcessEnv = process.env
): IdeContext {
  const termProgram = (env.TERM_PROGRAM ?? '').toLowerCase();

  if (termProgram === 'vscode' || 'VSCODE_PID' in env || 'VSCODE_GIT_IPC_HANDLE' in env) {
    const host = vscodeFork(env);
    return { host, isIde: true };
  }
  if (termProgram === 'zed' || 'ZED_TERM' in env) {
    return { host: 'zed', isIde: true };
  }
  // JetBrains does not set TERM_PROGRAM; its tell is the JediTerm emulator.
  if (env.TERMINAL_EMULATOR === 'JetBrains-JediTerm' || 'IDEA_INITIAL_DIRECTORY' in env) {
    return { host: 'jetbrains', isIde: true };
  }
  if (termProgram === 'apple_terminal' || termProgram === 'iterm.app' || 'WT_SESSION' in env) {
    return { host: 'terminal', isIde: false };
  }
  return { host: 'unknown', isIde: false };
}
