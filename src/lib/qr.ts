// A compact, dependency-free QR code encoder (byte mode, error correction M).
//
// Produces a standard QR matrix for the Stage Network URL shown in Settings.
// The implementation follows ISO/IEC 18004 with a precomputed format-information
// table (the robust, well-tested approach) and interleaved Reed-Solomon blocks.

const EXP: number[] = new Array(256);
const LOG: number[] = new Array(256);
(function initGF() {
  let x = 1;
  for (let i = 0; i < 255; i++) {
    EXP[i] = x;
    LOG[x] = i;
    x <<= 1;
    if (x & 0x100) x ^= 0x11d;
  }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
})();

function gMul(a: number, b: number): number {
  if (a === 0 || b === 0) return 0;
  return EXP[(LOG[a]! + LOG[b]!) % 255];
}

// Per version: [totalCodewords, ecCodewordsPerBlock, blockCount]
// Level M (25% EC). Versions 1-7 use a single uniform block group, which keeps
// the block-splitting logic simple. These cover every Stage URL the app prints
// (a URL is ~27 bytes, well within version 2-3). Values from ISO/IEC 18004.
const LEVEL_M: [number, number, number][] = [
  [26, 10, 1], // v1  (data 16)
  [44, 16, 1], // v2  (data 28)
  [70, 26, 1], // v3  (data 44)
  [100, 18, 2], // v4  (data 64)
  [134, 23, 2], // v5  (data 88)
  [172, 16, 4], // v6  (data 108)
  [196, 18, 4], // v7  (data 124)
];

function capacityBytes(version: number): number {
  const [total, ec, blocks] = LEVEL_M[version - 1]!;
  const data = total - ec * blocks;
  // byte mode, versions 1-9 use 8-bit char count
  return Math.floor((data * 8 - 4) / 8) - 8;
}

function chooseVersion(len: number): number {
  // If the content exceeds our supported range, return 0 to signal failure.
  for (let v = 1; v <= LEVEL_M.length; v++) {
    if (len <= capacityBytes(v)) return v;
  }
  return -1;
}

interface RSBlock {
  data: number[];
  ec: number[];
}

function rsEncode(data: number[], ecLen: number): number[] {
  const gen: number[] = [1];
  for (let i = 0; i < ecLen; i++) {
    const next: number[] = new Array(gen.length + 1).fill(0);
    for (let j = 0; j < gen.length; j++) {
      next[j]! ^= gMul(gen[j]!, EXP[i]);
      next[j + 1]! ^= gen[j]!;
    }
    gen.length = 0;
    gen.push(...next);
  }
  const genLen = gen.length;
  const msg = data.slice().concat(new Array(ecLen).fill(0));
  for (let i = 0; i < data.length; i++) {
    const factor = msg[i]!;
    if (factor === 0) continue;
    for (let j = 0; j < genLen; j++) {
      msg[i + j]! ^= gMul(gen[j]!, factor);
    }
  }
  return msg.slice(data.length);
}

/// Precomputed format-information 15-bit values for every (EC-level, mask)
/// combination. Index = (ecBits << 3) | mask, where ecBits are the 2-bit level
/// field (L=01, M=00, H=10, Q=11). These are the standard BCH(15,5) XOR 0x5412
/// outputs from ISO/IEC 18004, verified against known vectors.
const FORMAT_INFO = [
  0x5412, 0x5125, 0x5e7c, 0x5b4b, 0x45f9, 0x40ce, 0x4f97, 0x4aa0, // M (00): masks 0-7
  0x77c4, 0x72f3, 0x7daa, 0x789d, 0x662f, 0x6318, 0x6c41, 0x6976, // L (01): masks 0-7
  0x1689, 0x13be, 0x1ce7, 0x19d0, 0x0762, 0x0255, 0x0d0c, 0x083b, // H (10): masks 0-7
  0x355f, 0x3068, 0x3f31, 0x3a06, 0x24b4, 0x2183, 0x2eda, 0x2bed, // Q (11): masks 0-7
];

function formatBits(ecBits: number, mask: number): number {
  return FORMAT_INFO[(ecBits << 3) | mask]!;
}

function alignmentPositions(version: number): number[] {
  if (version === 1) return [];
  const num = Math.floor(version / 7) + 2;
  const size = version * 4 + 17;
  const step = version === 32 ? 26 : Math.ceil((size - 13) / (num * 2 - 2)) * 2;
  const result = [6];
  for (let pos = size - 7; result.length < num; pos -= step) {
    result.splice(1, 0, pos);
  }
  return result;
}

function buildMatrix(version: number, dataBlocks: RSBlock[], ecBits: number, mask: number): boolean[][] {
  const size = version * 4 + 17;
  const matrix = makeGrid(size);
  const func = makeGrid(size); // function-module map

  // Finder patterns (3 corners) + separators.
  const drawFinder = (row: number, col: number) => {
    for (let r = -1; r <= 7; r++) {
      for (let c = -1; c <= 7; c++) {
        const rr = row + r;
        const cc = col + c;
        if (rr < 0 || rr >= size || cc < 0 || cc >= size) continue;
        const on =
          r >= 0 && r <= 6 && c >= 0 && c <= 6 && (r === 0 || r === 6 || c === 0 || c === 6 || (r >= 2 && r <= 4 && c >= 2 && c <= 4));
        matrix[rr]![cc] = on;
        func[rr]![cc] = true;
      }
    }
  };
  drawFinder(0, 0);
  drawFinder(0, size - 7);
  drawFinder(size - 7, 0);

  // Alignment patterns.
  const positions = alignmentPositions(version);
  for (const r of positions) {
    for (const c of positions) {
      if (func[r]![c]) continue;
      for (let dr = -2; dr <= 2; dr++) {
        for (let dc = -2; dc <= 2; dc++) {
          const rr = r + dr;
          const cc = c + dc;
          // Dark outer ring, light 3x3 center, dark center dot.
          const on = (dr === 0 && dc === 0) || Math.abs(dr) === 2 || Math.abs(dc) === 2;
          matrix[rr]![cc] = on;
          func[rr]![cc] = true;
        }
      }
    }
  }

  // Timing patterns.
  for (let i = 8; i < size - 8; i++) {
    if (!func[6]![i]) {
      matrix[6]![i] = i % 2 === 0;
      func[6]![i] = true;
    }
    if (!func[i]![6]) {
      matrix[i]![6] = i % 2 === 0;
      func[i]![6] = true;
    }
  }

  // Reserve format areas + dark module location so data placement skips them.
  const reserved = [
    [8, 0], [8, 1], [8, 2], [8, 3], [8, 4], [8, 5], [8, 7], [8, 8], [7, 8],
    [6, 8], [5, 8], [4, 8], [3, 8], [2, 8], [1, 8], [0, 8],
    [size - 1, 8], [size - 2, 8], [size - 3, 8], [size - 4, 8], [size - 5, 8], [size - 6, 8], [size - 7, 8], [size - 8, 8],
    [8, size - 8], [8, size - 7], [8, size - 6], [8, size - 5], [8, size - 4], [8, size - 3], [8, size - 2], [8, size - 1],
  ];
  for (const [r, c] of reserved) {
    if (!func[r]![c]) func[r]![c] = true;
  }

  // Build final codeword stream (interleave data then EC within blocks).
  const dataStream: number[] = [];
  const ecStream: number[] = [];
  const maxData = Math.max(...dataBlocks.map((b) => b.data.length));
  const maxEc = Math.max(...dataBlocks.map((b) => b.ec.length));
  for (let i = 0; i < maxData; i++) {
    for (const b of dataBlocks) if (i < b.data.length) dataStream.push(b.data[i]!);
  }
  for (let i = 0; i < maxEc; i++) {
    for (const b of dataBlocks) ecStream.push(b.ec[i]!);
  }
  const allBits: boolean[] = [];
  for (const b of dataStream) for (let i = 7; i >= 0; i--) allBits.push(((b >>> i) & 1) === 1);
  for (const b of ecStream) for (let i = 7; i >= 0; i--) allBits.push(((b >>> i) & 1) === 1);

  // Place data zig-zag, two columns at a time, right to left, skipping the
  // vertical timing column (6). Up/down direction follows the canonical
  // formula `((right + 1) & 2) === 0` per column pair (per ISO/IEC 18004, as
  // implemented by the reference Nayuki QR encoder).
  let bitIndex = 0;
  let right = size - 1;
  while (right >= 1) {
    if (right === 6) right = 5; // skip timing column
    const upward = ((right + 1) & 2) === 0;
    for (let vert = 0; vert < size; vert++) {
      const row = upward ? size - 1 - vert : vert;
      for (let k = 0; k < 2; k++) {
        const c = right - k;
        if (func[row]![c]) continue;
        if (bitIndex < allBits.length) {
          matrix[row]![c] = allBits[bitIndex++]!;
        }
      }
    }
    right -= 2;
  }

  // Apply mask (chosen mask pattern).
  applyMask(matrix, func, mask);

  // Write format info (matches the reference Nayuki drawFormatBits).
  // bits15 is the 15-bit value; getBit(bits, i) = LSB index i. Here arr[i] = bit i.
  const bits15 = formatBits(ecBits, mask);
  const arr: number[] = [];
  for (let i = 0; i < 15; i++) arr.push((bits15 >>> i) & 1);

  // First copy (around top-left finder). Coordinates: set(x, y) with x=col, y=row.
  for (let i = 0; i <= 5; i++) matrix[i]![8] = arr[i] === 1; // (row i, col 8)
  matrix[7]![8] = arr[6] === 1; // (row 7, col 8)
  matrix[8]![8] = arr[7] === 1; // (row 8, col 8)
  matrix[8]![7] = arr[8] === 1; // (row 8, col 7)
  for (let i = 9; i < 15; i++) matrix[8]![14 - i] = arr[i] === 1; // row 8, cols 5..0

  // Second copy (top-right horizontal row 8 + bottom-left vertical col 8).
  for (let i = 0; i < 8; i++) matrix[8]![size - 1 - i] = arr[i] === 1;
  for (let i = 8; i < 15; i++) matrix[size - 15 + i]![8] = arr[i] === 1;
  matrix[size - 8]![8] = true; // dark module

  return matrix;
}

function applyMask(matrix: boolean[][], func: boolean[][], mask: number): void {
  const size = matrix.length;
  for (let r = 0; r < size; r++) {
    for (let c = 0; c < size; c++) {
      if (func[r]![c]) continue;
      let invert = false;
      switch (mask) {
        case 0: invert = (r + c) % 2 === 0; break;
        case 1: invert = r % 2 === 0; break;
        case 2: invert = c % 3 === 0; break;
        case 3: invert = (r + c) % 3 === 0; break;
        case 4: invert = (Math.floor(r / 2) + Math.floor(c / 3)) % 2 === 0; break;
        case 5: invert = ((r * c) % 2) + ((r * c) % 3) === 0; break;
        case 6: invert = (((r * c) % 2) + ((r * c) % 3)) % 2 === 0; break;
        case 7: invert = (((r + c) % 2) + ((r * c) % 3)) % 2 === 0; break;
      }
      if (invert) matrix[r]![c] = !matrix[r]![c];
    }
  }
}

function makeGrid(size: number): boolean[][] {
  return Array.from({ length: size }, () => new Array(size).fill(false));
}

/**
 * Encode a string into a QR matrix (byte mode, error correction M, mask 0).
 * Returns `null` if the input is too large.
 */
export function qrMatrix(text: string): boolean[][] | null {
  const bytes = utf8Bytes(text);
  const version = chooseVersion(bytes.length);
  if (version < 1) return null;
  const [total, ecPerBlock, blockCount] = LEVEL_M[version - 1]!;
  const dataTotal = total - ecPerBlock * blockCount;

  // Build data bit stream.
  const bitStream: number[] = [];
  // byte mode indicator
  pushBits(bitStream, 0b0100, 4);
  // char count (8 bits for v1-9)
  pushBits(bitStream, bytes.length, 8);
  for (const b of bytes) pushBits(bitStream, b, 8);
  // terminator
  const capacity = dataTotal * 8;
  pushBits(bitStream, 0, Math.min(4, capacity - bitStream.length));
  // pad to byte boundary
  while (bitStream.length % 8 !== 0) bitStream.push(0);
  // pad codewords 0xEC 0x11
  let dataBytes: number[] = [];
  for (let i = 0; i < bitStream.length; i += 8) {
    let v = 0;
    for (let j = 0; j < 8; j++) v = (v << 1) | bitStream[i + j]!;
    dataBytes.push(v);
  }
  const pad = [0xec, 0x11];
  let pi = 0;
  while (dataBytes.length < dataTotal) {
    dataBytes.push(pad[pi++ % 2]!);
  }

  // Split into `blockCount` equal blocks (level M uses uniform blocks for v1-9).
  const perBlock = Math.floor(dataTotal / blockCount);
  const dataBlocks: RSBlock[] = [];
  for (let b = 0; b < blockCount; b++) {
    const start = b * perBlock;
    const blockData = dataBytes.slice(start, start + perBlock);
    dataBlocks.push({ data: blockData, ec: rsEncode(blockData, ecPerBlock) });
  }

  return buildMatrix(version, dataBlocks, 0, 0);
}

function pushBits(out: number[], value: number, count: number): void {
  for (let i = count - 1; i >= 0; i--) {
    out.push((value >>> i) & 1);
  }
}

function utf8Bytes(text: string): number[] {
  const out: number[] = [];
  for (let i = 0; i < text.length; i++) {
    const c = text.codePointAt(i)!;
    if (c < 0x80) {
      out.push(c);
    } else if (c < 0x800) {
      out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
    } else if (c < 0x10000) {
      out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
    } else {
      out.push(
        0xf0 | (c >> 18),
        0x80 | ((c >> 12) & 0x3f),
        0x80 | ((c >> 6) & 0x3f),
        0x80 | (c & 0x3f),
      );
    }
    if (c > 0xffff) i++;
  }
  return out;
}

/**
 * Render a QR matrix as an SVG data URI with a quiet zone on a light background.
 */
export function qrSvgUri(text: string, modules = 8, size = 360): string {
  const matrix = qrMatrix(text);
  if (!matrix) return "";
  const n = matrix.length;
  const quiet = 4;
  const total = n + quiet * 2;
  const scale = size / total;

  let rects = "";
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      if (matrix[r]![c]) {
        const x = (c + quiet) * scale;
        const y = (r + quiet) * scale;
        rects += `<rect x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${(scale + 0.05).toFixed(2)}" height="${(scale + 0.05).toFixed(2)}"/>`;
      }
    }
  }
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">` +
    `<rect width="${size}" height="${size}" fill="#ffffff"/>` +
    `<g fill="#000000">${rects}</g>` +
    `</svg>`;
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg);
}
