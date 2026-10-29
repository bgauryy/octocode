// Dependency-free PNG crop, scale and alpha flatten (8-bit, non-interlaced), so no sips or ImageMagick is needed.
import { deflateSync, inflateSync } from 'node:zlib';

const SIG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const CRC = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; }
  return t;
})();
const crc32 = (buf) => { let c = 0xffffffff; for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };

export function decodePng(buf) {
  if (!buf.subarray(0, 8).equals(SIG)) throw new Error('not a PNG');
  let pos = 8, w, h, depth, type, interlace;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos), name = buf.toString('ascii', pos + 4, pos + 8), body = buf.subarray(pos + 8, pos + 8 + len);
    if (name === 'IHDR') { w = body.readUInt32BE(0); h = body.readUInt32BE(4); depth = body[8]; type = body[9]; interlace = body[12]; }
    else if (name === 'IDAT') idat.push(body);
    else if (name === 'IEND') break;
    pos += 12 + len;
  }
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[type];
  if (depth !== 8 || !channels || interlace) throw new Error(`unsupported PNG (depth ${depth}, type ${type}, interlace ${interlace})`);
  const raw = inflateSync(Buffer.concat(idat)), stride = w * channels, data = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)], src = y * (stride + 1) + 1, dst = y * stride;
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? data[dst + x - channels] : 0, b = y ? data[dst - stride + x] : 0, c = x >= channels && y ? data[dst - stride + x - channels] : 0;
      const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      const pred = [0, a, b, (a + b) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? b : c][f];
      if (f > 4) throw new Error(`bad PNG filter ${f}`);
      data[dst + x] = (raw[src + x] + pred) & 0xff;
    }
  }
  return { w, h, channels, data };
}

const chunk = (name, body) => {
  const out = Buffer.alloc(12 + body.length);
  out.writeUInt32BE(body.length, 0);
  out.write(name, 4, 'ascii');
  body.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + body.length)), 8 + body.length);
  return out;
};

// RGB, 8-bit, no alpha: the model reads this form reliably.
export function encodeRgb(w, h, rgb) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  const raw = Buffer.alloc(h * (w * 3 + 1));
  for (let y = 0; y < h; y++) rgb.copy(raw, y * (w * 3 + 1) + 1, y * w * 3, (y + 1) * w * 3);
  return Buffer.concat([SIG, chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

// Crop to box (clamped), flatten alpha over white, scale by an integer factor (nearest neighbour).
export function cropScale(buf, box = null, scale = 1) {
  const img = decodePng(buf);
  const x0 = Math.max(0, Math.round(box?.x ?? 0)), y0 = Math.max(0, Math.round(box?.y ?? 0));
  const w = Math.min(img.w - x0, Math.round(box?.w ?? img.w)), h = Math.min(img.h - y0, Math.round(box?.h ?? img.h));
  if (w <= 0 || h <= 0) throw new Error('empty crop');
  const out = Buffer.alloc(w * scale * h * scale * 3), ch = img.channels;
  for (let y = 0; y < h * scale; y++) {
    for (let x = 0; x < w * scale; x++) {
      const i = ((y0 + Math.floor(y / scale)) * img.w + x0 + Math.floor(x / scale)) * ch, o = (y * w * scale + x) * 3;
      const a = ch === 4 ? img.data[i + 3] / 255 : ch === 2 ? img.data[i + 1] / 255 : 1;
      for (let k = 0; k < 3; k++) { const v = img.data[i + (ch >= 3 ? k : 0)]; out[o + k] = Math.round(v * a + 255 * (1 - a)); }
    }
  }
  return encodeRgb(w * scale, h * scale, out);
}
