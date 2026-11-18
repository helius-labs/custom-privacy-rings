import { generateKeyPairSync, randomBytes, randomUUID, sign } from "node:crypto";
import { createServer } from "node:http";

const port = Number(process.argv[2] ?? 8787);
if (!Number.isInteger(port) || port < 0 || port > 65535) {
  throw new Error("invalid local KMS port");
}

const ed25519 = "ECC_NIST_EDWARDS25519";
const ed25519Algorithm = "ED25519_SHA_512";
const keys = new Map();
const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

function createKey({ KeySpec = "SYMMETRIC_DEFAULT", KeyUsage = "ENCRYPT_DECRYPT" }) {
  const KeyId = randomUUID();
  const Arn = `arn:aws:kms:us-east-1:000000000000:key/${KeyId}`;
  if (KeySpec === ed25519 && KeyUsage === "SIGN_VERIFY") {
    keys.set(Arn, generateKeyPairSync("ed25519"));
  } else if (KeySpec === "SYMMETRIC_DEFAULT" && KeyUsage === "ENCRYPT_DECRYPT") {
    keys.set(Arn, { dataKeys: new Map() });
  } else {
    throw new Error("invalid key usage");
  }
  return { KeyMetadata: { KeyId, Arn, KeySpec, KeyUsage, Enabled: true, KeyState: "Enabled" } };
}

function encryptionContext(value) {
  if (!object(value) || !Object.values(value).every((field) => typeof field === "string")) {
    throw new Error("invalid encryption context");
  }
  return JSON.stringify(Object.keys(value).sort().map((key) => [key, value[key]]));
}

function dispatch(operation, input) {
  if (!object(input)) throw new Error("invalid request");
  if (operation === "TrentService.CreateKey") return createKey(input);
  const { KeyId, EncryptionContext } = input;
  const key = keys.get(KeyId);
  if (key?.privateKey) {
    if (operation === "TrentService.GetPublicKey") {
      return {
        KeyId, KeySpec: ed25519, KeyUsage: "SIGN_VERIFY", SigningAlgorithms: [ed25519Algorithm],
        PublicKey: key.publicKey.export({ type: "spki", format: "der" }).toString("base64"),
      };
    }
    if (operation === "TrentService.Sign" && input.SigningAlgorithm === ed25519Algorithm &&
        (input.MessageType ?? "RAW") === "RAW" && typeof input.Message === "string") {
      const message = Buffer.from(input.Message, "base64");
      if (message.length < 1 || message.length > 4096) throw new Error("invalid message length");
      return { KeyId, SigningAlgorithm: ed25519Algorithm, Signature: sign(null, message, key.privateKey).toString("base64") };
    }
  }
  if (key?.dataKeys) {
    const context = encryptionContext(EncryptionContext);
    if (operation === "TrentService.GenerateDataKeyWithoutPlaintext" && input.KeySpec === "AES_256") {
      const CiphertextBlob = randomBytes(32).toString("base64");
      key.dataKeys.set(CiphertextBlob, { context, secret: randomBytes(32) });
      return { KeyId, CiphertextBlob };
    }
    if (operation === "TrentService.Decrypt") {
      const row = key.dataKeys.get(input.CiphertextBlob);
      if (row?.context === context) return { KeyId, Plaintext: row.secret.toString("base64") };
    }
  }
  throw new Error("invalid key or operation");
}

const server = createServer(async (request, response) => {
  response.setHeader("Content-Type", "application/x-amz-json-1.1");
  response.setHeader("Cache-Control", "no-store");
  try {
    if (request.headers.host !== `127.0.0.1:${server.address().port}` ||
        request.headers.origin !== undefined || request.method !== "POST" || request.url !== "/" ||
        request.headers["content-type"] !== "application/x-amz-json-1.1") {
      throw new Error("invalid local KMS request");
    }
    const chunks = [];
    let size = 0;
    for await (const chunk of request) {
      size += chunk.length;
      if (size > 16384) throw new Error("request too large");
      chunks.push(chunk);
    }
    const result = dispatch(request.headers["x-amz-target"], JSON.parse(Buffer.concat(chunks)));
    response.end(JSON.stringify(result));
  } catch {
    response.writeHead(400);
    response.end(JSON.stringify({ __type: "InvalidParameterException", message: "invalid local KMS request" }));
  }
});
server.listen(port, "127.0.0.1", () => console.log(`127.0.0.1:${server.address().port}`));
