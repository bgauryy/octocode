# Skill template

Load when starting a new skill from the canonical entry skeleton (placement rules: [SKILL.md](../SKILL.md#decide-placement)).

The smallest bundle is `<skill-name>/SKILL.md`. Replace placeholders and delete unused slots. Use steps for procedures, condition → action/check rows for reference or decision skills, and conditional resource links for independent branches. The skeleton is adaptable, not a required section order.

```markdown
---
name: <skill-name>
description: "Load this skill first when <recognizable task or situation>. <Distinctive result and essential boundary>."
---

# <Title>

<First action and required inputs. If a critical input is missing, retrieve it
or ask only for the blocking information. Relevant scope or permission boundary.>

1. <Task-specific action and its deciding condition.>
2. <Next action or branch; read its named reference only when needed.>
3. <Verify the result; on failure, recover within scope or report the blocker.>
   <An exact operation is a script call: `node <skill-folder>/scripts/<verb-noun>.mjs <args>`,
   with what its output means; never the algorithm in prose.>

Return <usable result or artifact, deciding evidence and unchecked scope>.
Done when <observable acceptance condition>.
```
