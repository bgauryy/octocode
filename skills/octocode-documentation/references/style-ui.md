# UI elements and interaction

Load when the text tells a reader to operate an interface. State what the reader accomplishes, not which widget they poke: "Refresh the page", "Expand the **Advanced options** section". It survives redesigns. IF the point of the procedure is to walk the reader through the page itself → THEN name the elements.

## Terminology

| Element | Use |
|---|---|
| Web page, console subpage | page — the preferred general term |
| Smaller window for one interaction | dialog, not `pop-up window` |
| Distinct region inside a window | pane or panel — never window, section, area, or column |
| Region inside a pane | section — never area or column |
| Item in a menu | command; "menu item" only when documenting how to build an interface |
| Text entry | box, as "the **Name** box"; Google Cloud and Workspace docs use field |
| Expandable region | expander arrow, expandable section — never "expando" or "zippy" |
| Navigation menu | navigation menu — never `navigation bar`, `pane`, `panel`, or `window` |
| Slang | never `hamburger icon` or `kebab menu` |

## Labels

- UI labels are bold: "Click **Save**". A label that also qualifies for code font takes both: "In the **`Network`** list, select **`my-net-2`**". Text the reader types is code font: "In the **Name** field, enter `wsfc-1`."
- Follow the label's own capitalization, but IF a label is all uppercase or a set of labels is inconsistently cased → THEN use sentence case: "Click **Refresh**". Drop a trailing ellipsis: document `Save ...` as "click **Save**". Don't bold a product or feature name unless it is literally the label on screen, and never quote a label.
- Icons: put the icon before the name from its tooltip — "click ⋮ **Settings and utilities**". Never describe an icon by shape, and don't append the word "icon" to a label. IF no tooltip exists → THEN check `aria-label`, `aria-labelledby`, `title`, or `placeholder`, and file a bug asking for a tooltip.
- Menu chains use a single bold span with a nonbreaking space before each angle bracket: "Click **File > Open**"; label the separator for screen readers (`aria-label="and then"`). The notation is for menu commands only — don't chain unrelated element types.

## Verbs

| Target | Verb |
|---|---|
| Button, link, icon | click (tap on touch devices) |
| Checkbox | select / clear; state it as "selected" or "not selected" |
| Radio button, menu command, list option | select or choose |
| Toggle, switch | turn on / turn off |
| Key | press |
| Box or field | enter, type |
| Page, tab, section | go to, open, expand |
| Pointer | drag; hold the pointer over (never `hover`) |
| Never | a label as a verb ("click **Save**", not "**Save** the file"), `toggle`, `deselect`, `hit` |

## Keys and location

`<kbd>` markup, spelled-out modifiers, uppercase letters, `MODIFIER+Shift+KEY`: `Control+S`. Put the macOS variant in parentheses: "press **Control+C** (or **Command+C** on macOS)". Spell out confusable characters (comma, hyphen, period, plus). A key typed for its literal value is code font, not `<kbd>`. Call it a keyboard shortcut or key combination — and don't make a shortcut the documented way to complete a task (`references/style-procedures.md`).

- "in" a dialog, field, menu, window, pane; "on" a page, tab, toolbar. Location first: "In the **Query** pane, click **Run**." Outside a numbered procedure, give the element enough context that the reader knows where it lives.
- No directional references. IF an element is genuinely hard to find → THEN provide a screenshot (`references/style-images.md`).

Upstream: [UI elements and interaction](https://developers.google.com/style/ui-elements). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: steps around the click → `references/style-procedures.md`; accessible phrasing → `references/style-global.md`.
