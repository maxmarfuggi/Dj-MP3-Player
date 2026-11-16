// Draws the app icon into apps/desktop/build. Run with: node scripts/writeAppIcon.mjs
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const buildDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'build');

const BG = [18, 20, 28, 255];
const VINYL = [32, 34, 42, 255];
const GROOVE = [58, 62, 74, 255];
const LABEL = [92, 157, 255, 255];
const HOLE = [13, 14, 18, 255];

/** @param {Buffer} buf */
function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

/** @param {string} type @param {Buffer} data */
function chunk(type, data) {
  const typeBuf = Buffer.from(type);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([length, typeBuf, data, crc]);
}

/** @param {number} size @param {(x: number, y: number) => [number, number, number, number]} pixel */
function png(size, pixel) {
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y++) {
    const row = y * stride;
    raw[row] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixel(x, y);
      const i = row + 1 + x * 4;
      raw[i] = r;
      raw[i + 1] = g;
      raw[i + 2] = b;
      raw[i + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

/** Signed distance to a rounded box centered on the origin. */
function sdRoundBox(px, py, half, radius) {
  const qx = Math.abs(px) - half + radius;
  const qy = Math.abs(py) - half + radius;
  return Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
}

/** @param {number} size */
function draw(size) {
  const cx = (size - 1) / 2;
  const half = size * 0.42;
  const corner = size * 0.22;
  const vinylRadius = size * 0.31;
  const labelRadius = size * 0.105;
  const holeRadius = size * 0.028;
  const grooves = [0.9, 0.74, 0.58];
  const grooveWidth = Math.max(size * 0.008, 0.75);

  return png(size, (x, y) => {
    const px = x - cx;
    const py = y - cx;
    const cover = Math.min(Math.max(0.5 - sdRoundBox(px, py, half, corner), 0), 1);
    if (cover <= 0) return [0, 0, 0, 0];

    let [r, g, b, a] = BG;
    const dist = Math.hypot(px, py);

    const vinyl = Math.min(Math.max(vinylRadius - dist + 0.5, 0), 1);
    if (vinyl > 0) {
      r = r + (VINYL[0] - r) * vinyl;
      g = g + (VINYL[1] - g) * vinyl;
      b = b + (VINYL[2] - b) * vinyl;
    }
    for (const groove of grooves) {
      const ring = Math.min(Math.max(1 - Math.abs(dist - vinylRadius * groove) / grooveWidth, 0), 1);
      r = r + (GROOVE[0] - r) * ring;
      g = g + (GROOVE[1] - g) * ring;
      b = b + (GROOVE[2] - b) * ring;
    }
    const label = Math.min(Math.max(labelRadius - dist + 0.5, 0), 1);
    if (label > 0) {
      r = r + (LABEL[0] - r) * label;
      g = g + (LABEL[1] - g) * label;
      b = b + (LABEL[2] - b) * label;
    }
    const hole = Math.min(Math.max(holeRadius - dist + 0.5, 0), 1);
    if (hole > 0) {
      r = r + (HOLE[0] - r) * hole;
      g = g + (HOLE[1] - g) * hole;
      b = b + (HOLE[2] - b) * hole;
    }

    return [Math.round(r), Math.round(g), Math.round(b), Math.round(a * cover)];
  });
}

/** @param {{ size: number, png: Buffer }[]} images */
function ico(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  const dir = Buffer.alloc(16 * images.length);
  let offset = 6 + dir.length;
  const parts = [header, dir];
  images.forEach((image, index) => {
    const entry = index * 16;
    dir[entry] = image.size >= 256 ? 0 : image.size;
    dir[entry + 1] = image.size >= 256 ? 0 : image.size;
    dir.writeUInt16LE(1, entry + 4);
    dir.writeUInt16LE(32, entry + 6);
    dir.writeUInt32LE(image.png.length, entry + 8);
    dir.writeUInt32LE(offset, entry + 12);
    offset += image.png.length;
    parts.push(image.png);
  });
  return Buffer.concat(parts);
}

const master = 1024;
const iconPng = draw(master);
const icoSizes = [16, 32, 48, 256];
mkdirSync(buildDir, { recursive: true });
writeFileSync(path.join(buildDir, 'icon.png'), iconPng);
writeFileSync(
  path.join(buildDir, 'icon.ico'),
  ico(icoSizes.map((size) => ({ size, png: size === master ? iconPng : draw(size) }))),
);
