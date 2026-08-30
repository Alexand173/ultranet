"use client";

import Link from "next/link";
import { AlertTriangle, ArrowLeft, Check } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import {
  fetchAccount,
  fetchAddressTransactions,
  fetchTransactionStatus,
  estimateTransactionFee,
  submitSignedTransaction,
  type AccountSnapshot,
  type TransactionRecord,
} from "@/lib/api";
import {
  decryptWalletSeed,
  deriveIdentityFromStoredSeed,
  storedPublicKeyToBytes,
} from "@/lib/wallet-crypto";
import { useWalletSession } from "@/components/wallet/WalletSessionProvider";
import WalletConnectionStatus from "@/components/wallet/WalletConnectionStatus";
import EducationalWalletCallout from "@/components/wallet/EducationalWalletCallout";
import {
  buildTransactionRequest,
  isSignedTransaction,
  ULTRA_WALLET_SIGN_TRANSACTION,
  ULTRA_WALLET_TRANSACTION_VERSION,
  type SignedTransaction,
} from "@/lib/ultra-wallet";
import { isUltraAddress, parseUltraAmount, toSafeNumber } from "@/lib/transaction-format";
import WalletSetup from "@/components/transactions/WalletSetup";
import WalletLocked from "@/components/transactions/WalletLocked";
import WalletLedger from "@/components/transactions/WalletLedger";
import SendTransaction from "@/components/transactions/SendTransaction";
import TransactionHistory from "@/components/transactions/TransactionHistory";
import TransactionReview from "@/components/transactions/TransactionReview";
import TransactionResult from "@/components/transactions/TransactionResult";
import type { TransferDraft, TransferState } from "@/components/transactions/transaction.types";
import type { LocalWalletKeyMaterial, StoredWallet } from "@/lib/wallet-crypto";

const EMPTY_DRAFT: TransferDraft = {
  recipient: "",
  amountInput: "",
  amountBaseUnits: null,
  feeEstimate: null,
  estimateError: "",
};

type NetworkState = "loading" | "connected" | "delayed" | "unavailable";

export default function TransactionPage({ returnTo }: { returnTo?: string }) {
  const router = useRouter();
  const { sessionState, storedWallet, walletAddress, storageError, localProvider, unlock, lock } = useWalletSession();
  const [account, setAccount] = useState<AccountSnapshot | null>(null);
  const [history, setHistory] = useState<TransactionRecord[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [networkState, setNetworkState] = useState<NetworkState>("loading");
  const [draft, setDraft] = useState<TransferDraft>(EMPTY_DRAFT);
  const [transferState, setTransferState] = useState<TransferState>("idle");
  const [result, setResult] = useState<TransactionRecord | null>(null);
  const [transferError, setTransferError] = useState("");
  const [pageError, setPageError] = useState("");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const signedTransactionRef = useRef<SignedTransaction | null>(null);
  const accountRequestRef = useRef<AbortController | null>(null);
  const reviewHeadingRef = useRef<HTMLHeadingElement>(null);
  const resultHeadingRef = useRef<HTMLHeadingElement>(null);
  const accountRef = useRef<AccountSnapshot | null>(null);


  const loadAccountData = useCallback(async (address: string) => {
    accountRequestRef.current?.abort();
    const controller = new AbortController();
    accountRequestRef.current = controller;
    setIsRefreshing(true);
    setNetworkState("loading");
    setHistoryLoading(true);

    const [accountResult, historyResult] = await Promise.allSettled([
      fetchAccount(address, controller.signal),
      fetchAddressTransactions(address, 20, controller.signal),
    ]);
    if (controller.signal.aborted) return;

    if (accountResult.status === "fulfilled") {
      accountRef.current = accountResult.value;
      setAccount(accountResult.value);
      setPageError("");
      setNetworkState("connected");
    } else {
      setNetworkState(accountRef.current ? "delayed" : "unavailable");
      setPageError(accountResult.reason instanceof Error ? accountResult.reason.message : "Unable to read the wallet account.");
    }

    if (historyResult.status === "fulfilled") {
      setHistory(historyResult.value);
      setHistoryError("");
    } else if (!controller.signal.aborted) {
      setHistory([]);
      setHistoryError(historyResult.reason instanceof Error ? historyResult.reason.message : "Unable to read transaction history.");
    }
    setHistoryLoading(false);
    setIsRefreshing(false);
  }, []);

  useEffect(() => () => {
    accountRequestRef.current?.abort();
  }, []);

  useEffect(() => {
    if (sessionState !== "unlocked" || !walletAddress || account?.address === walletAddress) return;
    setAccount(null);
    setHistory([]);
    setHistoryError("");
    void loadAccountData(walletAddress);
  }, [account?.address, loadAccountData, sessionState, walletAddress]);

  useEffect(() => {
    if (transferState === "reviewing" || transferState === "signing" || transferState === "submitting") {
      window.setTimeout(() => reviewHeadingRef.current?.focus(), 0);
    }
    if (transferState === "pending" || transferState === "success" || transferState === "failed" || transferState === "unknown") {
      window.setTimeout(() => resultHeadingRef.current?.focus(), 0);
    }
  }, [transferState]);

  useEffect(() => {
    if (sessionState !== "unlocked" || !account || transferState === "reviewing" || transferState === "signing" || transferState === "submitting" || transferState === "pending" || transferState === "success" || transferState === "failed" || transferState === "unknown") return;
    const parsed = parseUltraAmount(draft.amountInput);
    if (!isUltraAddress(draft.recipient) || parsed.value === null) {
      setDraft((current) => current.feeEstimate || current.estimateError ? { ...current, feeEstimate: null, estimateError: "" } : current);
      if (transferState === "ready" || transferState === "estimating-fee") setTransferState("idle");
      return;
    }

    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      setTransferState("estimating-fee");
      void estimateTransactionFee(draft.recipient.trim(), toSafeNumber(parsed.value, "Amount"), controller.signal)
        .then((estimate) => {
          if (controller.signal.aborted) return;
          setDraft((current) => ({ ...current, amountBaseUnits: parsed.value, feeEstimate: estimate, estimateError: "" }));
          setTransferState("ready");
        })
        .catch((error) => {
          if (controller.signal.aborted) return;
          setDraft((current) => ({ ...current, amountBaseUnits: parsed.value, feeEstimate: null, estimateError: error instanceof Error ? error.message : "The network fee is currently unavailable." }));
          setTransferState("idle");
        });
    }, 250);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [account, draft.amountInput, draft.recipient, sessionState, transferState]);

  const continueToReturnPath = useCallback(() => {
    if (returnTo) router.push(returnTo);
  }, [returnTo, router]);

  const handleUnlock = (material: LocalWalletKeyMaterial) => {
    unlock(material);
    setPageError("");
    setAccount(null);
    setHistory([]);
    setHistoryError("");
    continueToReturnPath();
  };

  const handleCreated = async (wallet: StoredWallet, password: string) => {
    const seed = await decryptWalletSeed(wallet.encryptedSeed, password);
    try {
      const material = await deriveIdentityFromStoredSeed(seed, storedPublicKeyToBytes(wallet), wallet.address, wallet.createdAt);
      unlock(material, wallet);
      setPageError("");
      setAccount(null);
      setHistory([]);
      setHistoryError("");
      continueToReturnPath();
    } finally {
      seed.fill(0);
    }
  };

  const handleRefresh = () => {
    if (walletAddress) void loadAccountData(walletAddress);
  };

  const updateDraft = (updates: Partial<TransferDraft>) => {
    setDraft((current) => ({ ...current, ...updates, ...(updates.recipient !== undefined || updates.amountInput !== undefined ? { feeEstimate: null, estimateError: "", amountBaseUnits: null } : {}) }));
    setTransferError("");
  };

  const handleReview = () => {
    if (!account || draft.amountBaseUnits === null || !draft.feeEstimate) return;
    if (BigInt(draft.feeEstimate.total) > BigInt(account.balanceBaseUnits)) {
      setTransferError("The amount and fee exceed your available balance.");
      return;
    }
    setTransferError("");
    setTransferState("reviewing");
  };

  const submitSignedTransfer = async (signed: SignedTransaction) => {
    setTransferState("submitting");
    try {
      const submitted = await submitSignedTransaction(signed);
      setResult(submitted);
      setTransferState(submitted.status === "confirmed" ? "success" : "pending");
      if (storedWallet) void loadAccountData(storedWallet.address);
      return submitted;
    } catch (error) {
      if (error instanceof TypeError || (error instanceof Error && error.name === "AbortError")) {
        setTransferError("The network response was interrupted. Do not send it again yet.");
        setTransferState("unknown");
      } else {
        signedTransactionRef.current = null;
        setTransferError(error instanceof Error ? error.message : "The node rejected this transfer.");
        setTransferState("failed");
      }
      return null;
    }
  };

  const handleConfirm = async () => {
    if (!account || draft.amountBaseUnits === null || !draft.feeEstimate || transferState !== "reviewing") return;
    setTransferError("");
    signedTransactionRef.current = null;
    setTransferState("signing");
    const nullifier = crypto.getRandomValues(new Uint8Array(32));
    const signedParams = {
      recipient: draft.recipient.trim(),
      amount: toSafeNumber(draft.amountBaseUnits, "Amount"),
      fee: draft.feeEstimate.fee,
      nonce: account.nonce,
      timestamp: Math.floor(Date.now() / 1000),
      nullifier: Array.from(nullifier),
      gasLimit: draft.feeEstimate.gasLimit,
      gasPrice: draft.feeEstimate.gasPrice,
      chainId: 0 as const,
      version: ULTRA_WALLET_TRANSACTION_VERSION,
    };
    const provider = localProvider;
    if (!provider) {
      nullifier.fill(0);
      setTransferError("The local wallet is locked. Unlock it before signing the transfer.");
      setTransferState("failed");
      return;
    }

    try {
      const response = await provider.request({ method: ULTRA_WALLET_SIGN_TRANSACTION, params: signedParams });
      if (!isSignedTransaction(response)) throw new Error("The local wallet returned an incomplete transaction signature.");
      const signed = buildTransactionRequest(response);
      signedTransactionRef.current = signed;
      await submitSignedTransfer(signed);
    } catch (error) {
      setTransferError(error instanceof Error ? error.message : "Unable to sign the transfer locally.");
      setTransferState("failed");
    } finally {
      nullifier.fill(0);
    }
  };

  const handleCheckStatus = async () => {
    const signed = signedTransactionRef.current;
    if (result?.hash) {
      setTransferState("submitting");
      try {
        const status = await fetchTransactionStatus(result.hash);
        setResult(status);
        setTransferState(status.status === "confirmed" ? "success" : status.status === "failed" ? "failed" : "pending");
      } catch (error) {
        setTransferError(error instanceof Error ? error.message : "Unable to check the transaction status.");
        setTransferState("unknown");
      }
      return;
    }
    if (signed) {
      await submitSignedTransfer(signed);
      return;
    }
    setTransferError("No transaction identity is available yet. Keep the signed transfer and try the status check again.");
  };

  const handleEdit = () => {
    signedTransactionRef.current = null;
    setTransferError("");
    setTransferState("idle");
  };

  const handleTryAgain = () => {
    setTransferError("");
    if (signedTransactionRef.current) {
      void submitSignedTransfer(signedTransactionRef.current);
      return;
    }
    setResult(null);
    setTransferState("reviewing");
  };

  const handleSendAnother = () => {
    setDraft(EMPTY_DRAFT);
    setResult(null);
    setTransferError("");
    signedTransactionRef.current = null;
    setTransferState("idle");
    if (walletAddress) void loadAccountData(walletAddress);
  };

  const handleLock = () => {
    lock();
    accountRequestRef.current?.abort();
    setDraft(EMPTY_DRAFT);
    setResult(null);
    setTransferError("");
    signedTransactionRef.current = null;
    setAccount(null);
    setHistory([]);
    setTransferState("idle");
  };

  if (sessionState === "checking") {
    return <main className="relative min-h-screen bg-ink-black px-6 pb-24 pt-36 sm:px-10 lg:px-12"><div className="mx-auto max-w-7xl border-y border-platinum/15 py-16"><p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">WALLET_STATUS / READING_LOCAL_STORAGE</p><p className="mt-4 text-sm text-platinum/50">Checking this device for a protected UltraNet wallet…</p></div></main>;
  }

  if (storageError) {
    return <main className="relative min-h-screen bg-ink-black px-6 pb-24 pt-36 sm:px-10 lg:px-12"><div className="mx-auto max-w-3xl border border-red-300/40 bg-red-300/10 p-6 sm:p-8" role="alert"><AlertTriangle className="h-6 w-6 text-red-300" aria-hidden="true" /><h1 className="mt-5 font-space-grotesk text-2xl font-bold uppercase text-red-200">Local wallet unavailable</h1><p className="mt-3 text-sm leading-7 text-red-100/80">{pageError || storageError}</p></div></main>;
  }

  if (sessionState === "none") {
    return <main className="relative min-h-screen overflow-x-clip bg-ink-black terminal-overlay"><div className="mx-auto max-w-[1680px] px-6 pt-28 sm:px-10 lg:px-12"><SendUltraReturnBanner returnTo={returnTo} /><EducationalWalletCallout variant="dark" placement="send-ultra" /></div><WalletSetup onCreated={handleCreated} /></main>;
  }

  if (sessionState === "locked" && storedWallet) {
    return <main className="relative min-h-screen overflow-x-clip bg-ink-black terminal-overlay"><div className="mx-auto max-w-[1680px] px-6 pt-28 sm:px-10 lg:px-12"><SendUltraReturnBanner returnTo={returnTo} /><EducationalWalletCallout variant="dark" placement="send-ultra" /></div><WalletLocked wallet={storedWallet} onUnlocked={handleUnlock} onCreated={handleCreated} /></main>;
  }

  if (!account || !walletAddress || !storedWallet) {
    return <main className="relative min-h-screen bg-ink-black px-6 pb-24 pt-36 sm:px-10 lg:px-12"><div className="mx-auto max-w-3xl border-y border-platinum/15 py-16"><p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">WALLET_STATUS / WAITING_FOR_ACCOUNT</p><p className="mt-4 text-sm text-platinum/50">Unlock the wallet to read its account.</p></div></main>;
  }

  const inReview = transferState === "reviewing" || transferState === "signing" || transferState === "submitting";
  const resultState = ["pending", "success", "failed", "unknown"].includes(transferState) ? transferState as Extract<TransferState, "pending" | "success" | "failed" | "unknown"> : null;
  const busy = transferState === "signing" || transferState === "submitting";

  return <main className="relative min-h-screen overflow-x-clip bg-ink-black terminal-overlay pb-24">
    <section className="relative mt-20 border-b border-platinum/10 px-6 py-12 sm:px-10 lg:px-12 lg:py-16"><div className="pointer-events-none absolute right-0 top-0 h-full w-1/3 dot-grid opacity-15" aria-hidden="true" /><div className="relative z-10 mx-auto flex max-w-[1680px] flex-col justify-between gap-6 md:flex-row md:items-end"><div><p className="font-mono text-xs uppercase tracking-[0.24em] text-cyan-glow">WALLET // SEND ULTRA // LOCAL</p><h1 className="mt-4 font-space-grotesk text-4xl font-bold uppercase tracking-tight text-platinum sm:text-6xl">Send Ultra</h1><p className="mt-4 max-w-2xl text-sm leading-7 text-platinum/55">Create or unlock your local wallet, review every amount and address, and sign a $ULTRA transfer on this device.</p></div><div className="flex flex-col items-start gap-3 md:items-end"><WalletConnectionStatus variant="dark" /><div className="flex items-center gap-3 font-mono text-[10px] uppercase tracking-[0.14em] text-emerald-300"><Check className="h-4 w-4" aria-hidden="true" /> Protected locally</div></div></div></section>
    <div className="relative z-10 mx-auto max-w-[1680px] px-6 pt-8 sm:px-10 lg:px-12"><SendUltraReturnBanner returnTo={returnTo} /><EducationalWalletCallout variant="dark" placement="send-ultra" /></div>
    <WalletLedger account={account} networkState={networkState} onRefresh={handleRefresh} onLock={handleLock} />
    {pageError && <div className="relative z-10 mx-auto mt-6 max-w-[1680px] px-6 sm:px-10 lg:px-12"><div role="alert" className="border border-amber-300/30 bg-amber-300/10 px-4 py-3 font-mono text-xs leading-6 text-amber-100">{pageError}</div></div>}
    <div className="relative z-10 mx-auto grid max-w-[1680px] gap-12 px-6 py-12 sm:px-10 lg:grid-cols-[7fr_5fr] lg:gap-10 lg:px-12 lg:py-16">
      <div className="min-w-0">{inReview && draft.amountBaseUnits !== null && draft.feeEstimate ? <TransactionReview account={account} recipient={draft.recipient} amount={draft.amountBaseUnits} estimate={draft.feeEstimate} busy={busy} onConfirm={handleConfirm} onEdit={handleEdit} headingRef={reviewHeadingRef} /> : resultState ? <TransactionResult state={resultState} result={result} error={transferError} onSendAnother={handleSendAnother} onTryAgain={handleTryAgain} onEdit={handleEdit} onCheckStatus={handleCheckStatus} headingRef={resultHeadingRef} /> : <SendTransaction account={account} recipient={draft.recipient} amountInput={draft.amountInput} estimate={draft.feeEstimate} estimateError={draft.estimateError} estimating={transferState === "estimating-fee"} submitError={transferError} onRecipientChange={(value) => updateDraft({ recipient: value })} onAmountChange={(value) => updateDraft({ amountInput: value })} onReview={handleReview} />}</div>
      <div className="min-w-0 border-l-0 border-platinum/10 lg:border-l lg:pl-10"><TransactionHistory address={account.address} transactions={history} loading={historyLoading || isRefreshing} error={historyError} /></div>
    </div>
    <div className="sr-only" aria-live="polite">{networkState === "connected" ? "Account data updated." : networkState === "delayed" ? "Balance update delayed." : ""}</div>
  </main>;
}

function SendUltraReturnBanner({ returnTo }: { returnTo?: string }) {
  if (!returnTo) return null;

  return (
    <div className="mb-5 flex flex-col gap-3 border border-cyan-glow/25 bg-cyan-glow/[0.04] px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
      <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow/80">Validator proposal paused // unlock this wallet, then return to signing</p>
      <Link href={returnTo} className="inline-flex min-h-11 shrink-0 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
        <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" /> Return to proposal
      </Link>
    </div>
  );
}
