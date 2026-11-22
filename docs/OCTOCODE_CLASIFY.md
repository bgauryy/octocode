# Classify files and search results

Use `clasify` to answer a question about a file, image or web page, or to filter results by meaning. For example, search for “test”, then keep only passages that give a runnable test command.

## Quick start

Save your provider key with `octocode config set OCTOCODE_CLASSIFICATION_API --stdin` and restart your MCP client. Run `octocode schema clasify --view variants` to see the available operations. Discovery is free. [Key setup](AUTHENTICATION.md#classification-key-clasify) · [Change the provider](CONFIGURATION.md#change-the-classification-host)

From a project containing a README, ask one question with a budget of one provider request.

```bash
octocode clasify '{
  "queries": [{
    "operation": "evaluate", "maxRequests": 1,
    "source": {"tool": "localFetch", "query": {"path": "README.md"}},
    "questions": {
      "tests": {"type": "noul", "instructions": "Does the README give a shell command for running the test suite?"}
    }
  }]
}'
```

### Choose a source

| Evidence you have | What to pass |
|---|---|
| A known file or image | Use a `localFetch` or `ghGetFileContent` query as `source`. Pin GitHub reads with `ref`. Use ranges for text and the whole file for PNG or JPEG. |
| A public web page | Set `source` to `{"url":"https://example.org/guide"}`. A fragment selects one HTML section. |
| Search or listing results | Pass one supported tool's query row as `source.query`. Run `octocode schema clasify --view query` for the source types. |
| Text that refers to an image | Put the image in `assets` and name its key in the text. |

A fresh evaluation sends your questions and selected evidence to the configured provider. A `noul` answer estimates the probability of yes. Use `next.read` when offered to open the source lines behind the answer. For an image, the provider receives its pixels. [Privacy and provider settings](SECURITY.md#classification-egress)

## Filter search hits

```bash
octocode clasify '{
  "queries": [{
    "operation": "evaluate", "maxRequests": 1,
    "source": {"tool": "localSearch", "query": {
      "path": "README.md",
      "matchString": ["test"],
      "contextLines": {"before": 2, "after": 2}
    }},
    "predicates": ["This passage gives a command to run the test suite."]
  }]
}'
```

Search-hit scores appear under `files[path][line]`. The keys `p1`, `p2` and so on follow the order of your predicates. A low score can help you prioritize reading. Other row-based sources, such as a list of paths, return answers under `units`.

<a id="clasify-sections-map-questions-across-many-files"></a>

## Map questions across files

Use `sections` when you need to see where an answer occurs across a set of files. Each section has source lines. Check `coverage` for sections still unread and use the returned continuation to reach them.

```bash
octocode clasify '{
  "queries": [{
    "operation": "sections",
    "resources": {"guides": {"tool": "localFetch", "query": {"path": "docs/*.md"}}},
    "maxRequests": 10,
    "questions": {
      "login": {"type": "noul", "instructions": "Does this passage give a command to sign in to GitHub?"}
    }
  }]
}'
```

<a id="from-a-pipe-cli"></a>

## From a pipe (CLI)

```bash
git log -5 --oneline | octocode clasify --content - '{"queries":[{"operation":"evaluate","maxRequests":1,"questions":{"docs":{"type":"noul","instructions":"Does any commit mention documentation?"}}}]}'
```

Use one query and let stdin supply its content. Save longer output in a workspace file and use it as a source when you need to revisit the underlying lines.

## Billing

Fresh evaluations can bill, including repeats. Reading saved answers with `next.details` or `next.quiet` makes no new provider requests. A continuation that reaches new source material can bill. A replay marked `replayed: true` can show historical usage.

### Budget (`maxRequests`)

Set `maxRequests` on a query to limit provider requests. A search or listing carries the remaining allowance across its issued cursors. `usage.calls` counts requests for a page and `usage.walkCalls` gives the running count for that walk. Separate fresh calls have separate allowances. For a `sections` plan the cap applies per page.

If a page needs more requests than remain, inspect `classificationOverBudget`. Narrow the source or use `next.confirm` if you want to pay for the larger request. [Read paged results](TOOL_DATA_CONTRACT.md#executable-continuations)
