const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const vlq = n => {
  let v = n < 0 ? (-n << 1) | 1 : n << 1;
  let out = '';
  do {
    let d = v & 31;
    v >>>= 5;
    if (v) d |= 32;
    out += B64[d];
  } while (v);
  return out;
};
/** Segments are absolute [genCol, src, srcLine, srcCol, name?] per generated line. */
export const encodeMappings = lines => {
  const prev = [0, 0, 0, 0];
  return lines
    .map(segs => {
      let genCol = 0;
      return segs
        .map(([gc, si, sl, sc, ni]) => {
          const f = [gc - genCol, si - prev[0], sl - prev[1], sc - prev[2]];
          genCol = gc;
          [prev[0], prev[1], prev[2]] = [si, sl, sc];
          if (ni !== undefined) {
            f.push(ni - prev[3]);
            prev[3] = ni;
          }
          return f.map(vlq).join('');
        })
        .join(',');
    })
    .join(';');
};

export const handMap = {
  version: 3,
  sources: ['src/a.ts', 'src/b.ts'],
  names: ['foo'],
  sourcesContent: ['const a = 1;\n', 'const b = 2;\n'],
  mappings: encodeMappings([
    [
      [0, 0, 0, 0],
      [10, 0, 2, 4, 0],
    ],
    [[0, 1, 5, 2]],
  ]),
};
