import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { errorMessage } from '../shared/util.js';
import { teamWorkspace } from '../team/store.js';
import { projectTrustNow } from '../shared/trust.js';
import type { Identity } from './launch.js';
import { createWorktree, finishWorktree, isolationReport, type Worktree } from './worktree.js';

/** An isolated run's private worktree; `finish` commits its changes to the agent's ref and removes it, once, and returns the text for the report. */
export class Isolation {
  worktree: Worktree | undefined;

  constructor(private readonly id: string) {}

  /** Creates the worktree and returns the child's cwd. The worktree is its own git root: the child stays on this team. */
  async start(ctx: Pick<ExtensionContext, 'cwd' | 'isProjectTrusted'>, identity: Identity): Promise<string> {
    this.worktree = await createWorktree(ctx.cwd, this.id);
    identity.workspace = teamWorkspace(ctx.cwd);
    // The worktree is a different root, so no stored decision matches it: hand down only a decision this session made.
    if (projectTrustNow(ctx) === true) identity.trustRoot = identity.workspace;
    return this.worktree.cwd;
  }

  finish(): Promise<string> {
    const worktree = this.worktree;
    return worktree
      ? finishWorktree(worktree).then(
          (result) => isolationReport(this.id, result),
          (error: unknown) => isolationReport(this.id, { error: errorMessage(error) }),
        )
      : Promise.resolve('');
  }
}
