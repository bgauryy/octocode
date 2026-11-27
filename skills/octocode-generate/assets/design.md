---
status: Draft
project: <name>
context: <workspace>/.octocode/octocode-generate/<slug>/context/
---

# Design: <name>

What the app is and how it works, written from the user's answers and the items in `context/`. Each row names its source: a context item, an answer, or `assumed`. Keep it short enough to review in one read; the plan says how it gets built.

## Product

Who uses it, the problem it solves, and what a successful first version does (one paragraph).

## Core flows

| Flow | Steps (screen → action → result) | Source |
|---|---|---|

## Screens and navigation

| Screen or route | Purpose | Main components | Data shown | Source (mock) |
|---|---|---|---|---|

Navigation: tabs, stack, sidebar or routes, and which screens need sign-in.

## Visual style

Taken from the mocks when given; otherwise the UI kit's defaults.

| Token | Value | Source |
|---|---|---|
| Colors (primary, surface, text, accent, danger) | | |
| Typography (families, sizes) | | |
| Spacing, radius, shadows | | |
| Dark mode | | |
| UI kit and icons | | |

## Data model

| Entity | Fields (type) | Relations | Stored in |
|---|---|---|---|

## Architecture

Clients, server or API, data store and integrations, and where auth is checked. Use a short list or a Mermaid diagram.

## API surface

| Route, action or function | Input | Output | Auth |
|---|---|---|---|

## States and edge cases

Empty, loading, error, offline, permission denied, and first run.

## Accessibility and locales

Keyboard and screen-reader basics, contrast, and the languages and formats needed.

## Open questions

Each with the default the build will use if the user does not answer.
