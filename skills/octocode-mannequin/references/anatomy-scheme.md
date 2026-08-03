# Anatomy Scheme

Load when building or explaining the bone hierarchy — before writing joint constraints.

22-bone "big bones" rig: one bone per major real bone/bone-group, hips as root. Two segments per limb-adjacent
spacer (Chest, Neck) are **0-DOF** — they exist only to carry proportion/attachment length, not to move; their
neighboring joints (Spine, Head) already carry the region's combined range of motion. This collapses the
cervical/thoracic/lumbar column (24 vertebrae) and carpal/tarsal/phalangeal clusters (54 small bones) into
rig-practical groups, matching the Unity Humanoid (15 required bones) / VRM 1.0 Humanoid (13 required, ~56 total)
convention rather than modeling all 206 bones. [VRM humanoid spec](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm-1.0/humanoid.md) · [OpenStax skeletal divisions](https://openstax.org/books/anatomy-and-physiology-2e/pages/7-1-divisions-of-the-skeletal-system) · [NCBI StatPearls appendicular skeleton](https://www.ncbi.nlm.nih.gov/books/NBK535397/)

**Rest pose = arms-down anatomical neutral**, not a T-pose: arms and legs hang straight down so the rig's
zero-rotation equals the goniometric zero (shoulder flexion then swings the arm forward like hip flexion). A
T-pose zero sits at ~90° abduction and mis-reads flexion; T-pose is only needed for external retarget interchange,
which this self-contained rig does not do (`references/references.md`, rest-pose research). The viewer additionally
**grounds** the figure — shifting the whole rig so the lowest foot rests on the floor — so it stands rather than
floats (`references/viewer-guide.md`).

## Hierarchy (parent → bone : real anatomical basis)

| Parent | Bone | Real basis |
|---|---|---|
| — | Hips | Pelvis (ilium+ischium+pubis); sacroiliac joint fixed into root |
| Hips | Spine | Lumbar (L1–L5) + thoracic (T1–T12) column, lumped |
| Spine | Chest | Rib cage + sternum (0-DOF spacer) |
| Chest | Neck | Cervical column C1–C7 length (0-DOF spacer) |
| Neck | Head | Skull; carries cervical spine's combined ROM |
| Head | Jaw | Mandible |
| Chest | Shoulder_L/R | Clavicle + scapula (shoulder girdle) |
| Shoulder_L/R | UpperArm_L/R | Humerus |
| UpperArm_L/R | Forearm_L/R | Radius + ulna, combined |
| Forearm_L/R | Hand_L/R | Carpals + metacarpals + phalanges, combined |
| Hips | UpperLeg_L/R | Femur |
| UpperLeg_L/R | LowerLeg_L/R | Tibia + fibula, combined (patella omitted — sesamoid, not load-chain) |
| LowerLeg_L/R | Foot_L/R | Talus + calcaneus + other tarsals + metatarsals, combined |
| Foot_L/R | Toes_L/R | Phalanges, combined |

Source of truth for this hierarchy plus DOF/ROM data lives in `scripts/skeleton.mjs` (`BONES`/`JOINTS`) — this
table is the human-readable mirror; run `node scripts/skeleton.mjs scheme` for the executable version.

Next: joint type/DOF per bone → `references/joint-constraints.md`; degree values → `references/rom-table.md`.
