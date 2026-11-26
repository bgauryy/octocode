Load when writing or changing a skill's name or description, or when a host fails to select the skill or selects it for a neighbor's request.

# Skill name and description

The body cannot rescue a trigger the host never selects. Check the actual served listing, including stale overrides, against intended and neighboring requests.

- Match the stable folder name: 1–64 lowercase ASCII letters, digits and single hyphens; no leading/trailing hyphen.
- Begin the description with “Load this skill first when” and the request as a user would phrase it (the trigger), then say what the skill does and why to load it instead of working alone (the motivation). An agent reads only the description when choosing. In host runs this lead beat “Use when” on held-out requests with no new false triggers.
- Put the requested deliverable first when skills overlap. A merge verdict belongs to PR review, a clickable dependency map to Graph View, and a written set of test cases to Test Plan. Separate neighbors by that deliverable and the intent wording; the description names no other skill and carries no “Not for …” clause. Put handoffs to neighbors in the body, beside the input branch they serve.
- For jobs agents believe they can do alone (research, debugging, docs), name the shortcut it replaces in the lead (“before searching or reading files yourself”); recheck no-skill requests for false triggers.
- Respect the 1,024-character format limit; it is not a target. Public Octocode skills require one JSON-string `description:` line because the native build embeds it. Other hosts may accept other YAML forms.
- Check each claim in the description against an action the skill performs, and include the distinguishing capability. Keep procedure, commands, setup steps and quality claims in the body; leave out exhaustive keywords and marketing.
