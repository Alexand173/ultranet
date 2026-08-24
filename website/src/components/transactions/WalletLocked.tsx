"use client";

import { LockKeyhole } from "lucide-react";
import { FormEvent, useState } from "react";
import {
  decryptWalletSeed,
  deriveIdentityFromStoredSeed,
  storedPublicKeyToBytes,
  type LocalWalletKeyMaterial,
  type StoredWallet,
} from "@/lib/wallet-crypto";

interface WalletLockedProps {
  wallet: StoredWallet;
  onUnlocked: (material: LocalWalletKeyMaterial) => void;
}

export default function WalletLocked({ wallet, onUnlocked }: WalletLockedProps) {
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<"idle" | "unlocking" | "error">("idle");
  const [error, setError] = useState("");

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

      <p className="mt-8 max-w-xl border-t border-platinum/10 pt-5 text-xs leading-6 text-platinum/40">Your recovery phrase is required if this device or password is lost. UltraNet cannot reset this local password.</p>
    </section>
  );
}
