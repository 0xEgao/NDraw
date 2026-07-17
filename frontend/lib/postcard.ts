/**
 * Small, defensive Postcard codec for NDraw's browser client.
 *
 * Rust remains the protocol authority. This module implements only the wire
 * primitives used by `ndraw-proto`; it is intentionally not a general Serde
 * implementation. All integers on this protocol stay below Number.MAX_SAFE_INTEGER.
 */
export class Writer {
  readonly #bytes: number[] = [];

  finish(): Uint8Array<ArrayBuffer> {
    const buffer = new ArrayBuffer(this.#bytes.length);
    const output = new Uint8Array(buffer);
    output.set(this.#bytes);
    return output;
  }

  u8(value: number): this {
    if (!Number.isInteger(value) || value < 0 || value > 0xff) throw new RangeError(`u8 out of range: ${value}`);
    this.#bytes.push(value);
    return this;
  }

  varint(value: number): this {
    if (!Number.isSafeInteger(value) || value < 0) throw new RangeError(`invalid unsigned integer: ${value}`);
    let remaining = value;
    while (remaining >= 0x80) {
      this.#bytes.push((remaining % 128) | 0x80);
      remaining = Math.floor(remaining / 128);
    }
    this.#bytes.push(remaining);
    return this;
  }

  signedVarint(value: number): this {
    if (!Number.isSafeInteger(value)) throw new RangeError(`invalid signed integer: ${value}`);
    return this.varint(value >= 0 ? value * 2 : -value * 2 - 1);
  }

  bool(value: boolean): this {
    return this.u8(value ? 1 : 0);
  }

  string(value: string): this {
    const encoded = new TextEncoder().encode(value);
    this.varint(encoded.length);
    for (const byte of encoded) this.#bytes.push(byte);
    return this;
  }

  fixed(bytes: Uint8Array): this {
    for (const byte of bytes) this.#bytes.push(byte);
    return this;
  }

  option<T>(value: T | null, write: (writer: Writer, item: T) => void): this {
    if (value === null) return this.u8(0);
    this.u8(1);
    write(this, value);
    return this;
  }

  vector<T>(values: readonly T[], write: (writer: Writer, item: T) => void): this {
    this.varint(values.length);
    for (const value of values) write(this, value);
    return this;
  }
}

export class Reader {
  readonly #bytes: Uint8Array;
  #offset = 0;

  constructor(bytes: Uint8Array) {
    this.#bytes = bytes;
  }

  get remaining(): number {
    return this.#bytes.length - this.#offset;
  }

  u8(): number {
    const value = this.#bytes[this.#offset];
    if (value === undefined) throw new Error("postcard frame ended unexpectedly");
    this.#offset += 1;
    return value;
  }

  varint(): number {
    let result = 0;
    let multiplier = 1;
    for (let index = 0; index < 10; index += 1) {
      const byte = this.u8();
      result += (byte & 0x7f) * multiplier;
      if (!Number.isSafeInteger(result)) throw new Error("postcard integer exceeds JavaScript's safe range");
      if ((byte & 0x80) === 0) return result;
      multiplier *= 128;
    }
    throw new Error("postcard varint is too long");
  }

  signedVarint(): number {
    const value = this.varint();
    return value % 2 === 0 ? value / 2 : -(value + 1) / 2;
  }

  bool(): boolean {
    const value = this.u8();
    if (value !== 0 && value !== 1) throw new Error(`postcard boolean has invalid tag ${value}`);
    return value === 1;
  }

  string(): string {
    const length = this.varint();
    const end = this.#offset + length;
    if (end > this.#bytes.length) throw new Error("postcard string extends beyond the frame");
    const value = new TextDecoder("utf-8", { fatal: true }).decode(this.#bytes.subarray(this.#offset, end));
    this.#offset = end;
    return value;
  }

  fixed(length: number): Uint8Array {
    const end = this.#offset + length;
    if (end > this.#bytes.length) throw new Error("postcard fixed array extends beyond the frame");
    const value = this.#bytes.slice(this.#offset, end);
    this.#offset = end;
    return value;
  }

  option<T>(read: (reader: Reader) => T): T | null {
    const tag = this.u8();
    if (tag === 0) return null;
    if (tag === 1) return read(this);
    throw new Error(`postcard option has invalid tag ${tag}`);
  }

  vector<T>(read: (reader: Reader) => T): T[] {
    const length = this.varint();
    if (length > this.remaining + 1) throw new Error("postcard collection length is not plausible for this frame");
    return Array.from({ length }, () => read(this));
  }

  finish(): void {
    if (this.remaining !== 0) throw new Error(`postcard frame contains ${this.remaining} trailing bytes`);
  }
}
