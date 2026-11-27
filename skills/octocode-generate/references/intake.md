# Intake

Load for the first reply, and again to ask for approval and record the answers. Aim for one question round, then the design and plan. Ask through the host's question tool (AskUserQuestion or its equivalent): a few questions per call, 2–4 options each with the recommended one first, multi-select for lists such as services, and free text for the one-sentence idea. Without such a tool, ask the same questions in one short message. Skip what the request, the repository or `detect-workspace.mjs` already answers.

## Ask what first

| Ask | Why it matters |
|---|---|
| What are you building, for whom, and its core features (a sentence plus a short list) | Preset, scope, data and auth |
| Where it runs: web, iOS/Android, desktop, server/API, edge, CLI, library, browser extension | Platform catalog |
| Target folder and project name (default: a new folder `<slug>` in the current one) | Where the scaffold runs; it must be new or empty |
| Mocks or references to build from: designs or screenshots, a Figma export, an API spec, sample data, a site or app to resemble | The build follows them instead of guesses |
| Hard constraints: a required language, framework, host or existing backend (Firebase, Supabase, Wix, Base44, MongoDB); budget or compliance | Which preset or alternative |
| Recommended stack, or choose each piece | Whether a second round is needed |

Save every file or link the user gives to `<workspace>/.octocode/octocode-generate/<slug>/context/`, one file per item plus a `README.md` that lists each with a line on what it shows. An image pasted into the chat may not exist as a file: describe it in `context/README.md` (screens, text, colors, layout) and ask for the file when exact assets matter (logos, icons). Read images with the host's image support, or `clasify` with the image as an asset. Read reference sites with `clasify` `source:{"url":…}` or the host's web tools. Note in the plan what each item decided: screens, fields, flows, style.

## When to ask more

- **Second round.** Only when the user wants to choose each piece, or when a costly choice stays open: data store, auth, a hosted backend, or the package manager when none is detected. Offer the leading options from [integrations](integrations.md) and [backends](integrations-backends.md) with their docs links.
- **Services.** Ask once, with defaults off, whether the app needs analytics, error monitoring, payments or email. Tracking needs the user's yes.
- **Brainstorm.** When the idea is still open (no clear features, or the user asks for ideas), offer a short brainstorm before planning: the `octocode-brainstorming` skill when installed, otherwise 3–5 directions with a one-line trade-off each. Offer it; do not force it.

Every other choice takes the preset default ([presets](presets.md)), recorded as `assumed`.

## Approve

After `design.md` and `plan.md` exist, show their paths and a short summary (stack, screens, integrations, commands that will run, open questions), then ask through the question tool:

| Option | Then |
|---|---|
| Approve and build (recommended) | Record the approval in `plan.md`; start building |
| Change the design | Revise `design.md` and the plan rows it affects; ask again |
| Change the stack or services | Revise `plan.md`, re-run the affected research; ask again |

Building starts only on an explicit approval.

## Record

Write each answer to the plan's Requirements table marked `user`, `detected` or `assumed`, and list the context items. An assumption the user later rejects reopens only the decisions that rest on it.
