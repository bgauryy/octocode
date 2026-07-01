# READ and UNDERSTAND Gates

Load when starting an optimization: read the whole input, then understand what it does before rating anything.

## Step 1: READ

**STOP. DO NOT proceed to analysis.**

### Pre-Conditions
- [ ] User provided a prompt/file to optimize
- [ ] Path is valid and readable

### Required actions
1. Read the input file completely.
2. Note the document type and purpose.
3. Note the approximate line count.

### Gate Check
- [ ] File read completely (no skipped sections)
- [ ] Document type identified
- [ ] Line count noted

### Forbidden
- Making any changes before reading
- Skipping sections

### Allowed
- Read-only file access
- Text output to confirm reading

### On Failure
- **IF** file unreadable and inline content exists → **THEN** continue using the provided content.
- **IF** file unreadable and no content exists → **THEN** ask the user for the correct path.
- **IF** file empty → **THEN** ask the user to provide content.

## Step 2: UNDERSTAND

**STOP. DO NOT proceed to rating. Understand what this prompt does first.**

### Pre-Conditions
- [ ] READ gate completed
- [ ] File content in context

### Required actions
1. Identify the **goal** — what is this prompt supposed to achieve?
2. Identify the **logical parts** — sections, phases, or steps.
3. Identify the **flow** — how the parts connect.
4. Document understanding in the format below.

```markdown
## Understanding
**Goal:** [What the prompt achieves]
**Logical Parts:**
1. [Part name] - [purpose]
2. [Part name] - [purpose]
**Flow:** [How parts connect]
```

If the prompt is underspecified, also record assumptions and unknowns:

```markdown
## Assumptions & Unknowns
**Assumptions (proceeding with these):**
- [Assumption] - Impact if wrong: [consequence]
**Unknowns (ask before proceeding):**
- [Unknown] - Why critical: [reason]
**Clarification needed:** Yes/No
```

**IF** unknowns exist → **THEN** STOP and ask the user before RATE.

### Gate Check
- [ ] Goal clearly stated
- [ ] All logical parts identified
- [ ] Flow documented
- [ ] Understanding output produced

### Reflection
- Did I understand the intent correctly?
- Did I identify all logical parts?

**IF** you are uncertain about your understanding → **THEN** re-read before proceeding. Do not guess.

### Forbidden
- Proceeding without understanding the goal
- Making changes based on assumptions

### Allowed
- Text output (understanding summary)
- Re-reading the file if needed

### On Failure
- **IF** intent unclear → **THEN** ask the user for clarification.
- **IF** multiple interpretations → **THEN** present options and wait for the user's choice.
