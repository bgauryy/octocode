# Procedures

Load when writing or reviewing numbered steps and task instructions.

## Shape

- Numbered steps for a sequence; substeps a, b, c; sub-substeps i, ii, iii.
- A one-step procedure is a single bulleted item, not a list of one.
- Introduce the procedure with a complete sentence, ending in a colon before the steps; "do the following:" is a good closer. IF the introduction would only repeat the heading → THEN drop it.
- Prerequisites, permissions, and required software go before step 1.
- Don't repeat a procedure that already exists — link to it.
- Document one procedure that works for everyone: prefer the keyboard-accessible, shortest, most familiar path. Don't document keyboard shortcuts as the way to do a task.

## Each step

- The first sentence of a step contains an imperative verb; location and goal come before the verb: "In the console, click **Create**"; "To enable billing, click **Enable**".
- IF the "To …" opener could read as optional → THEN name the outcome first: "Start a new document: click **File** > **New** > **Document**".
- One action per step. Chain only trivial menu hops in a single bold sequence.
- Optional steps read "Optional: Enter a description." — not "(Optional)".
- IF the reader must press Enter → THEN say so inside the step.
- Give the reason when it prevents a mistake: "Store the key. You need it in the next step."
- Keep the result in the same step when it matters ("Click **Run**. The results appear in the console."), and don't split one action into an action step plus a "the dialog appears" step.
- Avoid bolding every UI element in sight; bold the ones the reader acts on.
- IF a step has substeps → THEN treat its text as an introduction and end it with a colon or a period.

## Complex steps

Order the parts: action → command → placeholder explanations → what the command does → sample output → result. Don't introduce a code block with "run the following command"; say what the command accomplishes.

## What doesn't belong

- No directional language ("the button below", "the left pane"). IF an element is genuinely hard to find → THEN show a screenshot or name it with its icon (`references/style-ui.md`).
- No tables in the middle of a numbered procedure (`references/style-blocks.md`).
- No notices carrying a required step — put it in the flow (`references/style-blocks.md`).

Next: UI wording inside steps → `references/style-ui.md`; commands and output blocks → `references/style-cli.md`.
