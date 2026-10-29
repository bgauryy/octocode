Load only when a report needs an explorable network or a data chart. Ordinary reports use [Markdown and Mermaid](report-output.md).

# Interactive report widgets

## Content contract

Use `graph` for interactive networks; `chart` for measured bar, line, or pie charts. Fences contain JSON, never library options or JavaScript. The template validates data, loads the required renderer, supplies consistent Expand/Export controls, separate Show data and Show source views, and text tables, and keeps source available on failure. Ordinary Markdown needs no widget library. Mermaid supports find-in-diagram.

````markdown
```graph
{"title":"Dependency map","nodes":[{"id":"a","label":"App","description":"Entry point","url":"https://example.com/source"},{"id":"b","label":"API"}],"edges":[{"source":"a","target":"b","label":"calls"}]}
```
```chart
{"title":"Measured latency","type":"bar","unit":"ms","labels":["Before","After"],"series":[{"name":"Median","values":[120,80]}]}
```
````

Replace example values with evidence. Graph nodes need unique IDs and labels; edge endpoints must exist. Description and HTTP(S) evidence URL are optional. Charts accept `bar`, `line`, or `pie`; each series has a name and finite numeric values matching the labels. Pie accepts one nonnegative series. Do not duplicate the widget data in another authored table: the viewer generates its accessible data view.

