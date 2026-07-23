import assert from "node:assert/strict";
import test from "node:test";
import { floodFillPixels } from "../lib/floodFill.ts";

function whitePixels(width: number, height: number): Uint8ClampedArray {
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let offset = 0; offset < pixels.length; offset += 4) {
    pixels[offset] = 0xff;
    pixels[offset + 1] = 0xff;
    pixels[offset + 2] = 0xff;
    pixels[offset + 3] = 0xff;
  }
  return pixels;
}

function setPixel(pixels: Uint8ClampedArray, width: number, x: number, y: number, color: number): void {
  const offset = (y * width + x) * 4;
  pixels[offset] = (color >>> 16) & 0xff;
  pixels[offset + 1] = (color >>> 8) & 0xff;
  pixels[offset + 2] = color & 0xff;
  pixels[offset + 3] = 0xff;
}

function pixelColor(pixels: Uint8ClampedArray, width: number, x: number, y: number): number {
  const offset = (y * width + x) * 4;
  return (pixels[offset] << 16) | (pixels[offset + 1] << 8) | pixels[offset + 2];
}

test("fills only the region enclosed by a shape boundary", () => {
  const width = 7;
  const height = 7;
  const pixels = whitePixels(width, height);
  for (let index = 1; index <= 5; index += 1) {
    setPixel(pixels, width, index, 1, 0);
    setPixel(pixels, width, index, 5, 0);
    setPixel(pixels, width, 1, index, 0);
    setPixel(pixels, width, 5, index, 0);
  }

  assert.equal(floodFillPixels(pixels, width, height, { x: 3, y: 3 }, 0xff_69_5c), true);
  assert.equal(pixelColor(pixels, width, 3, 3), 0xff_69_5c);
  assert.equal(pixelColor(pixels, width, 0, 0), 0xff_ff_ff);
  assert.equal(pixelColor(pixels, width, 1, 3), 0);
});

test("a fill with the existing color is a no-op", () => {
  const pixels = whitePixels(2, 2);
  assert.equal(floodFillPixels(pixels, 2, 2, { x: 0, y: 0 }, 0xff_ff_ff), false);
});
