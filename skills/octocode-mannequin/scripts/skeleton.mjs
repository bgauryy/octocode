#!/usr/bin/env node
// Zero-dependency (no npm install) source-of-truth for the octocode-mannequin skeleton scheme + movement
// protocol. Mirrors references/anatomy-scheme.md, joint-constraints.md, rom-table.md, movement-protocol.md —
// on disagreement this file is authoritative. Rest-pose lengths/offsets are illustrative visualization scale,
// not clinical anthropometry; DOF/ROM values are cited in references/rom-table.md.

// ---------- vector / quaternion math (mirrors Three.js semantics; no dependency on the `three` package) ----------

const V = {
  add: (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  sub: (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  scale: (a, s) => [a[0] * s, a[1] * s, a[2] * s],
  dot: (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  cross: (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  len: (a) => Math.hypot(a[0], a[1], a[2]),
  norm: (a) => { const l = V.len(a) || 1; return [a[0] / l, a[1] / l, a[2] / l]; },
};

const Q = {
  identity: () => [0, 0, 0, 1], // x,y,z,w
  fromAxisAngle: (axis, rad) => {
    const a = V.norm(axis);
    const s = Math.sin(rad / 2);
    return [a[0] * s, a[1] * s, a[2] * s, Math.cos(rad / 2)];
  },
  multiply: (a, b) => [
    a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
    a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
    a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
    a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
  ],
  rotate: (q, v) => {
    // v' = q * [v,0] * q^-1, expanded to avoid allocating quaternion objects
    const [qx, qy, qz, qw] = q;
    const [vx, vy, vz] = v;
    const tx = 2 * (qy * vz - qz * vy), ty = 2 * (qz * vx - qx * vz), tz = 2 * (qx * vy - qy * vx);
    return [
      vx + qw * tx + (qy * tz - qz * ty),
      vy + qw * ty + (qz * tx - qx * tz),
      vz + qw * tz + (qx * ty - qy * tx),
    ];
  },
  // Rotation that takes unit vector `from` to unit vector `to` — handles the antiparallel singularity
  // (used only at module load to build each bone's rest orientation from its rest direction).
  fromTo: (from, to) => {
    const d = V.dot(from, to);
    if (d > 0.999999) return Q.identity();
    if (d < -0.999999) {
      let axis = V.cross([1, 0, 0], from);
      if (V.len(axis) < 1e-6) axis = V.cross([0, 1, 0], from);
      return Q.fromAxisAngle(axis, Math.PI);
    }
    const axis = V.cross(from, to);
    const angle = Math.acos(Math.max(-1, Math.min(1, d)));
    return Q.fromAxisAngle(axis, angle);
  },
  conjugate: (q) => [-q[0], -q[1], -q[2], q[3]],
};

const deg2rad = (d) => (d * Math.PI) / 180;

// ---------- rest-pose bone data (world-space rest direction + length + optional start offset) ----------
// axes: 'up'=+Y 'down'=-Y 'left'=-X 'right'=+X 'forward'=+Z — T-pose directions only, illustrative scale (m).
const DIR = { up: [0, 1, 0], down: [0, -1, 0], left: [-1, 0, 0], right: [1, 0, 0], forward: [0, 0, 1] };

export const BONES = [
  { name: 'Hips', parent: null, dir: 'up', length: 0.10 },
  { name: 'Spine', parent: 'Hips', dir: 'up', length: 0.25 },
  { name: 'Chest', parent: 'Spine', dir: 'up', length: 0.15 },
  { name: 'Neck', parent: 'Chest', dir: 'up', length: 0.12 },
  { name: 'Head', parent: 'Neck', dir: 'up', length: 0.22 },
  { name: 'Jaw', parent: 'Head', dir: 'forward', length: 0.08, startOffset: [0, -0.15, 0.05] },
  ...['L', 'R'].flatMap((side) => {
    const out = side === 'L' ? 'left' : 'right';
    const s = (n) => `${n}_${side}`;
    return [
      // Arms hang DOWN in the rest pose (anatomical neutral), not out to the sides (T-pose). This makes the
      // rig's zero-rotation equal the goniometric zero, so shoulder flexion swings the arm forward like the hip
      // — a T-pose zero sits at ~90° abduction and mis-reads flexion. (references/references.md, rest-pose research.)
      // Only the shoulder girdle bone (clavicle+scapula) still points out to place the shoulder joint.
      { name: s('Shoulder'), parent: 'Chest', dir: out, length: 0.16 },
      { name: s('UpperArm'), parent: s('Shoulder'), dir: 'down', length: 0.30 },
      { name: s('Forearm'), parent: s('UpperArm'), dir: 'down', length: 0.26 },
      { name: s('Hand'), parent: s('Forearm'), dir: 'down', length: 0.18 },
      { name: s('UpperLeg'), parent: 'Hips', dir: 'down', length: 0.42, startOffset: [side === 'L' ? -0.09 : 0.09, 0, 0] },
      { name: s('LowerLeg'), parent: s('UpperLeg'), dir: 'down', length: 0.40 },
      { name: s('Foot'), parent: s('LowerLeg'), dir: 'forward', length: 0.18 },
      { name: s('Toes'), parent: s('Foot'), dir: 'forward', length: 0.08 },
    ];
  }),
];

// Uniform scale so foot-to-head-top comes out near a typical adult ~1.75m (raw lengths above summed to ~1.56m).
const SCALE = 1.12;
for (const b of BONES) { b.length *= SCALE; if (b.startOffset) b.startOffset = b.startOffset.map((v) => v * SCALE); }

// ---------- joint DOF / ROM / constraint model — references/joint-constraints.md + rom-table.md ----------
// axes.{x,z}: {min,max} degrees, signed per references/movement-protocol.md (flexion+/extension-, abduction+/
// adduction-, dorsiflexion+/plantarflexion-, inversion+/eversion-, elevation+/depression-, protraction+/
// retraction-, lateralFlexion sign-per-value). axes.y: twist/rotation, internalRotation+/externalRotation-,
// pronation+/supination-, rotation sign-per-value.
// `flip`: per-axis sign correction so a named movement's world-space direction matches real anatomy — needed
// because a bone's rest orientation (up/down/sideways) changes what "rotate +X" looks like in world space, and
// the uniform X=flex/Z=abd/Y=twist convention can't derive that automatically (verified empirically, see
// scripts/verify-directions.mjs-style checks in the skill's dev notes; e.g. hip flexion swings the thigh
// forward but knee flexion swings the shank backward even though both are 'down'-pointing chains).
// `axisMap`: per-joint override of which physical axis a named movement targets, for joints (Shoulder, Hand)
// whose rest orientation is sideways — rotating "around X" there swings horizontally, not vertically, so
// elevation/flexion need the axis that actually produces the anatomically-expected plane of motion.
// `movements`: the anatomically-valid movement names for this joint — the protocol's allowlist. A movement not
// listed here is rejected even if its axis physically exists (e.g. the shoulder girdle has an X axis for
// protraction/retraction but does not "flex"), so describe_joint's list is authoritative and typos/wrong-joint
// commands fail loudly instead of silently rotating the wrong plane.
export const JOINTS = {
  Spine: { constraint: 'box', movements: ['flexion', 'extension', 'lateralFlexion', 'rotation'], axes: { x: { min: -25, max: 80 }, z: { min: -35, max: 35 }, y: { min: -45, max: 45 } } },
  Head: { constraint: 'box', movements: ['flexion', 'extension', 'lateralFlexion', 'rotation'], axes: { x: { min: -45, max: 45 }, z: { min: -45, max: 45 }, y: { min: -60, max: 60 } } },
  Jaw: { constraint: 'box', movements: ['depression', 'lateralExcursion'], flip: { x: -1 }, axes: { x: { min: -40, max: 0 }, z: { min: -10, max: 10 } } },
  Shoulder: {
    constraint: 'box',
    movements: ['elevation', 'depression', 'protraction', 'retraction'],
    axisMap: { elevation: 'z', depression: 'z', protraction: 'x', retraction: 'x' },
    axes: { z: { min: -10, max: 40 }, x: { min: -15, max: 20 } },
  },
  UpperArm: { constraint: 'swing-twist', movements: ['flexion', 'extension', 'abduction', 'adduction', 'internalRotation', 'externalRotation'], swing: { flex: 180, ext: 60, abd: 180, add: 40 }, twist: { min: -90, max: 70 } },
  Forearm: { constraint: 'hinge-twist', movements: ['flexion', 'pronation', 'supination'], axes: { x: { min: 0, max: 145 } }, twist: { min: -80, max: 80 } },
  Hand: {
    constraint: 'box',
    movements: ['flexion', 'extension', 'radialDeviation', 'ulnarDeviation'],
    axisMap: { flexion: 'x', extension: 'x', radialDeviation: 'z', ulnarDeviation: 'z' },
    axes: { x: { min: -70, max: 80 }, z: { min: -30, max: 20 } },
  },
  UpperLeg: { constraint: 'swing-twist', movements: ['flexion', 'extension', 'abduction', 'adduction', 'internalRotation', 'externalRotation'], swing: { flex: 122, ext: 20, abd: 45, add: 30 }, twist: { min: -45, max: 42 } },
  LowerLeg: { constraint: 'hinge', movements: ['flexion'], flip: { x: -1 }, axes: { x: { min: 0, max: 135 } } },
  Foot: { constraint: 'hinge-box', movements: ['dorsiflexion', 'plantarflexion', 'inversion', 'eversion'], flip: { x: -1 }, axes: { x: { min: -50, max: 20 }, z: { min: -15, max: 35 } } },
  Toes: { constraint: 'hinge', movements: ['flexion', 'extension'], axes: { x: { min: -70, max: 45 } } },
};
const jointKeyFor = (boneName) => boneName.replace(/_[LR]$/, '');

// movement name → {axis, sign}; asymmetric pairs flip sign so the caller writes the anatomical name, not a
// signed number, and skeleton.mjs derives the sign per references/movement-protocol.md.
const MOVEMENTS = {
  flexion: ['x', 1], extension: ['x', -1],
  abduction: ['z', 1], adduction: ['z', -1],
  dorsiflexion: ['x', 1], plantarflexion: ['x', -1],
  inversion: ['z', 1], eversion: ['z', -1],
  elevation: ['x', 1], depression: ['x', -1],
  protraction: ['z', 1], retraction: ['z', -1],
  lateralFlexion: ['z', 1], lateralExcursion: ['z', 1],
  internalRotation: ['y', 1], externalRotation: ['y', -1],
  pronation: ['y', 1], supination: ['y', -1],
  radialDeviation: ['z', 1], ulnarDeviation: ['z', -1],
  rotation: ['y', 1],
};

// ---------- rest pose (computed once at module load) ----------

function buildRestPose() {
  const byName = new Map(BONES.map((b) => [b.name, b]));
  const start = new Map(), end = new Map(), worldQuat = new Map(), localPos = new Map(), localQuat = new Map();
  for (const b of BONES) {
    const parent = b.parent && byName.get(b.parent);
    const parentStart = parent ? start.get(parent.name) : [0, 0, 0];
    const parentEnd = parent ? end.get(parent.name) : [0, 0, 0];
    const s = V.add(parentEnd, b.startOffset || [0, 0, 0]);
    const wq = Q.fromTo([0, 1, 0], DIR[b.dir]);
    start.set(b.name, s);
    end.set(b.name, V.add(s, V.scale(DIR[b.dir], b.length)));
    worldQuat.set(b.name, wq);
    const parentWq = parent ? worldQuat.get(parent.name) : Q.identity();
    localQuat.set(b.name, Q.multiply(Q.conjugate(parentWq), wq));
    // displacement from parent's own start (its joint origin), not parent's end, rotated into parent's local frame
    localPos.set(b.name, Q.rotate(Q.conjugate(parentWq), V.sub(s, parentStart)));
  }
  return { localPos, localQuat };
}
const REST = buildRestPose();

// ---------- movement protocol: normalize → clamp → compose → forward-kinematics ----------

function normalizeMovements(joint, jointSpec, movements) {
  const perAxis = { x: 0, y: 0, z: 0 };
  const seen = {};
  for (const [name, value] of Object.entries(movements || {})) {
    const spec = MOVEMENTS[name];
    if (!spec) throw new Error(`unknown movement "${name}" for joint ${joint}`);
    if (jointSpec.movements && !jointSpec.movements.includes(name)) {
      throw new Error(`movement "${name}" not valid for joint ${joint}; valid movements: ${jointSpec.movements.join(', ')}`);
    }
    const axis = jointSpec.axisMap?.[name] || spec[0];
    const sign = spec[1];
    const opposite = axis + (sign > 0 ? '-' : '+');
    if (seen[opposite] !== undefined) throw new Error(`opposite-pair collision on axis ${axis} for joint ${joint}`);
    seen[axis + (sign > 0 ? '+' : '-')] = true;
    perAxis[axis] += value * sign;
  }
  return perAxis;
}

// sideSign mirrors the LEFT side: abduction and axial rotation are anatomically mirror-image between limbs
// (left abduction goes left, right goes right), so the geometric z (lateral) and y (twist) rotations are negated
// for _L bones — WITHOUT touching the ROM buckets (left abduction is still clamped by the abduction limit, not
// adduction) or sagittal x (flexion is the same forward direction on both sides).
function clampBox(axes, perAxis, warnings, joint, flip = {}, sideSign = 1) {
  const mirror = { x: 1, y: sideSign, z: sideSign };
  const q = { x: Q.identity(), y: Q.identity(), z: Q.identity() };
  for (const axis of ['x', 'z', 'y']) {
    const limit = axes[axis];
    if (!limit) { if (perAxis[axis]) throw new Error(`joint ${joint} has no ${axis} axis`); continue; }
    let v = perAxis[axis];
    if (v > limit.max) { warnings.push(`${joint}.${axis} clamped ${v}->${limit.max}`); v = limit.max; }
    if (v < limit.min) { warnings.push(`${joint}.${axis} clamped ${v}->${limit.min}`); v = limit.min; }
    const axisVec = axis === 'x' ? [1, 0, 0] : axis === 'z' ? [0, 0, 1] : [0, 1, 0];
    q[axis] = Q.fromAxisAngle(axisVec, deg2rad(v * (flip[axis] || 1) * mirror[axis]));
  }
  return Q.multiply(Q.multiply(q.x, q.z), q.y);
}

function clampSwingTwist(spec, perAxis, warnings, joint, sideSign = 1) {
  const { swing, twist } = spec;
  let flex = perAxis.x, abd = perAxis.z, tw = perAxis.y;
  const ux = flex / (flex >= 0 ? swing.flex : swing.ext);
  const uz = abd / (abd >= 0 ? swing.abd : swing.add);
  const mag = Math.hypot(ux, uz);
  if (mag > 1) { warnings.push(`${joint} swing scaled by ${(1 / mag).toFixed(3)} to stay inside cone`); flex /= mag; abd /= mag; }
  if (twist) {
    if (tw > twist.max) { warnings.push(`${joint}.twist clamped ${tw}->${twist.max}`); tw = twist.max; }
    if (tw < twist.min) { warnings.push(`${joint}.twist clamped ${tw}->${twist.min}`); tw = twist.min; }
  } else if (tw) throw new Error(`joint ${joint} has no twist axis`);
  const angle = Math.hypot(flex, abd);
  const swingQuat = angle < 1e-9 ? Q.identity() : Q.fromAxisAngle([flex, 0, abd * sideSign], deg2rad(angle));
  const twistQuat = Q.fromAxisAngle([0, 1, 0], deg2rad(tw * sideSign));
  return { quat: Q.multiply(swingQuat, twistQuat), clamped: mag > 1 };
}

// {tx,ty,tz,pitch,yaw,roll} -> {pos:[x,y,z], quat}. Order yaw(Y)·pitch(X)·roll(Z) so a pure pitch is a clean
// sagittal somersault regardless of yaw. All fields optional; absent root => identity (figure at origin).
export function rootTransform(root = {}) {
  const { tx = 0, ty = 0, tz = 0, pitch = 0, yaw = 0, roll = 0 } = root || {};
  const q = Q.multiply(Q.multiply(
    Q.fromAxisAngle([0, 1, 0], deg2rad(yaw)),
    Q.fromAxisAngle([1, 0, 0], deg2rad(pitch))),
    Q.fromAxisAngle([0, 0, 1], deg2rad(roll)));
  return { pos: [tx, ty, tz], quat: q };
}

/** Apply a `{pose:[{joint,movements}], root?:{tx,ty,tz,pitch,yaw,roll}}` command. Returns {bones:[...], warnings}. */
export function applyPose(command) {
  const deltas = new Map();
  const warnings = [];
  for (const entry of command.pose || []) {
    const { joint } = entry;
    if (!BONES.some((b) => b.name === joint)) throw new Error(`unknown joint "${joint}"`);
    const jointSpec = JOINTS[jointKeyFor(joint)];
    if (!jointSpec) throw new Error(`joint "${joint}" has no movable DOF (root or spacer bone)`);
    const perAxis = normalizeMovements(joint, jointSpec, entry.movements);
    const sideSign = joint.endsWith('_L') ? -1 : 1;
    let quat, clamped = false;
    if (jointSpec.constraint === 'swing-twist') {
      ({ quat, clamped } = clampSwingTwist(jointSpec, perAxis, warnings, joint, sideSign));
    } else if (jointSpec.constraint === 'hinge-twist') {
      const hinge = clampBox(jointSpec.axes, { x: perAxis.x, z: 0, y: 0 }, warnings, joint, jointSpec.flip, sideSign);
      let tw = perAxis.y;
      if (tw > jointSpec.twist.max) { warnings.push(`${joint}.twist clamped ${tw}->${jointSpec.twist.max}`); tw = jointSpec.twist.max; }
      if (tw < jointSpec.twist.min) { warnings.push(`${joint}.twist clamped ${tw}->${jointSpec.twist.min}`); tw = jointSpec.twist.min; }
      quat = Q.multiply(hinge, Q.fromAxisAngle([0, 1, 0], deg2rad(tw * sideSign)));
    } else {
      quat = clampBox(jointSpec.axes, perAxis, warnings, joint, jointSpec.flip, sideSign);
    }
    deltas.set(joint, { quat, clamped: clamped || warnings.some((w) => w.startsWith(joint + '.')) });
  }

  // Root 6-DOF: an optional virtual parent of Hips that translates and orients the WHOLE figure in the world.
  // Per-joint FK alone can only fold the body around a fixed pelvis; whole-body motion that travels (steps),
  // turns (pirouette, yaw), or tumbles (a backflip is a 360° pitch through an airborne arc) needs this channel.
  // pitch=X (forward/back somersault), yaw=Y (turn), roll=Z (cartwheel); t* in metres. Keyframed, not simulated.
  const r = rootTransform(command.root);

  const out = [];
  const worldQuat = new Map(), worldPos = new Map();
  for (const b of BONES) {
    const rest = REST.localQuat.get(b.name);
    const delta = deltas.get(b.name);
    const local = delta ? Q.multiply(rest, delta.quat) : rest;
    const parentWq = b.parent ? worldQuat.get(b.parent) : r.quat;
    const parentWp = b.parent ? worldPos.get(b.parent) : r.pos;
    const wq = Q.multiply(parentWq, local);
    const wp = V.add(parentWp, Q.rotate(parentWq, REST.localPos.get(b.name)));
    worldQuat.set(b.name, wq); worldPos.set(b.name, wp);
    out.push({
      name: b.name, parent: b.parent, localQuat: local, worldPos: wp, worldQuat: wq,
      clamped: !!delta?.clamped, warnings: warnings.filter((w) => w.startsWith(b.name + '.') || w.startsWith(b.name + ' ')),
    });
  }
  return { bones: out, warnings };
}

/** The scheme, in the same shape the reference docs describe — the executable mirror `missing-scheme-script` expects. */
export function scheme() {
  return {
    bones: BONES.map((b) => ({ name: b.name, parent: b.parent, length: b.length, dir: b.dir, startOffset: b.startOffset || [0, 0, 0] })),
    joints: Object.fromEntries(Object.entries(JOINTS).map(([k, v]) => [k, v])),
    movements: Object.fromEntries(Object.entries(MOVEMENTS).map(([k, [axis, sign]]) => [k, { axis, sign }])),
  };
}

// ---------- CLI ----------

function printHelp() {
  process.stdout.write(`skeleton.mjs — octocode-mannequin scheme + movement protocol (no npm install needed)

Usage:
  node scripts/skeleton.mjs scheme [--json]
  node scripts/skeleton.mjs pose --cmd '{"pose":[{"joint":"UpperArm_L","movements":{"flexion":90}}]}'
  node scripts/skeleton.mjs pose --file pose.json
  node scripts/skeleton.mjs sequence --file frames.json
  node scripts/skeleton.mjs viewer --out mannequin.html [--pose pose.json]

scheme    print the bone hierarchy + joint DOF/ROM/movement table (JSON by default)
pose      apply a movement-protocol command, print resulting FK transforms + clamp warnings
sequence  validate an animation: an array of pose commands (keyframes); reports per-frame clamps, fails on error
viewer    generate a self-contained Three.js HTML page (see references/viewer-guide.md)
`);
}

async function main() {
  const args = process.argv.slice(2);
  const cmd = args[0];
  const flag = (name) => { const i = args.indexOf(name); return i === -1 ? undefined : args[i + 1]; };
  if (!cmd || args.includes('--help') || args.includes('-h')) return printHelp();

  if (cmd === 'scheme') {
    process.stdout.write(JSON.stringify(scheme(), null, 2) + '\n');
    return;
  }
  if (cmd === 'pose') {
    const { readFileSync } = await import('node:fs');
    const raw = flag('--file') ? readFileSync(flag('--file'), 'utf8') : flag('--cmd');
    if (!raw) { process.stderr.write('error: pose needs --cmd \'<json>\' or --file <path>\n'); process.exitCode = 1; return; }
    try {
      const result = applyPose(JSON.parse(raw));
      process.stdout.write(JSON.stringify(result, null, 2) + '\n');
      if (result.warnings.length) process.stderr.write(result.warnings.map((w) => `warning: ${w}`).join('\n') + '\n');
    } catch (err) {
      process.stderr.write(`error: ${err.message}\n`); process.exitCode = 1;
    }
    return;
  }
  if (cmd === 'sequence') {
    const { readFileSync } = await import('node:fs');
    const raw = flag('--file') ? readFileSync(flag('--file'), 'utf8') : flag('--cmd');
    if (!raw) { process.stderr.write('error: sequence needs --file <path> or --cmd \'[...]\'\n'); process.exitCode = 1; return; }
    let frames;
    try { frames = JSON.parse(raw); } catch (err) { process.stderr.write(`error: invalid JSON: ${err.message}\n`); process.exitCode = 1; return; }
    if (!Array.isArray(frames)) { process.stderr.write('error: a sequence is a JSON array of pose commands (keyframes)\n'); process.exitCode = 1; return; }
    const report = { frames: frames.length, valid: true, keyframes: [] };
    frames.forEach((frame, i) => {
      try { const r = applyPose(frame); report.keyframes.push({ frame: i, ok: true, warnings: r.warnings }); }
      catch (err) { report.valid = false; report.keyframes.push({ frame: i, ok: false, error: err.message }); }
    });
    process.stdout.write(JSON.stringify(report, null, 2) + '\n');
    if (!report.valid) process.exitCode = 1;
    return;
  }
  if (cmd === 'viewer') {
    const { readFileSync, writeFileSync } = await import('node:fs');
    const { fileURLToPath } = await import('node:url');
    const { dirname, join } = await import('node:path');
    const here = dirname(fileURLToPath(import.meta.url));
    const out = flag('--out');
    if (!out) { process.stderr.write('error: viewer needs --out <path.html>\n'); process.exitCode = 1; return; }
    const poseFile = flag('--pose');
    const command = poseFile ? JSON.parse(readFileSync(poseFile, 'utf8')) : { pose: [] };
    const posed = applyPose(command);
    const template = readFileSync(join(here, '..', 'assets', 'viewer.template.html'), 'utf8');
    const html = template
      .replace('"__SCHEME_JSON__"', JSON.stringify(scheme()))
      .replace('"__POSE_JSON__"', JSON.stringify(command))
      .replace('"__RESULT_JSON__"', JSON.stringify(posed));
    writeFileSync(out, html);
    process.stdout.write(`wrote ${out}\n`);
    return;
  }
  process.stderr.write(`error: unknown command "${cmd}"\n`); process.exitCode = 1;
}

if (import.meta.url === `file://${process.argv[1]}`) main();
