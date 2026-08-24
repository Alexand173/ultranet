import type { FeeEstimate } from "@/lib/api";
import { formatUltraAmount } from "@/lib/transaction-format";

interface FeeReceiptProps {
  amount: bigint | null;
  balance: bigint;
  estimate: FeeEstimate | null;
  estimating: boolean;
}

export default function FeeReceipt({ amount, balance, estimate, estimating }: FeeReceiptProps) {
  const total = estimate ? BigInt(estimate.total) : null;
  const balanceAfter = total !== null && balance >= total ? balance - total : null;

  return (
    <dl className="border-y border-platinum/15 py-4 font-mono text-xs tabular-nums">
      <div className="flex items-center justify-between gap-4 py-2"><dt className="text-platinum/45">Amount</dt><dd className="text-right text-platinum/80">{amount === null ? "—" : `${formatUltraAmount(amount)} ULTRA`}</dd></div>
      <div className="flex items-center justify-between gap-4 py-2"><dt className="text-platinum/45">Network fee</dt><dd className="text-right text-platinum/80">{estimating ? "Estimating…" : estimate ? `${formatUltraAmount(BigInt(estimate.fee))} ULTRA` : "—"}</dd></div>
      <div className="my-2 border-t border-platinum/10" />
      <div className="flex items-center justify-between gap-4 py-2 font-bold"><dt className="text-platinum/70">Total</dt><dd className="text-right text-platinum">{total === null ? "—" : `${formatUltraAmount(total)} ULTRA`}</dd></div>
      <div className="flex items-center justify-between gap-4 py-2"><dt className="text-platinum/45">Balance after</dt><dd className="text-right text-platinum/80">{balanceAfter === null ? "—" : `${formatUltraAmount(balanceAfter)} ULTRA`}</dd></div>
    </dl>
  );
}
