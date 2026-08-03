# Range-Of-Motion Table

Load when picking or explaining a joint's degree limits — data behind `JOINTS[].axes.*.min/max` in
`scripts/skeleton.mjs`. Values are clinical/kinesiology population norms (AAOS *Joint Motion* chart cross-checked
against independent sources) — **rig defaults, not hard biomechanical limits**; real individuals vary. Two
research passes validated these against [goniometer.io](https://goniometer.io/range-of-motion),
[OrthoFixar](https://orthofixar.com/), [Kenhub](https://www.kenhub.com/en/library/anatomy), Physiopedia, and
NCBI/PubMed (full list in `references/references.md`); six values were corrected in the second pass (✎).

| Joint | Movement (+X/−X, +Z/−Z, +Y/−Y) | Degrees | Confidence |
|---|---|---|---|
| Spine (lumbar+thoracic) | flexion / extension | 80 / 25 | AAOS; ext 20–30 range |
| Spine | lateral flex (each side) / rotation (each side) | 35 / 45 | AAOS (rotation 30–45) |
| Head (cervical) | flexion / extension | 45 / 45 | AAOS (studies 45–60) |
| Head | lateral flex / rotation (each side) | 45 / 60 | AAOS (rotation functional ~80) |
| Jaw (TMJ) | depression (angular approx) / lateral excursion | ~40 / ~10 | weak — clinically measured in mm, not degrees |
| Shoulder girdle | elevation / depression | 40 / 10 | Kenhub — often reported in cm, convention-soft |
| Shoulder girdle | protraction / retraction | 20 / 15 | Kenhub — convention-soft |
| Glenohumeral (shoulder) | flexion / extension | 180 / 60 | 2+ sources (whole-shoulder-complex) |
| Glenohumeral | abduction / adduction | 180 / 40 | 2+ sources; adduction = cross-body, definitional |
| Glenohumeral | internal / external rotation | 70 / 90 | 2+ sources |
| Elbow | flexion / extension | 145 / 0 | 2+ sources (strict AAOS 150) |
| Forearm (twist) | pronation / supination | 80 / 80 | 2+ sources (AAOS; supination up to 90) |
| Wrist | flexion / extension | 80 / 70 ✎ | 2+ sources (ext was 80; AAOS ceiling 70) |
| Wrist | radial / ulnar deviation | 20 / 30 ✎ | 2+ sources (was 18/38) |
| Hip | flexion / extension | 122 / 20 | 2+ sources (AAOS chart ext 30) |
| Hip | abduction / adduction | 45 / 30 | 2+ sources |
| Hip | internal / external rotation | 42 / 45 ✎ | 2+ sources (ext was 52) |
| Knee | flexion / extension | 135 / 0 | 2+ sources; axial rot ~40 total only when flexed (not modeled) |
| Ankle | dorsiflexion / plantarflexion | 20 / 50 | 2+ sources |
| Ankle (foot inversion/eversion) | inversion / eversion | 35 / 15 | AAOS combined-foot convention |
| Toes (MTP) | flexion / extension | 45 / 70 ✎ | 2 sources (was 35/60; Kenhub/Norkin) |

Values feed `scripts/skeleton.mjs` directly; do not hand-edit the script's numbers without updating this table.

Next: how a caller expresses a movement as a signed command → `references/movement-protocol.md`.
