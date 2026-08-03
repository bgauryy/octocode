# References

Sources consulted to research, create, and validate this skill (2026-07-29 research pass, two dispatched agents:
anatomy/biomechanics, and Three.js skeleton/rigging).

## Anatomy / biomechanics
| Source | URL | Used for |
|---|---|---|
| OpenStax A&P 2e | https://openstax.org/books/anatomy-and-physiology-2e/pages/7-1-divisions-of-the-skeletal-system | Bone count/divisions |
| NCBI StatPearls, Appendicular Skeleton | https://www.ncbi.nlm.nih.gov/books/NBK535397/ | Limb bone chain |
| NCBI StatPearls, Glenohumeral Joint | https://www.ncbi.nlm.nih.gov/books/NBK537018/ | Shoulder joint type/DOF |
| NCBI StatPearls, TMJ | https://www.ncbi.nlm.nih.gov/books/NBK538486/ | Jaw joint type |
| Kenhub, Shoulder/Scapulothoracic/Knee joints | https://www.kenhub.com/en/library/anatomy | Joint type + shoulder-girdle ROM |
| TeachMeAnatomy, Joints | https://teachmeanatomy.info/the-basics/learning-anatomy/joints/ | Joint-type taxonomy |
| goniometer.io | https://goniometer.io/range-of-motion | Primary ROM degree table (AAOS-derived) |
| Turkish Spinal Surgery journal | https://jtss.org/articles/doi/jtss.galenos.2023.33042 | Lumbar/cervical ROM research norms |
| VRM 1.0 humanoid spec | https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm-1.0/humanoid.md | Rig-practical bone-count reconciliation |
| Unity Manual, Avatar config | https://docs.unity3d.com/Manual/ConfiguringtheAvatar.html | Required-bone count comparison |

## Three.js / rigging
| Source | URL | Used for |
|---|---|---|
| three.js docs, Bone/Skeleton/SkinnedMesh | https://threejs.org/docs/#api/en/objects/Skeleton | Bone hierarchy + FK primitives |
| mrdoob/three.js Skeleton.js source | https://github.com/mrdoob/three.js/blob/dev/src/objects/Skeleton.js | `pose()`/`update()` behavior |
| three.js CCDIKSolver docs | https://threejs.org/docs/#examples/en/animations/CCDIKSolver | Box-clamp IK constraint shape |
| jsantell/THREE.IK (`three-ik`) | https://github.com/jsantell/THREE.IK | FABRIK reference, confirmed stale (last push 2023-03-15) |
| Daniel Holden, Joint Limits | https://theorangeduck.com/page/joint-limits | Swing-twist decomposition algorithm |
| cannon-es ConeTwistConstraint docs | https://pmndrs.github.io/cannon-es/docs/classes/ConeTwistConstraint.html | Physics-engine analog for ball joints |
| Matt Rossman, Visualizing bone orientations | https://mattrossman.com/2024/07/10/visualizing-threejs-bone-orientations/ | Why `SkeletonHelper` alone is insufficient |
| Don McCurdy, Mixamo+Blender+glTF | https://www.donmccurdy.com/2017/11/06/creating-animated-gltf-characters-with-mixamo-and-blender/ | Why we skip pre-rigged-model loading for this skill |

## Animation smoothness + grounding (2nd research pass)
| Source | URL | Used for |
|---|---|---|
| AnimSchool, run-cycle key poses | https://blog.animschool.edu/2024/04/10/the-key-poses-of-a-run-cycle/ | Contact/down/passing/flight poses; run needs flight |
| MoCap Online, run cycle + locomotion | https://mocaponline.com/blogs/mocap-news/run-cycle-animation | Flight phase, in-place vs root-motion, loop matching |
| animcoding, lerp vs slerp/ease | https://animcoding.com/post/animation-tech-intro-part-3-blending/ | Linear=robotic; ease/slerp fix (→ Catmull-Rom choice) |
| Three.js source (KeyframeTrack, QuaternionLinearInterpolant) | https://github.com/mrdoob/three.js | Quaternion tracks slerp by default; interpolation semantics |
| NCBI PMC gait / CoM oscillation | https://pmc.ncbi.nlm.nih.gov/articles/PMC6763727/ | Pelvis bobs 2×/gait cycle → grounding gives it |
| Unreal foot-IK pelvis offset | https://forums.unrealengine.com/t/how-do-you-handle-foot-ik-and-game-logic-when-pelvis-moves-up-down/1177523 | Drop-pelvis-by-lowest-foot grounding technique |

## ROM re-validation + rest pose (2nd research pass)
| Source | URL | Used for |
|---|---|---|
| AAOS Joint Motion chart | https://goniometer.io/range-of-motion | Primary ROM norms; 6 corrections (wrist ext 70, ulnar 30, radial 20, hip ext-rot 45, toe 45/70) |
| OrthoFixar joint ROM | https://orthofixar.com/ | Independent ROM cross-check |
| Neumann Kinesiology (via clinicalgate) | https://clinicalgate.com/2015/03/18/knee-5/ | Knee axial rotation ~40° (flexed only) — not modeled |
| NCBI Clinical Methods, TMJ | https://www.ncbi.nlm.nih.gov/books/NBK271/ | Jaw opening is mm not degrees; ~25° rotation before translation |
| Unity Humanoid / VRM / Mixamo | https://docs.unity3d.com/Manual/ConfiguringtheAvatar.html | Rest pose: T-pose for interchange, A/arms-down for bespoke; rig-zero ≠ anatomical-zero |

## Interpolation / landing / polish (3rd research pass)
| Source | URL | Used for |
|---|---|---|
| Yuksel, Catmull-Rom parameterization | https://www.cemyuksel.com/research/catmullrom_param/catmullrom.pdf | Why uniform C-R overshoots; centripetal α=0.5 |
| Centripetal Catmull-Rom (Wikipedia) | https://en.wikipedia.org/wiki/Centripetal_Catmull%E2%80%93Rom_spline | Overshoot fix options (we use bracket-clamp) |
| Unity, rotation in animations | https://docs.unity3d.com/Manual/AnimationRotate.html | Can't scalar-loop rotation past 360 → one-shot / phase-accumulate |
| Holden, looping animations / inertialization | https://theorangeduck.com/page/creating-looping-animations-motion-capture | Seam matching, accumulate net delta |
| gameanim, 12 principles in games | https://www.gameanim.com/2019/05/15/the-12-principles-of-animation-in-video-games/ | Crouch→settle landing, fake squash, slow-in/out |

## Dance + streaming landscape (3rd research pass)
| Source | URL | Used for |
|---|---|---|
| Kalidokit | https://github.com/yeemachine/kalidokit | MediaPipe landmarks → bone rotations (webcam mocap path) |
| three.js SkeletonUtils retarget issue #25751 | https://github.com/mrdoob/three.js/issues/25751 | Mixamo retarget is buggy → author JSON dance moves instead |
| Wawa Sensei three.js+VRM+MediaPipe | https://wawasensei.dev/tuto/vrm-avatar-with-threejs-react-three-fiber-and-mediapipe | Browser VTuber pipeline reference |
| Saving canvas animations (MediaRecorder) | https://blog.theodo.com/2023/03/saving-canvas-animations/ | Record page 3D → WebM for streaming/sharing |
| Animator Island, animating to music | https://www.animatorisland.com/animating-to-music/ | Dance: beat-snap hits, weight shift, contrast |

## Robotics / agent-control best practices (4th research pass)
| Source | URL | Used for |
|---|---|---|
| ASU Robotics, control overview | https://wanxinjin.github.io/asu-robotics/lec19/control_overview.html | Joint-space vs task-space; FK has no singularities |
| Unity IK / Roblox IK | https://docs.unity3d.com/Manual/InverseKinematics.html | Task-space reach + look-at (aim) are first-class primitives → `reach`/`look_at` tools |
| ROS REP-199 / URDF | https://gavanderhoorn.github.io/rep/rep-0199.html | Self-describing contract: frame, units, sign, limits → `get_scheme.guide` |
| MoveIt motion-planning best practices | https://docs.picknik.ai/how_to/robotics_applications/motion_planning_best_practices/ | Report unreachable/clamped, don't fail silently → reach returns reached+distance |
| Grounding LLMs with closed-loop feedback | https://advanced.onlinelibrary.wiley.com/doi/10.1002/adrr.202500072 | Return observations from mutating tools (clamp warnings) for closed loop |
| Fanuc/Yaskawa jogging | https://industrialrobotics.miraheze.org/wiki/Jogging | Absolute + relative(jog) commands → `apply_pose` relative:true |

## Dance + "alive" idle (authored from research)
| Source | URL | Used for |
|---|---|---|
| Animator Island, animating to music | https://www.animatorisland.com/animating-to-music/ | Dance: weight-shift bounce, contrast, beat hits → `assets/dance.json` |
| gameanim, 12 principles | https://www.gameanim.com/2019/05/15/the-12-principles-of-animation-in-video-games/ | Moving holds (never perfectly frozen) → `assets/idle.json` alive idle |

## WebMCP (agent control)
| Source | URL | Used for |
|---|---|---|
| WebMCP site | https://webmcp.dev/ | Concept: pages expose MCP tools to agents |
| webmachinelearning/webmcp | https://github.com/webmachinelearning/webmcp | `document.modelContext.registerTool` page-side API shape |
| octocode-chrome-devtools skill | (this repo) `skills/octocode-chrome-devtools` | CDP `WebMCP` domain driver (`examples/webmcp-tools.mjs`), `--enableFeatures WebMCP` launch — delegated, not reimplemented |

The two full agent reports (dense ROM tables, DOF tables, IK maturity comparison) are reproducible by re-running
the same research prompts; this file lists only the primary sources actually opened via WebFetch, not every
search-result snippet.
