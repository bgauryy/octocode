# BUG-09 Baseline — Vite HMR queueUpdate wrong callback order

## Root cause

**H2 is correct.** From `packages/vite/src/shared/hmr.ts`, `queueUpdate`:

```ts
public async queueUpdate(payload: Update): Promise<void> {
  this.updateQueue.push(this.fetchUpdate(payload))   // ← Promise started immediately
  if (!this.pendingUpdateQueue) {
    this.pendingUpdateQueue = true
    await Promise.resolve()                            // ← yields to microtask queue
    this.pendingUpdateQueue = false                    // ← flag cleared BEFORE awaiting Promise.all
    const loading = [...this.updateQueue]
    this.updateQueue = []
    ;(await Promise.all(loading)).forEach((fn) => fn && fn())
  }
}
```

The race condition:

1. Update A arrives: `updateQueue = [fetchUpdate(A)]`, `pendingUpdateQueue = true`, yields
2. Update B arrives during the yield: `updateQueue = [fetchUpdate(A), fetchUpdate(B)]`, but `pendingUpdateQueue = true` so the if-block is skipped
3. First microtask fires: `pendingUpdateQueue = false`, `loading = [fetchUpdate(A), fetchUpdate(B)]`, both cleared from queue
4. Update C arrives (second rapid save) DURING `await Promise.all(loading)`: `pendingUpdateQueue` is now `false`, so a NEW flush cycle starts IMMEDIATELY — it snapshots the current queue into `loading2`, awaits `Promise.all(loading2)` in parallel with the first
5. If C's fetch completes faster, C's callback fires **before** B's callback

The fundamental issue: `pendingUpdateQueue = false` is set BEFORE `await Promise.all(loading)` completes, creating a window where a new flush cycle races with the existing one.

## Fix proposal

Set `pendingUpdateQueue = false` only AFTER `Promise.all` resolves:

```ts
public async queueUpdate(payload: Update): Promise<void> {
  this.updateQueue.push(this.fetchUpdate(payload))
  if (!this.pendingUpdateQueue) {
    this.pendingUpdateQueue = true
    await Promise.resolve()
    const loading = [...this.updateQueue]
    this.updateQueue = []
    try {
      ;(await Promise.all(loading)).forEach((fn) => fn && fn())
    } finally {
      this.pendingUpdateQueue = false
      // If new updates arrived during the flush, process them
      if (this.updateQueue.length > 0) {
        this.queueUpdate(this.updateQueue[0])  // re-trigger
      }
    }
  }
}
```

## Evidence

- `packages/vite/src/shared/hmr.ts`: `queueUpdate` implementation — confirmed via `ghGetFileContent`; `pendingUpdateQueue = false` at line before `Promise.all` is the race window
- `packages/vite/src/shared/hmrHandler.ts`: outer queue serializes WS messages but not the `fetchUpdate` completions — confirmed separate concern

## Confidence

**Medium** — the race window is clear in source. The exact repro requires specific network timing for the rapid saves, but the structural defect is deterministic.
