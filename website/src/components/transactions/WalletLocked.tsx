"use client";

import { AlertTriangle, ArrowLeft, ArrowRight, KeyRound, LockKeyhole } from "lucide-react";
import { FormEvent, useEffect, useRef, useState } from "react";
import {
  decryptWalletSeed,
  deriveIdentityFromStoredSeed,
  RECOVERY_PHRASE_WORD_COUNT,
  storedPublicKeyToBytes,
  type LocalWalletKeyMaterial,
  type StoredWallet,
} from "@/lib/wallet-crypto";
import WalletSetup from "@/components/transactions/WalletSetup";

interface WalletLockedProps {
  wallet: StoredWallet;
  onUnlocked: (material: LocalWalletKeyMaterial) => void;
  onCreated: (wallet: StoredWallet, password: string) => Promise<void> | void;
}

type WalletLockedView = "locked" | "restore" | "replace-confirm" | "replace-create";

export default function WalletLocked({ wallet, onUnlocked, onCreated }: WalletLockedProps) {
  const [view, setView] = useState<WalletLockedView>("locked");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<"idle" | "unlocking" | "error">("idle");
  const [error, setError] = useState("");
  const [replacementAcknowledged, setReplacementAcknowledged] = useState(false);
  const recoveryHeadingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (view === "replace-confirm") recoveryHeadingRef.current?.focus();
  }, [view]);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    setStatus("unlocking");

    try {
      const seed = await decryptWalletSeed(wallet.encryptedSeed, password);
      try {
        const material = await deriveIdentityFromStoredSeed(
          seed,
          storedPublicKeyToBytes(wallet),
          wallet.address,
          wallet.createdAt,
        );
        setPassword("");
        setStatus("idle");
        onUnlocked(material);
      } finally {
        seed.fill(0);
      }
    } catch (unlockError) {
      setPassword("");
      setStatus("error");
      setError(unlockError instanceof Error ? unlockError.message : "That password did not unlock this wallet.");
    }
  };

  const openRecoveryView = (nextView: Exclude<WalletLockedView, "locked">) => {
    setPassword("");
    setStatus("idle");
    setError("");
    setView(nextView);
  };

  const returnToLocked = () => {
    setPassword("");
    setStatus("idle");
    setView("locked");
    setReplacementAcknowledged(false);
    setError("");
  };

  if (view === "restore") {
    return (
      <WalletSetup
        key="restore-wallet"
        initialMode="restore"
        allowModeToggle={false}
        replacement
        restoreTarget={wallet}
        onCancel={returnToLocked}
        onCreated={onCreated}
      />
    );
  }

  if (view === "replace-create") {
    return (
      <WalletSetup
        key="replace-wallet"
        initialMode="create"
        allowModeToggle={false}
        replacement
        onCancel={returnToLocked}
        onCreated={onCreated}
      />
    );
  }

  if (view === "replace-confirm") {
    return (
      <section className="relative mx-auto max-w-3xl border-y border-platinum/15 px-6 py-14 sm:px-10 sm:py-20" aria-labelledby="wallet-replace-title">
        <div className="absolute left-0 top-0 h-full w-1 bg-amber-200/70" aria-hidden="true" />
        <p className="font-mono text-[10px] uppercase tracking-[0.22em] text-amber-200">WALLET_RECOVERY / NEW_KEY</p>
        <AlertTriangle className="mt-6 h-7 w-7 text-amber-200" aria-hidden="true" />
        <h1 ref={recoveryHeadingRef} id="wallet-replace-title" tabIndex={-1} className="mt-5 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-4xl">Create a brand-new wallet?</h1>
        <p className="mt-4 max-w-2xl text-sm leading-7 text-platinum/65">This is the fallback when the old wallet password and {RECOVERY_PHRASE_WORD_COUNT}-word recovery phrase are both unavailable. UltraNet cannot reset the local password or recover that old wallet.</p>

        <div className="mt-8 border border-amber-300/30 bg-amber-300/10 p-5 text-sm leading-7 text-amber-100/85">
          <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-amber-200">Before you continue</p>
          <ul className="mt-3 space-y-2">
            <li><span className="mr-2 text-amber-200">01</span>A new wallet creates a new Dilithium key pair, recovery phrase, and address.</li>
            <li><span className="mr-2 text-amber-200">02</span>The old address and any funds at it will not be restored or moved into the new wallet.</li>
            <li><span className="mr-2 text-amber-200">03</span>The existing browser record stays untouched until the new encrypted wallet is successfully saved.</li>
          </ul>
        </div>

        <label htmlFor="confirm-wallet-replacement" className="mt-7 flex min-h-11 items-start gap-3 text-sm leading-6 text-platinum/75">
          <input
            id="confirm-wallet-replacement"
            type="checkbox"
            checked={replacementAcknowledged}
            onChange={(event) => setReplacementAcknowledged(event.target.checked)}
            className="mt-1 h-4 w-4 accent-cyan-glow"
          />
          <span>I understand that the old wallet cannot be recovered without its recovery phrase, and that this creates a separate wallet with a new address.</span>
        </label>

        <div className="mt-8 flex flex-col gap-3 sm:flex-row sm:items-center">
          <button type="button" onClick={returnToLocked} className="inline-flex min-h-11 items-center justify-center gap-2 border border-platinum/15 px-5 py-3 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/60 transition-colors hover:border-cyan-glow/50 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
            <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" /> Back to locked wallet
          </button>
          <button type="button" disabled={!replacementAcknowledged} onClick={() => setView("replace-create")} className="inline-flex min-h-11 items-center justify-center gap-2 bg-cyan-glow px-5 py-3 font-mono text-[10px] font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black disabled:cursor-not-allowed disabled:opacity-40">
            I understand — create new wallet <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="relative mx-auto max-w-3xl border-y border-platinum/15 px-6 py-14 sm:px-10 sm:py-20" aria-labelledby="wallet-locked-title">
      <div className="absolute left-0 top-0 h-full w-1 bg-cyan-glow/60" aria-hidden="true" />
      <p className="font-mono text-[10px] uppercase tracking-[0.22em] text-cyan-glow">WALLET_STATUS / LOCKED</p>
      <LockKeyhole className="mt-6 h-7 w-7 text-platinum/80" aria-hidden="true" />
      <h1 id="wallet-locked-title" className="mt-5 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-4xl">Wallet locked</h1>
      <p className="mt-4 max-w-xl text-sm leading-7 text-platinum/60">Enter your wallet password to view the balance or send ULTRA.</p>

      <form className="mt-8 max-w-md space-y-5" onSubmit={handleSubmit} noValidate aria-busy={status === "unlocking"}>
        <div className="space-y-2">
          <label htmlFor="wallet-unlock-password" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/55">Wallet password</label>
          <input
            id="wallet-unlock-password"
            name="walletPassword"
            type="password"
            autoComplete="current-password"
            required
            value={password}
            onChange={(event) => {
              setPassword(event.target.value);
              setError("");
              setStatus("idle");
            }}
            aria-invalid={Boolean(error)}
            aria-describedby={error ? "wallet-unlock-error" : "wallet-unlock-help"}
            className="h-14 w-full border border-platinum/15 bg-platinum/[0.03] px-4 font-mono text-sm text-platinum outline-hidden transition-colors placeholder:text-platinum/20 focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40 aria-[invalid=true]:border-red-300"
          />
          {error ? <p id="wallet-unlock-error" role="alert" className="font-mono text-xs leading-5 text-red-300">{error}</p> : <p id="wallet-unlock-help" className="font-mono text-[10px] leading-5 text-platinum/40">This password protects the wallet on this device.</p>}
        </div>
        <button type="submit" disabled={status === "unlocking" || password.length === 0} className="inline-flex min-h-11 w-full items-center justify-center bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black transition-colors hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black disabled:cursor-wait disabled:opacity-50">
          {status === "unlocking" ? "Unlocking wallet…" : "Unlock wallet"}
        </button>
      </form>

      <section className="mt-10 border-t border-platinum/10 pt-7" aria-labelledby="wallet-recovery-title">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Forgot your wallet password?</p>
        <h2 id="wallet-recovery-title" className="mt-3 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum">Choose a recovery path</h2>
        <p className="mt-3 max-w-2xl text-sm leading-7 text-platinum/55"><strong className="text-platinum/80">Your {RECOVERY_PHRASE_WORD_COUNT}-word recovery phrase is required</strong> if this device or password is lost. UltraNet cannot reset this local password. Use the phrase to restore this same wallet, or create a new wallet if the phrase is gone too.</p>

        <div className="mt-6 grid gap-4 md:grid-cols-2">
          <article className="border border-cyan-glow/25 bg-cyan-glow/[0.04] p-5">
            <KeyRound className="h-5 w-5 text-cyan-glow" aria-hidden="true" />
            <h3 className="mt-4 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Restore this wallet</h3>
            <p className="mt-3 text-sm leading-6 text-platinum/60">Enter the original {RECOVERY_PHRASE_WORD_COUNT} words and choose a new password. The same wallet address and funds remain available after the restore.</p>
            <button type="button" onClick={() => openRecoveryView("restore")} className="mt-5 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-cyan-glow transition-colors hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
              Restore with recovery phrase <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </article>

          <article className="border border-amber-200/25 bg-amber-200/[0.04] p-5">
            <AlertTriangle className="h-5 w-5 text-amber-200" aria-hidden="true" />
            <h3 className="mt-4 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Lost both password and phrase?</h3>
            <p className="mt-3 text-sm leading-6 text-platinum/60">The old wallet cannot be recovered. Create a new key and recovery phrase, then explicitly confirm the old address and funds will not be restored.</p>
            <button type="button" onClick={() => { setReplacementAcknowledged(false); openRecoveryView("replace-confirm"); }} className="mt-5 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-amber-200 transition-colors hover:text-white focus:outline-none focus:ring-2 focus:ring-amber-200 focus:ring-offset-2 focus:ring-offset-ink-black">
              Create a brand-new wallet <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </article>
        </div>
      </section>
    </section>
  );
}
