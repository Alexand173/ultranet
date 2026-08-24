"use client";

import { ArrowRight, Send } from "lucide-react";
import { FormEvent, useState } from "react";
import type { AccountSnapshot, FeeEstimate } from "@/lib/api";
import { formatUltraAmount, isUltraAddress, parseUltraAmount } from "@/lib/transaction-format";
import FeeReceipt from "@/components/transactions/FeeReceipt";

interface SendTransactionProps {
  account: AccountSnapshot;
  recipient: string;
  amountInput: string;
  estimate: FeeEstimate | null;
  estimateError: string;
  estimating: boolean;
  submitError: string;
  onRecipientChange: (value: string) => void;
  onAmountChange: (value: string) => void;
  onReview: () => void;
}

export default function SendTransaction({ account, recipient, amountInput, estimate, estimateError, estimating, submitError, onRecipientChange, onAmountChange, onReview }: SendTransactionProps) {
  const [touched, setTouched] = useState({ recipient: false, amount: false });
  const parsedAmount = parseUltraAmount(amountInput);
  const recipientError = touched.recipient && recipient.trim() && !isUltraAddress(recipient) ? "Enter a valid UltraNet address." : "";
  const amountError = touched.amount ? parsedAmount.error ?? "" : "";
  const totalCost = estimate ? BigInt(estimate.total) : null;
  const balance = BigInt(account.balance);
  const insufficient = totalCost !== null && totalCost > balance;
  const canReview = Boolean(isUltraAddress(recipient) && parsedAmount.value !== null && estimate && !estimateError && !insufficient);

  const useMax = () => {
    const available = BigInt(account.balance);
    let amount = available * 100n / 101n;
    while (amount > 0n && amount + (amount / 100n || 1n) > available) amount -= 1n;
    onAmountChange(formatUltraAmount(amount));
    setTouched((current) => ({ ...current, amount: true }));
  };

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setTouched({ recipient: true, amount: true });
    if (canReview) onReview();
  };

  return (
    <section aria-labelledby="send-ultra-title" className="border-t border-platinum/15 pt-8">
      <div className="flex items-start justify-between gap-6"><div><p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Transfer // local signature</p><h2 id="send-ultra-title" className="mt-3 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum">Send ULTRA</h2></div><Send className="h-6 w-6 text-cyan-glow" aria-hidden="true" /></div>
      <p className="mt-4 max-w-xl text-sm leading-7 text-platinum/55">Enter the recipient and amount. You will review the complete transfer before your local wallet signs it.</p>

      {(submitError || estimateError) && <div role="alert" aria-live="polite" className="mt-6 border border-red-300/40 bg-red-300/10 px-4 py-3 font-mono text-xs leading-6 text-red-200">{submitError || estimateError}</div>}

      <form className="mt-8 max-w-2xl space-y-7" onSubmit={handleSubmit} noValidate aria-busy={estimating}>
        <div className="space-y-2">
          <label htmlFor="transaction-recipient" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Recipient address</label>
          <input id="transaction-recipient" name="recipient" type="text" inputMode="text" autoComplete="off" spellCheck={false} required value={recipient} onChange={(event) => onRecipientChange(event.target.value)} onBlur={() => setTouched((current) => ({ ...current, recipient: true }))} aria-invalid={Boolean(recipientError)} aria-describedby={recipientError ? "transaction-recipient-error" : "transaction-recipient-help"} placeholder="64-character UltraNet address" className="h-14 w-full border border-platinum/15 bg-platinum/[0.03] px-4 font-mono text-sm text-platinum outline-hidden transition-colors placeholder:text-platinum/20 focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40 aria-[invalid=true]:border-red-300" />
          {recipientError ? <p id="transaction-recipient-error" role="alert" className="font-mono text-xs text-red-300">{recipientError}</p> : <p id="transaction-recipient-help" className="font-mono text-[10px] leading-5 text-platinum/40">Paste the UltraNet address that should receive this transfer.</p>}
        </div>

        <div className="space-y-2">
          <div className="flex items-center justify-between gap-4"><label htmlFor="transaction-amount" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Amount</label><button type="button" onClick={useMax} className="min-h-11 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow underline decoration-cyan-glow/40 underline-offset-4 hover:text-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">Use max</button></div>
          <div className="flex items-center border border-platinum/15 bg-platinum/[0.03] transition-colors focus-within:border-cyan-glow focus-within:ring-1 focus-within:ring-cyan-glow/40"><input id="transaction-amount" name="amount" type="text" inputMode="decimal" autoComplete="off" required value={amountInput} onChange={(event) => onAmountChange(event.target.value)} onBlur={() => setTouched((current) => ({ ...current, amount: true }))} aria-invalid={Boolean(amountError)} aria-describedby={amountError ? "transaction-amount-error" : "transaction-amount-help"} placeholder="0.000000" className="h-14 min-w-0 flex-1 border-0 bg-transparent px-4 font-mono text-lg tabular-nums text-platinum outline-hidden placeholder:text-platinum/20 focus:ring-0" /><span className="pr-4 font-mono text-xs text-cyan-glow">ULTRA</span></div>
          {amountError ? <p id="transaction-amount-error" role="alert" className="font-mono text-xs text-red-300">{amountError}</p> : <p id="transaction-amount-help" className="font-mono text-[10px] leading-5 text-platinum/40">Available: {formatUltraAmount(balance)} ULTRA. Use up to six decimal places.</p>}
        </div>

        <FeeReceipt amount={parsedAmount.value} balance={balance} estimate={estimate} estimating={estimating} />
        {insufficient && <p className="font-mono text-xs leading-5 text-red-300" role="alert">The amount and fee exceed your available balance.</p>}
        {!estimating && !estimate && !estimateError && parsedAmount.value !== null && isUltraAddress(recipient) && <p className="font-mono text-xs leading-5 text-amber-200">The network fee is currently unavailable. Try again before reviewing.</p>}

        <button type="submit" disabled={!canReview || estimating} className="group inline-flex min-h-11 w-full items-center justify-center gap-3 bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black transition-colors hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black disabled:cursor-not-allowed disabled:opacity-45 sm:w-auto">Review transfer <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" aria-hidden="true" /></button>
      </form>
    </section>
  );
}
