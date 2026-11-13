# Runnable stream failure and retry scope

## Goal

Investigate the following scenario: A multi-stage runnable streams partial output and a later stage raises an exception.

## Questions

- What happens when a LangChain runnable fails after emitting partial streamed output?
- How do retry and fallback boundaries affect already-emitted chunks and retained state?
- Can retrying a stage versus the entire composition duplicate output or side effects?
