/** Bit pack/unpack matching ford-core `as_built::{get_bits,set_bits}` (LSB-first). */

export const getBits = (
  data: number[],
  byte: number,
  bit: number,
  width: number,
): number | null => {
  if (width < 1 || width > 64 || bit > 7) return null;
  const startBit = byte * 8 + bit;
  const endBit = startBit + width;
  const needBytes = Math.ceil(endBit / 8);
  if (data.length < needBytes) return null;
  let value = 0;
  for (let i = 0; i < width; i++) {
    const abs = startBit + i;
    const b = data[Math.floor(abs / 8)] ?? 0;
    if ((b >> (abs % 8)) & 1) value |= 1 << i;
  }
  return value >>> 0;
};

export const setBits = (
  data: number[],
  byte: number,
  bit: number,
  width: number,
  value: number,
): number[] | null => {
  if (width < 1 || width > 64 || bit > 7) return null;
  const maxVal = width === 64 ? Number.MAX_SAFE_INTEGER : (1 << width) - 1;
  if (value < 0 || value > maxVal) return null;
  const startBit = byte * 8 + bit;
  const endBit = startBit + width;
  const needBytes = Math.ceil(endBit / 8);
  if (data.length < needBytes) return null;
  const out = [...data];
  for (let i = 0; i < width; i++) {
    const abs = startBit + i;
    const idx = Math.floor(abs / 8);
    const bitI = abs % 8;
    const mask = 1 << bitI;
    if ((value >> i) & 1) out[idx] |= mask;
    else out[idx] &= ~mask;
  }
  return out.map((b) => b & 0xff);
};

export const bytesToHex = (data: number[]): string =>
  data.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join("");

export const hexToBytes = (hex: string): number[] | null => {
  const cleaned = hex.replace(/\s+/g, "").toUpperCase();
  if (!cleaned || cleaned.length % 2 !== 0 || /[^0-9A-F]/.test(cleaned)) {
    return null;
  }
  const out: number[] = [];
  for (let i = 0; i < cleaned.length; i += 2) {
    out.push(parseInt(cleaned.slice(i, i + 2), 16));
  }
  return out;
};
