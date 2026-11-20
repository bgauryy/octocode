# Flows: named, parametrized plans

Load when the same browser sequence repeats (sign in, fill a form, extract links, paginate, screenshot, audit). A flow is a plan with `{{param}}` placeholders; `flow run` expands it, validates the expanded plan, then executes it through the same executor as `run`.

## Commands

| Call                                               | Result                                                                                                                                                                  |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `flow list`                                        | Every flow: name, description, params, `source` (`built-in` or `workspace`), `expects`; unreadable saved files appear under `invalid`                                   |
| `flow show <name> [--params JSON]`                 | The definition, the template, and the expanded `plan` for those params, plus `missing` and `valid` (real plan validator). Nothing runs; secrets show as `[secret:name]` |
| `flow run <name> --params JSON [connection flags]` | Validates params and the expanded plan before connecting, then runs it; the reply is the run's own reply plus `[FLOW] {"flow","expects"}`                               |
| `flow save <name> --plan file\|-`                  | Stores a plan (stdin or file) as `<workspace>/.octocode/octocode-chrome-devtools/flows/<name>.json`, mode 0600                                                          |

```bash
octocode-chrome-devtools /raw flow show login-form --params '{"userLabel":"User","userValue":"bob","passwordLabel":"Password","passwordValue":"pw","submitName":"Sign in","successText":"Welcome"}'
octocode-chrome-devtools /raw flow run extract-links --params '{"url":"https://example.com/"}' --port 9222
```

## Definition

`{name, description, params: {key: {type: string|number|boolean, required?, default?, secret?, description}}, plan, repeat?, expects}`.

- Placeholders `{{key}}` are replaced inside string values only. A placeholder in an object key is refused. A number-typed param is text inside a string, so a numeric plan field (`timeoutMs`) cannot take one: the expanded plan fails validation.
- Params are checked first: an unknown name, a wrong type or a missing required param is refused before any connection.
- A step that names only absent optional params is dropped (an optional `url` skips the `goto`). A step naming some absent and some present params is an error, so a `fill-form` pair needs both halves.
- `repeat: {param, offset?, steps}` appends `steps` `params[param] + offset` times; `paginate-list` uses it for `maxPages`.
- `secret: true` (strings, no default): never printed by `list`, `show`, replies or errors, and scrubbed from the files a run saved (below).

## Built-ins

| Flow                    | Required params                                                                      | Optional (default)                                          |
| ----------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| `login-form`            | userLabel, userValue, passwordLabel, passwordValue (secret), submitName, successText | url                                                         |
| `fill-form`             | field1Label, field1Value                                                             | url, field2..6 Label/Value (pairs), submitName, successText |
| `extract-links`         | none                                                                                 | url, selector (`a`)                                         |
| `paginate-list`         | itemSelector, nextName                                                               | url, nextRole (`link`), maxPages (5)                        |
| `full-page-screenshot`  | none                                                                                 | url                                                         |
| `console-network-audit` | url                                                                                  | none                                                        |

Fields are matched as textboxes by accessible name. `paginate-list` clicks next as an optional step: when no next control remains the click is skipped (listed in `failedOptionalSteps`) and later extracts repeat the last page. `console-network-audit` observes network (`observe.network`) and saves Runtime console and exception events plus a snapshot and a screenshot.

## Saving

`save` turns each distinct `{{name}}` into a required string param. Names containing pass, secret, token, api key or credential become `secret`. The plan must validate with every placeholder replaced by a sample string. A name equal to a built-in is refused; an existing workspace flow is never overwritten (delete its file manually). Names follow `[A-Za-z0-9][A-Za-z0-9._-]{0,63}`.

## Secrets: what is and is not covered

The executor saves the complete expanded plan (`browser-plan.json`) and may echo steps in its journal, so a typed password would persist there. `flow run` therefore rewrites every JSON/text file in the run folder after the run and scrubs stdout and stderr, replacing each secret (raw, JSON-escaped, URL-encoded) with `[REDACTED]`. Not covered: the page itself (the value was typed into it), the process argument list holding `--params` while the command runs, and a secret shorter than a few characters, which scrubs everything that contains it.
