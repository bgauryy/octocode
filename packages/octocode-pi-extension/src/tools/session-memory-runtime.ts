import type { PiContext } from '../types.js';
import { paintUi } from '../tui/palette.js';
import type { SessionArtifactContext } from './session-artifacts.js';
import {
  readSessionMemoryState,
  type SessionMemoryReadState,
} from './session-memory.js';
import { setManagedStatus } from './runtime-renderer.js';

export function sessionMemoryStatusText(
  state: SessionMemoryReadState
): string | undefined {
  if (state.state === 'invalid') {
    return `Session memory invalid · ${state.issues.length} policy issue${state.issues.length === 1 ? '' : 's'}`;
  }
  if (state.state === 'unavailable') return 'Session memory unavailable';
  return undefined;
}

/** Read prompt memory and keep its health visible without exposing filesystem errors. */
export function readSessionMemoryForContext(
  ctx: PiContext | undefined,
  artifacts: SessionArtifactContext
): string | undefined {
  const state = readSessionMemoryState(artifacts);
  const status = sessionMemoryStatusText(state);
  setManagedStatus(
    ctx,
    'octocode-session-memory',
    status ? paintUi(ctx?.ui, 'warning', status) : undefined
  );
  return state.content;
}
