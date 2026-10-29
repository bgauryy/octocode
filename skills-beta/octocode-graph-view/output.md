# Output

BLUF per the [protocol catalog](references/protocol-catalog.md#catalog): lead with the verdict (what the graph shows, which leads survived the evidence layers), then facts (each beside its `data.json` field or `path:line`), assumptions (layers inferred from names, clasify probabilities, unresolved leads), unknowns (layers skipped, runtime flows not machine-checked). Report the token table (source vs brief), the OCR model and wall time, and the clasify model.

## Saved result

`<workspace>/.octocode/octocode-graph-view/<YYYY-MM-DD>-<slug>/` holds `report.html`, `brief.txt` (short default), `brief-full.txt`, `data.json`, `verify.json`, `report-check.json`, `measure-*.json`, optional `model.json`, and the shots. Scratch lives in its `tmp/`.
