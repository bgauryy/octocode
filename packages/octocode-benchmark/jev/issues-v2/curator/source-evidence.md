# issues-v2 curator source evidence

Frozen 2026-09-18. Public source was researched only through the benchmark's
`run.mjs curator ... octocode` harness. Every case reached the eight-invocation
cap. No baseline, treatment, previous answer, or report was read.

## R37637 — React ViewTransition portal nesting

- Current React client tests say nested `enter`/`onEnter` do **not** fire when
  an ancestor mounts, while a nested boundary with `parentEnter`/
  `onParentEnter` does. That is the decisive alternative to the report's
  interpretation. [test](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-dom/src/__tests__/ReactDOMViewTransition-test.js#L485)
- The commit traversal applies normal `enter` to the placement, then traverses
  descendant `parentEnter` opt-ins. [implementation](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberCommitViewTransitions.js#L462)
- Parent-enter/exit remains experimental in the public feature configuration.
  [flag](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/shared/ReactFeatureFlags.js#L98)
- The portal PR handles root animation because portals are treated like roots;
  it does not establish automatic descendant `enter` behavior.
  [PR 32772](https://github.com/react/react/pull/32772)

Conclusion: reject a proposed recursive-enter fix. Test `parentEnter` first;
only a failure in that supported path supports a portal implementation bug.

## R37619 — React Flight Map/Set self cycles

- Server Map serialization outlines `Array.from(map)` then emits `$Q<row>`;
  Set similarly emits `$W<row>`. [source](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-server/src/ReactFlightServer.js#L3410)
- The prior Flight repair explicitly separates a genuine cycle (which receives
  a partial object to break deadlock) from a nested noncycle listener that must
  wait. Its included Set/`foo` scenario is different from a container referring
  to itself. [PR 37542](https://github.com/react/react/pull/37542)
- The open report provides an exact claimed wire shape but is not itself proof
  of current behavior. [issue](https://github.com/react/react/issues/37619)

Conclusion: likely defect with a credible outlined-row mechanism, but retain
medium confidence pending an executable current/version-pinned Flight test.

## L40592 — LangChain in-memory list limit

- The abstract interface describes `limit: int | None` as an optional limit;
  the concrete in-memory code appends results then uses truthiness rather than
  a `None` check. [source](https://github.com/langchain-ai/langchain/blob/fd4f1615359371fbb1b3b2de9183a18a15ee9e34/libs/core/langchain_core/indexing/base.py#L352)
- Existing unit coverage covers normal key listing but did not establish the
  zero case in the retrieved version. [tests](https://github.com/langchain-ai/langchain/blob/fd4f1615359371fbb1b3b2de9183a18a15ee9e34/libs/core/tests/unit_tests/indexing/test_in_memory_record_manager.py)
- The linked implementation PR is unaccepted/closed context, not repair proof.
  [issue](https://github.com/langchain-ai/langchain/issues/40592)

Conclusion: `if limit is not None: return result[:limit]`, with both sync and
async zero-limit tests, is the narrow safe repair.

## L40590 — LangChain ChatGroq n

- Current validation rejects `n < 1` and only rejects `n > 1` when streaming.
  [validator](https://github.com/langchain-ai/langchain/blob/fd4f1615359371fbb1b3b2de9183a18a15ee9e34/libs/partners/groq/langchain_groq/chat_models.py#L540)
- The default request params include `n: self.n`, so a nonstreaming invalid
  value is forwarded. [params](https://github.com/langchain-ai/langchain/blob/fd4f1615359371fbb1b3b2de9183a18a15ee9e34/libs/partners/groq/langchain_groq/chat_models.py#L814)

Conclusion: locally reject all `n != 1`; test both streaming modes without a
provider request.

## O151639 — OpenClaw Telegram/MiniMax thinking output

- Current documentation explicitly supports MiniMax-M3 on Anthropic-compatible
  streaming and says M3 uses omitted/adaptive thinking by default. It is not an
  unsupported-environment conclusion. [provider docs](https://github.com/openclaw/openclaw/blob/34c8ae00b678330a2bf531687efaae49e85e248c/docs/concepts/model-providers/custom-providers.md#L192)
- Project history includes related fixes for richer stream text and stripping
  thinking tags, but neither is a trace of this exact Telegram serialization
  claim. [changelog](https://github.com/openclaw/openclaw/blob/34c8ae00b678330a2bf531687efaae49e85e248c/CHANGELOG/2026.2.13.md#L43)

Conclusion: no safe patch can be specified. Require a redacted payload and an
end-to-end trace through normalizer, streamed payload, Telegram renderer, and
session persistence.

## O151637 — OpenClaw command-palette layout shift

- Current markup conditionally inserts a `role=status` `cmd-palette__empty`
  between the input and result container, and the footer follows that container.
  [markup](https://github.com/openclaw/openclaw/blob/34c8ae00b678330a2bf531687efaae49e85e248c/ui/src/components/command-palette.ts#L225)
- CSS gives the normal-flow status node flex layout and 16px vertical padding;
  results have a capped scroll container. [styles](https://github.com/openclaw/openclaw/blob/34c8ae00b678330a2bf531687efaae49e85e248c/ui/src/styles/components.css#L4896)
- Pending tests protect announcements, `aria-busy`, empty/error distinctions,
  stale-request handling, and usable static commands, but jsdom does not prove
  pixel geometry. [tests](https://github.com/openclaw/openclaw/blob/34c8ae00b678330a2bf531687efaae49e85e248c/ui/src/components/command-palette-pending.test.ts#L35)
- The pending status came from merged [PR 146938](https://github.com/openclaw/openclaw/pull/146938); the current issue has an explicit maintainer batch hold.

Conclusion: confirmed layout defect, but defer an independent patch. A future
owner must preserve status/accessibility and prove stable geometry in browser.
