# Research examples

| Ask your agent | How to check the answer |
|---|---|
| Who calls this function? | Find its declaration with `localSearch` or `astSearch`. Give `lspSearch` the path, symbol and line, then open a returned call site with `localFetch`. |
| Why did this change land? | Find the pull request with `ghSearchHistory`. Open it with `ghGetHistoryItem` and read the discussion alongside the patch. |
| Does our installed version support this option? | Give `artifactSearch` the version from the lockfile. Open the package source at the returned revision and read the relevant declaration. |
| Which of these documents discuss retries? | Give `clasify` the documents and a focused question. Follow the source-read call to check a passage. |

If an answer depends on every match, follow the remaining pages and check which files or revisions the search covered. [Tool reference](OCTOCODE_TOOLS.md) · [Read paged results](TOOL_DATA_CONTRACT.md#executable-continuations)
