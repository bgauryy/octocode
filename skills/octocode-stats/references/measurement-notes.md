# Measurement Notes

Read this before interpreting or describing dashboard numbers — they are approximations from the response pipeline, not exact ledgers.

- Token counts are estimates: `estimatedTokensSaved = savedChars / 4`. Always describe them as approximate.
- `charsSavedByTool` is cumulative response-savings data, not a recent activity log and not a complete per-tool call ledger.
- Tool response savings may exclude zero-savings tools from visual breakdowns, while total measured calls can still include those calls.
- `rawChars`, `responseChars`, and `savedChars` are measurements from the response pipeline, not a perfect partition where `raw = response + saved`.
- `responseChars` is the final tool response after output pagination; `rawChars` counts upstream API/command/file payloads that were actually fetched or read.
- For paginated upstream APIs, only pages that were actually requested are counted. Later pages appear in stats when those tool calls run.
- Avoid invented comparisons like pages, words, books, or money unless the script explicitly computes them.
