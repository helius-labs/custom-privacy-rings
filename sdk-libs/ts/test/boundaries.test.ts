import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve, sep } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = resolve(__dirname, "../src");

/** Root modules are unconstrained, `crypto` and `protocol` sit below every other layer. */
const ALLOWED: Readonly<Record<string, readonly string[]>> = {
  crypto: ["protocol"],
  protocol: ["crypto"],
  keypair: ["crypto"],
  proof: ["crypto", "operations", "protocol", "rpc"],
  rpc: ["crypto", "operations", "protocol"],
  state: ["crypto", "operations", "protocol", "rpc"],
  operations: ["crypto", "proof", "protocol", "rpc", "state"],
  client: ["protocol", "rpc"],
  ring: ["crypto", "operations", "proof", "protocol", "rpc", "state"],
};

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return walk(path);
    return entry.endsWith(".ts") ? [path] : [];
  });
}

function layerOf(path: string): string {
  const rel = path.slice(SRC.length + 1);
  return rel.includes(sep) ? (rel.split(sep)[0] as string) : "<root>";
}

describe("module boundaries", () => {
  it("keeps every import inside its layer's allowed set", () => {
    const violations: string[] = [];
    for (const file of walk(SRC)) {
      const layer = layerOf(file);
      if (layer === "<root>") continue;
      const allowed = ALLOWED[layer];
      expect(allowed, `unknown layer ${layer}`).toBeDefined();
      for (const match of readFileSync(file, "utf8").matchAll(/from "(\.[^"]+)"/gu)) {
        const target = resolve(file, "..", match[1] as string);
        if (!target.startsWith(SRC)) continue;
        const targetLayer = layerOf(target.endsWith(".js") ? target : join(target, "x"));
        if (targetLayer === layer) continue;
        if (!(allowed as readonly string[]).includes(targetLayer)) {
          violations.push(`${file.slice(SRC.length + 1)} -> ${targetLayer}`);
        }
      }
    }
    expect(violations).toEqual([]);
  });
});
