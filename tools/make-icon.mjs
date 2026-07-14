// Draws the WinTilex mark and writes it as a PNG, for `tauri icon` to slice up.
// The same shape lives in assets/logo.svg, which is the one to edit by hand;
// keep the rectangles below in step with it. Kept dependency free so
// regenerating the icon never needs an install.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

const SIZE = 1024;
const ACCENT = [47, 111, 235];
const ACCENT_DIM = [120, 160, 245];
const RADIUS = 44;

const pixels = new Uint8Array(SIZE * SIZE * 4);

function roundedRect(x0, y0, x1, y1, radius, [r, g, b]) {
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const dx = Math.max(x0 + radius - x, x - (x1 - 1 - radius), 0);
      const dy = Math.max(y0 + radius - y, y - (y1 - 1 - radius), 0);
      if (dx * dx + dy * dy > radius * radius) continue;
      const offset = (y * SIZE + x) * 4;
      pixels[offset] = r;
      pixels[offset + 1] = g;
      pixels[offset + 2] = b;
      pixels[offset + 3] = 255;
    }
  }
}

// One tall tile on the left, two stacked on the right: the layout WinTilex
// produces for three windows.
roundedRect(128, 128, 472, 896, RADIUS, ACCENT);
roundedRect(552, 128, 896, 488, RADIUS, ACCENT_DIM);
roundedRect(552, 536, 896, 896, RADIUS, ACCENT_DIM);

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body) >>> 0);
  return Buffer.concat([length, body, crc]);
}

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let c = -1;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return c ^ -1;
}

const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0;
  Buffer.from(pixels.buffer, y * SIZE * 4, SIZE * 4).copy(
    raw,
    y * (SIZE * 4 + 1) + 1,
  );
}

const header = Buffer.alloc(13);
header.writeUInt32BE(SIZE, 0);
header.writeUInt32BE(SIZE, 4);
header[8] = 8;
header[9] = 6;

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", header),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/source.png", png);
console.log(`wrote src-tauri/icons/source.png (${png.length} bytes)`);
