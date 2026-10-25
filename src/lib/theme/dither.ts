const BAYER_8 = [
  0, 32, 8, 40, 2, 34, 10, 42,
  48, 16, 56, 24, 50, 18, 58, 26,
  12, 44, 4, 36, 14, 46, 6, 38,
  60, 28, 52, 20, 62, 30, 54, 22,
  3, 35, 11, 43, 1, 33, 9, 41,
  51, 19, 59, 27, 49, 17, 57, 25,
  15, 47, 7, 39, 13, 45, 5, 37,
  63, 31, 55, 23, 61, 29, 53, 21
];

export function applyBayerDither(
  value: number,
  x: number,
  y: number,
  amplitude = 1
): number {
  const threshold = (BAYER_8[(y & 7) * 8 + (x & 7)] + 0.5) / 64 - 0.5;
  return Math.max(0, Math.min(255, Math.round(value + threshold * amplitude)));
}
