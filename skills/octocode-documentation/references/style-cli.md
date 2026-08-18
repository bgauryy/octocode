# Commands And Placeholders

Load when documenting a command line, its syntax, its placeholders, or its output.

## Syntax notation

| Notation | Meaning |
|---|---|
| `[FLAG]` | optional — one set of brackets per optional item |
| `{a\|b}` | choose exactly one |
| `ARG...` | repeatable, three dots, no spaces |

- Wrap lines over 80 characters with a four-space continuation indent. Every line except the last must end with the continuation character — `\` on Linux and Cloud Shell, `^` on Windows — or the command doesn't run.
- Click-to-copy blocks must not contain `[]`, `{}`, `|`, or `...`, because the reader can't paste them as-is. Options: drop the optional arguments, give each option its own block, split them into separate tasks, or tell the reader the command contains optional arguments.
- Multi-line input blocks start each line with the prompt symbol; single-line commands may show it, and if the page has both, use it everywhere. Never show the current directory before the prompt. Change the prompt indicator when the context changes (`shell@ $`).
- Document flags with end punctuation only for complete sentences; single words and noun phrases go unpunctuated unless the list mixes both.
- Terminology: command, command group, flag (the Google Cloud term), argument; "option" is the informal catchall; `--` separates tool arguments from user arguments. Don't map a tool's commands onto Linux equivalents.
- Signal names are exact: `SIGKILL`, `SIGTERM`, `SIGINT` — don't paraphrase them as stop, cancel, exit, or quit.
- Link to the full command reference instead of restating every flag.

## Placeholders

- `UPPERCASE_WITH_UNDERSCORES`: `PROJECT_ID`, `INSTANCE_NAME`, `REGION`, `API_NAME`, `BUILD_ID`. Never `MY_*` or `YOUR_*`. IF that casing is genuinely wrong for the context → THEN pick another scheme and stay consistent.
- Markup: `<var>` inside `<code>` for code and command placeholders, bare `<var>` outside code, `` *`PROJECT_ID`* `` in Markdown. Inside a fenced block, formatting doesn't apply — the placeholder is plain uppercase text. No brackets or braces inside the placeholder.
- Explain on first use: "Replace `PROJECT_ID` with your project ID." For several, write "Replace the following:" and list them in the order they appear, each with a lowercase description, even when the value looks obvious. Introduce an example inside a description with an em dash or "such as".
- Repeat the explanations when the document is long, holds several placeholders, or isn't read start to finish.
- Avoid `x` or `xxx` as a placeholder except in established forms such as HTTP `4xx`.

## Output

- Keep input and output in separate blocks. Introduce output with "The output is similar to the following:" or "The output is the following:", and say what to look at when it matters.
- Show only the relevant part; mark omitted lines with `...` on its own line, not an ellipsis character.
- Placeholders in output are introduced with "This output includes the following values:" and listed in order of appearance.

Next: what gets code font → `references/style-code.md`; steps around the command → `references/style-procedures.md`.
