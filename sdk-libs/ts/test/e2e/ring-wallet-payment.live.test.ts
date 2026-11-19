import { generateKeyPairSigner } from "@solana/kit";
import { describe, expect, it } from "vitest";

import { buildDepositTransaction, buildRingPaymentTransaction, SOL_MINT } from "@heliuslabs/spr";
import type { Bytes32 } from "@heliuslabs/spr/interface";

import { anonymousRingTransfers, syncRingWallet } from "../../src/ring/index.js";
import { RingAddress } from "../../src/crypto/keys/ring-address.js";
import { AnonymousWallet } from "../../src/state/anonymous-wallet.js";
import { ViewingHistory } from "../../src/state/viewing-history.js";
import { currentSlot, signSendAndConfirm, type Actor } from "./live-helpers.js";
import {
  airdrop,
  enroll,
  freshActor,
  readableRing,
  ringHoldings,
  ringKeys,
  type ReadableRing,
} from "./ring-live-helpers.js";

const DEPOSIT = 1_000_000_000n;
const PAYMENT = 250_000_000n;

async function syncOn(ring: ReadableRing, actor: Actor, document: AnonymousWallet): Promise<void> {
  await syncRingWallet({
    client: ring.client,
    wallet: document,
    keys: ringKeys(actor),
    config: { requireSlot: await currentSlot(ring.client) },
  });
}

function watching(ring: ReadableRing, actor: Actor): AnonymousWallet {
  const history = ViewingHistory.create(actor.wallet.identity.viewingPublicKey, 0n);
  return AnonymousWallet.create(actor.wallet, history.withRing(ring.ringProgramId));
}

describe("core wallet payment into an anonymous ring", () => {
  it("lands through the wallet builder, the recipient and the sender's change found by one sync each", async () => {
    const ring = await readableRing({ kind: "anonymous" });
    const funder = await generateKeyPairSigner();
    const [sender, recipient] = [await freshActor(ring.client), await freshActor(ring.client)];
    await Promise.all([
      airdrop(ring.client, funder.address),
      airdrop(ring.client, sender.signer.address),
    ]);
    const [senderDocument, recipientDocument] = [watching(ring, sender), watching(ring, recipient)];

    const deposit = await buildDepositTransaction({
      client: ring.client,
      feePayer: sender.signer.address,
      recipient: sender.keypair.shieldedAddress(),
      amount: DEPOSIT,
    });
    await signSendAndConfirm(ring.client, deposit, [sender.signer]);
    await syncOn(ring, sender, senderDocument);
    await enroll({ ring, owner: sender });

    const to = new RingAddress({
      recipient: recipient.keypair.shieldedAddress(),
      ringProgramId: ring.ringProgramId,
      listSalt: new Uint8Array(32).fill(1) as Bytes32,
    });
    const payment = await buildRingPaymentTransaction({
      client: ring.client,
      ring: anonymousRingTransfers({
        client: ring.client,
        ringProgramId: ring.ringProgramId,
        assets: sender.wallet.registry,
        payees: [to],
      }),
      wallet: sender.wallet,
      keys: sender.keys,
      feePayer: funder.address,
      owner: sender.keypair,
      recipient: to.recipient,
      amount: PAYMENT,
    });
    await signSendAndConfirm(ring.client, payment, [funder]);

    await syncOn(ring, recipient, recipientDocument);
    expect(ringHoldings(recipient, ring.ringProgramId)).toBe(PAYMENT);
    await syncOn(ring, sender, senderDocument);
    expect(sender.wallet.balance(SOL_MINT).amount).toBe(0n);
    expect(ringHoldings(sender, ring.ringProgramId)).toBe(DEPOSIT - PAYMENT);
  }, 900_000);
});
