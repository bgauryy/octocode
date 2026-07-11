/** Shared shorthand types used across the `search` command's argv-resolution modules. */

export type CliShorthandCorpus =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo: string; path?: string; ref?: string }
  | { kind: 'npm' };

export type CliSearchShorthand = Record<string, unknown> & {
  corpus: CliShorthandCorpus;
};

export type Resolved = { input: unknown } | { error: string } | undefined;

export interface GithubDiffShortcut {
  corpus: CliShorthandCorpus;
  baseRef: string;
  headRef: string;
  path: string;
}
