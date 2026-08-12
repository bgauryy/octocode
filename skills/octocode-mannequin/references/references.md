# References

Provenance for this skill (research passes 2026-07-29+). Primary sources actually opened; dense agent reports are reproducible from the same prompts.

## Anatomy / ROM
| Source | Used for |
|---|---|
| [OpenStax A&P 2e](https://openstax.org/books/anatomy-and-physiology-2e/pages/7-1-divisions-of-the-skeletal-system) | Bone divisions |
| [NCBI Appendicular Skeleton](https://www.ncbi.nlm.nih.gov/books/NBK535397/) | Limb bone chain |
| [NCBI Glenohumeral](https://www.ncbi.nlm.nih.gov/books/NBK537018/) / [TMJ](https://www.ncbi.nlm.nih.gov/books/NBK538486/) | Shoulder / jaw DOF |
| [goniometer.io](https://goniometer.io/range-of-motion) (AAOS) | Primary ROM table; 6 corrections |
| [OrthoFixar](https://orthofixar.com/) | Independent ROM cross-check |
| [VRM 1.0 humanoid](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm-1.0/humanoid.md) / [Unity Avatar](https://docs.unity3d.com/Manual/ConfiguringtheAvatar.html) | Rig bone count; rest pose |

## Three.js / constraints
| Source | Used for |
|---|---|
| [three.js Skeleton](https://threejs.org/docs/#api/en/objects/Skeleton) / [Skeleton.js](https://github.com/mrdoob/three.js/blob/dev/src/objects/Skeleton.js) | FK hierarchy |
| [CCDIKSolver](https://threejs.org/docs/#examples/en/animations/CCDIKSolver) / [THREE.IK](https://github.com/jsantell/THREE.IK) | IK shape (stale FABRIK ref) |
| [Holden joint limits](https://theorangeduck.com/page/joint-limits) | Swing-twist |
| [Matt Rossman bone orientations](https://mattrossman.com/2024/07/10/visualizing-threejs-bone-orientations/) | Why SkeletonHelper alone fails |

## Animation / grounding
| Source | Used for |
|---|---|
| [AnimSchool run cycle](https://blog.animschool.edu/2024/04/10/the-key-poses-of-a-run-cycle/) / [MoCap Online](https://mocaponline.com/blogs/mocap-news/run-cycle-animation) | Contact/flight; root motion |
| [animcoding blend](https://animcoding.com/post/animation-tech-intro-part-3-blending/) / [Yuksel Catmull-Rom](https://www.cemyuksel.com/research/catmullrom_param/catmullrom.pdf) | Ease/slerp; centripetal α=0.5 |
| [PMC gait CoM](https://pmc.ncbi.nlm.nih.gov/articles/PMC6763727/) / Unreal foot-IK pelvis offset | Grounding bob |
| [gameanim 12 principles](https://www.gameanim.com/2019/05/15/the-12-principles-of-animation-in-video-games/) / [Animator Island music](https://www.animatorisland.com/animating-to-music/) | Landing settle; dance beat → `assets/` |

## Agent control / WebMCP
| Source | Used for |
|---|---|
| [ASU robotics control](https://wanxinjin.github.io/asu-robotics/lec19/control_overview.html) | Joint- vs task-space; FK |
| [Unity IK](https://docs.unity3d.com/Manual/InverseKinematics.html) / [MoveIt practices](https://docs.picknik.ai/how_to/robotics_applications/motion_planning_best_practices/) | `reach`/`look_at`; report clamp |
| [WebMCP](https://webmcp.dev/) / [webmcp repo](https://github.com/webmachinelearning/webmcp) | Page-side tool API |
| `octocode-chrome-devtools` (this repo) | CDP WebMCP driver — delegated, not reimplemented |
