import { ArrowDownLeft, ArrowUpRight, ExternalLink } from "lucide-react";
import type { TransactionRecord } from "@/lib/api";
import { EXPLORER_URL } from "@/lib/links";
import { formatAddress, formatTimestamp, formatUltraAmount } from "@/lib/transaction-format";

interface TransactionHistoryProps {
  address: string;
  transactions: TransactionRecord[];
  loading: boolean;
  error: string;
}

export default function TransactionHistory({ address, transactions, loading, error }: TransactionHistoryProps) {
  return (
    <section aria-labelledby="transaction-history-title" className="border-t border-platinum/15 pt-8">
      <div className="flex items-end justify-between gap-4"><div><p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Ledger // address history</p><h2 id="transaction-history-title" className="mt-3 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum">Recent transactions</h2></div><a href={EXPLORER_URL} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/45 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">Open Explorer <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" /></a></div>
      {loading ? <div className="mt-6 border-y border-platinum/15" aria-live="polite"><span className="sr-only">Reading transaction history…</span>{[1, 2, 3].map((item) => <div key={item} className="h-20 border-b border-platinum/10 last:border-b-0" aria-hidden="true"><div className="mx-4 mt-7 h-2 w-2/3 bg-platinum/[0.04]" /></div>)}</div> : error ? <div className="mt-6 border border-red-300/30 bg-red-300/10 p-5" role="alert"><p className="font-space-grotesk text-xl font-bold uppercase text-red-200">History unavailable</p><p className="mt-3 text-sm leading-6 text-red-100/75">{error}</p></div> : transactions.length === 0 ? <div className="mt-6 border-y border-platinum/15 py-12"><p className="font-space-grotesk text-xl font-bold uppercase text-platinum">No transactions yet</p><p className="mt-3 text-sm leading-6 text-platinum/50">Transfers sent or received by this wallet will appear here.</p></div> : <div className="mt-6 border-y border-platinum/15">{transactions.map((transaction) => <TransactionRow key={`${transaction.hash}-${transaction.status}`} address={address} transaction={transaction} />)}</div>}
    </section>
  );
}

function TransactionRow({ address, transaction }: { address: string; transaction: TransactionRecord }) {
  const sent = transaction.sender === address;
  const amount = `${sent ? "−" : "+"}${formatUltraAmount(BigInt(transaction.amount))} ULTRA`;
  const statusClass = transaction.status === "pending" ? "text-amber-200" : transaction.status === "failed" ? "text-red-300" : "text-platinum/55";
  const counterparty = sent ? transaction.recipient : transaction.sender;
  return <article className="grid gap-3 border-b border-platinum/10 py-5 last:border-b-0 md:grid-cols-[28px_1fr_auto_auto_auto] md:items-center md:gap-4"><div className={sent ? "text-cyan-glow" : "text-emerald-300"}>{sent ? <ArrowUpRight className="h-4 w-4" aria-label="Sent" /> : <ArrowDownLeft className="h-4 w-4" aria-label="Received" />}</div><div className="min-w-0"><p className="font-space-grotesk text-sm font-bold uppercase tracking-tight text-platinum">{sent ? "Sent" : "Received"}</p><p className="mt-1 truncate font-mono text-[10px] text-platinum/45" title={counterparty}>{sent ? "To" : "From"} {formatAddress(counterparty)}</p></div><p className="font-mono text-xs tabular-nums text-platinum/80 md:text-right">{amount}</p><p className="font-mono text-[10px] text-platinum/40 md:text-right">{formatTimestamp(transaction.timestamp)}</p><div className="flex items-center justify-between gap-4 md:contents"><span className={`font-mono text-[10px] uppercase tracking-[0.12em] ${statusClass}`}>{transaction.status}</span><a href={`${EXPLORER_URL}?search=${encodeURIComponent(transaction.hash)}`} target="_blank" rel="noreferrer" aria-label={`View transaction ${transaction.id} in Explorer`} className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-platinum/40 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">View <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" /></a></div></article>;
}
