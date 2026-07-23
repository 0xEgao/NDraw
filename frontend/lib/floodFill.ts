import type { Point } from "./protocol.ts";

/**
 * Replaces the four-connected pixel region containing `at`.
 *
 * The operation mutates `pixels` in place and uses a fixed-size typed queue,
 * avoiding recursion and unbounded JavaScript arrays on a full-canvas fill.
 * Exact RGBA matching keeps antialiased stroke pixels as the region boundary.
 */
export function floodFillPixels(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  at: Point,
  color: number,
): boolean {
  if (width <= 0 || height <= 0 || pixels.length !== width * height * 4) return false;

  const x = Math.max(0, Math.min(width - 1, Math.floor(at.x)));
  const y = Math.max(0, Math.min(height - 1, Math.floor(at.y)));
  const start = y * width + x;
  const startOffset = start * 4;
  const targetRed = pixels[startOffset];
  const targetGreen = pixels[startOffset + 1];
  const targetBlue = pixels[startOffset + 2];
  const targetAlpha = pixels[startOffset + 3];
  const red = (color >>> 16) & 0xff;
  const green = (color >>> 8) & 0xff;
  const blue = color & 0xff;

  if (targetRed === red && targetGreen === green && targetBlue === blue && targetAlpha === 0xff) {
    return false;
  }

  const matchesTarget = (pixel: number): boolean => {
    const offset = pixel * 4;
    return pixels[offset] === targetRed
      && pixels[offset + 1] === targetGreen
      && pixels[offset + 2] === targetBlue
      && pixels[offset + 3] === targetAlpha;
  };
  const replace = (pixel: number): void => {
    const offset = pixel * 4;
    pixels[offset] = red;
    pixels[offset + 1] = green;
    pixels[offset + 2] = blue;
    pixels[offset + 3] = 0xff;
  };

  const queue = new Int32Array(width * height);
  let head = 0;
  let tail = 0;
  queue[tail] = start;
  tail += 1;
  replace(start);

  const visit = (neighbor: number): void => {
    if (!matchesTarget(neighbor)) return;
    replace(neighbor);
    queue[tail] = neighbor;
    tail += 1;
  };

  while (head < tail) {
    const pixel = queue[head];
    head += 1;
    const pixelX = pixel % width;

    if (pixelX > 0) visit(pixel - 1);
    if (pixelX + 1 < width) visit(pixel + 1);
    if (pixel >= width) visit(pixel - width);
    if (pixel + width < width * height) visit(pixel + width);
  }

  return true;
}
