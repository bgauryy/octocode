# Improve loop
Load when the subject is a skill, harness, document, or process. Why: separate a justified edit from a measured behavior claim.

1. Freeze the outcome, baseline copy, evaluation plan, and authority. For an editorial review, freeze a rubric and explicitly label its rating subjective.
2. Read the affected behavior paths and identify a concrete defect or below-target outcome. Use `agent-loop.md` for subject changes; use `nested-loops.md` when the harness itself is the subject.
3. Apply the smallest coherent change. Keep comparison tests fixed. If correcting a grader defect, retain the old version/results and compare old/new graders against independent labeled controls; never use its higher score alone as proof of solver improvement.
4. Run the required mechanical checks. Public fixtures, embedded answer samples, and regex checks establish maintenance properties only. Test live agent behavior in clean isolated trials when claiming behavioral improvement.
5. Decide at the evidence level actually measured. An authorized editorial or deterministic bug fix can be delivered with passing checks and behavioral benefit unmeasured; do not invent held-out scores or claim release acceptance from prose inspection.

Before/after reports use `output.md`. Final behavioral acceptance uses `held-out-and-guards.md`. Skill-folder structure and portability use `octocode-skills` review. Keep the original job and user authority intact.

Next: when presenting the result, use `references/output.md`; this improvement cycle ends after reporting its evidence limits.
