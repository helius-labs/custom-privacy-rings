import { relative } from "node:path";

import { beforeAll, beforeEach, expect, vi } from "vitest";
import { sha256 } from "@noble/hashes/sha2.js";
import { concatBytes, utf8ToBytes } from "@noble/hashes/utils.js";

import { initializePoseidon } from "@heliuslabs/spr";

const SEED_ENV = "ZOLANA_RANDOM_SEED";
const U64_MAX = (1n << 64n) - 1n;
// The spy sits on the instance, the prototype method stays native.
const nativeRandom: Crypto["getRandomValues"] = Object.getPrototypeOf(
  globalThis.crypto,
).getRandomValues.bind(globalThis.crypto);
const seed = parseSeed(process.env[SEED_ENV]);

// File hooks draw under the file path, each test under its full name.
seedRandom(relative(process.cwd(), expect.getState().testPath ?? ""));

beforeAll(async () => {
  await initializePoseidon();
});

beforeEach(({ task, onTestFailed }) => {
  seedRandom(task.fullName);
  onTestFailed(() => {
    console.error(
      `${SEED_ENV}=0x${seed.toString(16)} npx vitest run ${task.file.name} -t ${shellQuote(namePattern(task.fullTestName))}`,
    );
  });
});

function parseSeed(raw: string | undefined): bigint {
  const text = raw?.trim() ?? "";
  if (text === "") return nativeRandom(new BigUint64Array(1))[0] as bigint;
  if (!/^(0x[0-9a-f]+|[0-9]+)$/iu.test(text) || BigInt(text) > U64_MAX)
    throw new Error(`${SEED_ENV} must be a decimal or 0x hex u64, got ${raw}`);
  return BigInt(text);
}

/** Draws under `label` replay from the seed and their call order. */
function seedRandom(label: string): void {
  const key = sha256(utf8ToBytes(`${seed}\n${label}`));
  const counter = new DataView(new ArrayBuffer(8));
  let block = new Uint8Array(0);
  let offset = 0;
  const seeded = (array: ArrayBufferView<ArrayBuffer>): ArrayBufferView<ArrayBuffer> => {
    // The native call keeps its type and quota refusals.
    nativeRandom(array);
    const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
    for (let index = 0; index < bytes.length; index += 1) {
      if (offset === block.length) {
        block = sha256(concatBytes(key, new Uint8Array(counter.buffer)));
        counter.setBigUint64(0, counter.getBigUint64(0, true) + 1n, true);
        offset = 0;
      }
      bytes[index] = block[offset] as number;
      offset += 1;
    }
    return array;
  };
  vi.spyOn(globalThis.crypto, "getRandomValues").mockImplementation(
    seeded as Crypto["getRandomValues"],
  );
}

function namePattern(name: string): string {
  return `^${name.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}$`;
}

function shellQuote(text: string): string {
  return `'${text.replaceAll("'", `'\\''`)}'`;
}
