import { describe, expect, it } from "vitest";
import { normalizeMempool } from "@/lib/api";

const PENDING_ROW = {
  id: "752a92417a1180c9",
  hash: "752a92417a1180c9f6744979bb70ad07d5a3e5670e23c1ef5e476a8937ff0281",
  sender: "fe0b9a9227ebb2bb40b408c5b10b0155a2591fdf97ce43580a1ab873271af1f8",
  recipient: "2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed",
  amount: 1_000_000,
  fee: 10_000,
  nonce: 1,
  timestamp: 1_793_457_224,
  status: "pending",
};

describe("normalizeMempool", () => {
  it("keeps pending rows and trusts the node's depth count", () => {
    const snapshot = normalizeMempool({
      success: true,
      count: 4,
      transactions: [PENDING_ROW],
    });

    expect(snapshot.count).toBe(4);
    expect(snapshot.transactions).toHaveLength(1);
    expect(snapshot.transactions[0]).toMatchObject({
      hash: PENDING_ROW.hash,
      sender: PENDING_ROW.sender,
      recipient: PENDING_ROW.recipient,
      amount: 1_000_000,
      status: "pending",
    });
  });

  it("defaults a row without a status to pending", () => {
    const withoutStatus: Record<string, unknown> = { ...PENDING_ROW };
    delete withoutStatus.status;

    const snapshot = normalizeMempool({ success: true, transactions: [withoutStatus] });

    expect(snapshot.transactions[0].status).toBe("pending");
    expect(snapshot.count).toBe(1);
  });

  it("drops rows that are missing required fields", () => {
    const snapshot = normalizeMempool({
      success: true,
      count: 2,
      transactions: [PENDING_ROW, { hash: "abc" }],
    });

    expect(snapshot.transactions).toHaveLength(1);
    expect(snapshot.count).toBe(2);
  });

  it("treats a missing transaction array as an empty mempool", () => {
    expect(normalizeMempool({ success: true })).toEqual({ count: 0, transactions: [] });
  });

  it("throws when the node reports the mempool as unavailable", () => {
    expect(() =>
      normalizeMempool({ success: false, message: "Mempool unavailable: disk" }),
    ).toThrow("Mempool unavailable: disk");
  });
});
