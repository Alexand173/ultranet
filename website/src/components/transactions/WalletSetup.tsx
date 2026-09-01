"use client";

import { AlertTriangle, ArrowLeft, Check, Eye, EyeOff, KeyRound, RotateCcw } from "lucide-react";
import { FormEvent, useMemo, useState } from "react";
import {
  bytesToHex,
  clearLocalWalletKeyMaterial,
  createLocalWalletFromPhrase,
  createRecoveryPhrase,
  encryptWalletSeed,
  isRecoveryPhraseValid,
  keyMaterialToStoredWallet,
  normalizeRecoveryPhrase,
  splitRecoveryPhrase,
  type LocalWalletKeyMaterial,
  type StoredWallet,
} from "@/lib/wallet-crypto";
import { saveStoredWallet } from "@/lib/wallet-storage";

interface WalletSetupProps {
  onCreated: (wallet: StoredWallet, password: string) => Promise<void> | void;
  initialMode?: SetupMode;
  allowModeToggle?: boolean;
  onCancel?: () => void;
  replacement?: boolean;
  restoreTarget?: Pick<StoredWallet, "address" | "publicKey">;
}

type SetupMode = "create" | "restore";
type SetupStep = "password" | "phrase" | "verify" | "created";

function makeCheckPositions(): [number, number] {
  const values = crypto.getRandomValues(new Uint32Array(2));
  const first = (values[0] % 12) + 1;
  const second = (values[1] % 12) + 1;
  return [first, second === first ? (second % 12) + 1 : second];
}

export default function WalletSetup({
  onCreated,
  initialMode = "create",
  allowModeToggle = true,
  onCancel,
  replacement = false,
  restoreTarget,
}: WalletSetupProps) {
  const [mode, setMode] = useState<SetupMode>(initialMode);
  const [step, setStep] = useState<SetupStep>("password");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [restorePhrase, setRestorePhrase] = useState("");
  const [phrase, setPhrase] = useState("");
  const [visible, setVisible] = useState(false);
  const [saved, setSaved] = useState(false);
  const [checkPositions, setCheckPositions] = useState<[number, number]>(() => makeCheckPositions());
  const [checkWords, setCheckWords] = useState<[string, string]>(["", ""]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const words = useMemo(() => splitRecoveryPhrase(phrase), [phrase]);
  const title = mode === "restore"
    ? replacement ? "Restore this wallet" : "Restore a wallet"
    : replacement ? "Create a new wallet" : "Create a wallet";

  const reset = () => {
    setStep("password");
    setPassword("");
    setConfirmPassword("");
    setRestorePhrase("");
    setPhrase("");
    setVisible(false);
    setSaved(false);
    setCheckWords(["", ""]);
    setError("");
    setBusy(false);
  };

  const toggleMode = () => {
    reset();
    setMode((current) => current === "create" ? "restore" : "create");
  };

  const handleCancel = () => {
    reset();
    onCancel?.();
  };

  const validatePassword = (): boolean => {
    if (password.length < 12) {
      setError("Use at least 12 characters for the wallet password.");
      return false;
    }
    if (password !== confirmPassword) {
      setError("The two wallet passwords do not match.");
      return false;
    }
    return true;
  };

  const handlePasswordSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    if (!validatePassword()) return;

    if (mode === "restore") {
      if (!isRecoveryPhraseValid(restorePhrase)) {
        setError("Enter the valid 12-word recovery phrase for this wallet.");
        return;
      }
      void persistWallet(normalizeRecoveryPhrase(restorePhrase));
      return;
    }

    const nextPhrase = createRecoveryPhrase();
    setPhrase(nextPhrase);
    setCheckPositions(makeCheckPositions());
    setStep("phrase");
  };

  const persistWallet = async (recoveryPhrase: string) => {
    setBusy(true);
    setError("");
    let material: LocalWalletKeyMaterial | null = null;
    try {
      material = await createLocalWalletFromPhrase(recoveryPhrase);
      if (restoreTarget) {
        const expectedPublicKey = restoreTarget.publicKey.trim().replace(/^0x/i, "").toLowerCase();
        if (material.address !== restoreTarget.address || bytesToHex(material.publicKey) !== expectedPublicKey) {
          throw new Error("That recovery phrase belongs to a different wallet. To replace the locked wallet, cancel and explicitly choose Create a brand-new wallet.");
        }
      }
      const encryptedSeed = await encryptWalletSeed(material.seed, password);
      const storedWallet = keyMaterialToStoredWallet(material, encryptedSeed);
      await saveStoredWallet(storedWallet);
      setStep("created");
      await onCreated(storedWallet, password);
      setPassword("");
      setConfirmPassword("");
      setRestorePhrase("");
      setPhrase("");
    } catch (persistError) {
      setPassword("");
      setConfirmPassword("");
      setRestorePhrase("");
      setPhrase("");
      setSaved(false);
      setStep("password");
      setError(persistError instanceof Error ? persistError.message : "This browser could not securely save the wallet.");
    } finally {
      if (material) clearLocalWalletKeyMaterial(material);
      setBusy(false);
    }
  };

  const handleBackupSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    if (!saved) {
      setError("Confirm that you saved all 12 words before checking your backup.");
      return;
    }
    setStep("verify");
  };

  const handleVerifySubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    const expected = [words[checkPositions[0] - 1], words[checkPositions[1] - 1]];
    if (checkWords.some((word, index) => word.trim().toLowerCase() !== expected[index])) {
      setError("Those words do not match. Check your backup and try again.");
      return;
    }
    void persistWallet(phrase);
  };

  return (
    <section className="relative z-10 mx-auto max-w-5xl px-6 py-12 md:px-10 lg:px-12 lg:py-20" aria-labelledby="wallet-setup-title">
      <div className="mb-8 flex flex-col gap-5 border-b border-platinum/15 pb-8 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="font-mono text-xs uppercase tracking-[0.24em] text-cyan-glow">WALLET // PHASE_0</p>
          <h1 id="wallet-setup-title" className="mt-4 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-5xl">{title}</h1>
          <p className="mt-4 max-w-2xl text-sm leading-7 text-platinum/60">Your keys stay on this device. UltraNet never receives your private key, password, or recovery phrase.</p>
        </div>
        <div className="flex flex-wrap items-center gap-4 self-start sm:self-auto">
          {onCancel && (
            <button type="button" onClick={handleCancel} disabled={busy} className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/30 underline-offset-4 transition-colors hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black disabled:cursor-wait disabled:opacity-50">
              <ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" /> Back to locked wallet
            </button>
          )}
          {allowModeToggle && (
            <button type="button" onClick={toggleMode} className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/30 underline-offset-4 transition-colors hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
              <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" /> {mode === "create" ? "Restore wallet" : "Create new wallet"}
            </button>
          )}
        </div>
      </div>

      <ol className="mb-8 grid gap-px border border-platinum/10 bg-platinum/10 sm:grid-cols-3">
        {["Protect this wallet", "Save your recovery phrase", "Check your backup"].map((label, index) => {
          const active = (mode === "restore" && index === 0) || (mode === "create" && ((step === "password" && index === 0) || (step === "phrase" && index === 1) || (step === "verify" && index === 2) || step === "created"));
          const completed = (mode === "create" && ((index === 0 && step !== "password") || (index === 1 && (step === "verify" || step === "created")) || (index === 2 && step === "created")));
          return <li key={label} className={`bg-ink-black p-4 font-mono text-[10px] uppercase tracking-[0.14em] ${active ? "text-cyan-glow" : "text-platinum/35"}`}><span className="mr-3 text-platinum/25">0{index + 1}</span>{completed ? <Check className="mr-2 inline h-3.5 w-3.5" aria-hidden="true" /> : null}{label}</li>;
        })}
      </ol>

      {error && <div id="wallet-setup-error" role="alert" aria-live="polite" aria-atomic="true" className="mb-6 border border-red-300/40 bg-red-300/10 px-4 py-3 font-mono text-xs leading-6 text-red-200">{error}</div>}

      {mode === "restore" && (
        <form className="max-w-2xl space-y-6" onSubmit={handlePasswordSubmit} noValidate aria-busy={busy}>
          {replacement && (
            <div className="border border-cyan-glow/25 bg-cyan-glow/[0.04] p-5 text-sm leading-7 text-platinum/65">
              <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-cyan-glow">Recover the existing wallet</p>
              <p className="mt-3">Use the original 12 words to keep this wallet&apos;s existing address and funds. The phrase must derive the same wallet identity before the encrypted record can be replaced.</p>
            </div>
          )}
          <div className="space-y-2">
            <label htmlFor="restore-recovery-phrase" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Recovery phrase</label>
            <textarea id="restore-recovery-phrase" value={restorePhrase} onChange={(event) => setRestorePhrase(event.target.value)} rows={4} autoComplete="off" spellCheck={false} autoCapitalize="none" aria-describedby="restore-recovery-phrase-help" placeholder="Enter the original 12 words in order, separated by spaces" className="w-full resize-y border border-platinum/15 bg-platinum/[0.03] p-4 font-mono text-sm leading-7 text-platinum outline-hidden focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40" />
            <p id="restore-recovery-phrase-help" className="font-mono text-[10px] leading-5 text-platinum/40">Never enter this phrase into a website that does not clearly keep it local.</p>
          </div>
          <PasswordFields password={password} confirmPassword={confirmPassword} setPassword={setPassword} setConfirmPassword={setConfirmPassword} />
          <button type="submit" disabled={busy} className="inline-flex min-h-11 items-center justify-center bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black disabled:cursor-wait disabled:opacity-50">{busy ? "Restoring wallet…" : "Restore wallet"}</button>
        </form>
      )}

      {mode === "create" && step === "password" && (
        <form className="max-w-2xl space-y-6" onSubmit={handlePasswordSubmit} noValidate>
          {replacement && (
            <div className="border border-amber-300/30 bg-amber-300/10 p-5 text-sm leading-7 text-amber-100/85">
              <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-amber-200">New wallet replacement</p>
              <p className="mt-3">This creates a separate wallet with a new address and recovery phrase. The old wallet remains untouched until the new encrypted record is successfully saved.</p>
            </div>
          )}
          <div className="flex items-start gap-4 border border-cyan-glow/20 bg-cyan-glow/[0.03] p-5"><KeyRound className="mt-0.5 h-5 w-5 shrink-0 text-cyan-glow" aria-hidden="true" /><p className="text-sm leading-7 text-platinum/60">Use this password to unlock the wallet on this device. It cannot replace your recovery phrase.</p></div>
          <PasswordFields password={password} confirmPassword={confirmPassword} setPassword={setPassword} setConfirmPassword={setConfirmPassword} />
          <button type="submit" className="inline-flex min-h-11 items-center justify-center bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">Create wallet</button>
        </form>
      )}

      {mode === "create" && step === "phrase" && (
        <div className="cut-corner neon-inset max-w-3xl p-6 sm:p-8">
          <div className="flex flex-col gap-4 border-b border-cyan-glow/20 pb-5 sm:flex-row sm:items-start sm:justify-between"><div><p className="font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Recovery phrase</p><p className="mt-2 font-mono text-[10px] uppercase tracking-[0.14em] text-amber-200">Anyone with these words can control your wallet.</p></div><button type="button" onClick={() => setVisible((current) => !current)} className="inline-flex min-h-11 items-center gap-2 self-start font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow underline decoration-cyan-glow/40 underline-offset-4 focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-[#0A0A1A]"><span aria-hidden="true">{visible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}</span>{visible ? "Hide recovery phrase" : "Reveal recovery phrase"}</button></div>
          {!visible && <p className="mt-5 border border-amber-300/25 bg-amber-300/10 p-4 font-mono text-xs leading-6 text-amber-100">Make sure nobody can see your screen before you reveal these words.</p>}
          <div className="mt-6 grid grid-cols-2 gap-x-5 border-y border-cyan-glow/15 font-mono text-xs sm:gap-x-10">
            {Array.from({ length: 6 }, (_, index) => { const left = index; const right = index + 6; return <div key={index} className="contents"><PhraseWord index={left + 1} word={words[left]} visible={visible} /><PhraseWord index={right + 1} word={words[right]} visible={visible} /></div>; })}
          </div>
          {visible && <form className="mt-6 space-y-5" onSubmit={handleBackupSubmit}><label className="flex min-h-11 items-start gap-3 text-sm leading-6 text-platinum/70"><input type="checkbox" checked={saved} onChange={(event) => setSaved(event.target.checked)} className="mt-1 h-4 w-4 accent-cyan-glow" /> <span>I saved all 12 words in order</span></label><button type="submit" className="inline-flex min-h-11 items-center justify-center bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-[#0A0A1A]">Check my backup</button></form>}
        </div>
      )}

      {mode === "create" && step === "verify" && (
        <form className="max-w-2xl space-y-6" onSubmit={handleVerifySubmit}>
          <div><p className="font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow">Backup verification</p><h2 className="mt-3 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum">Check your backup</h2><p className="mt-3 text-sm leading-7 text-platinum/60">Enter the two requested words exactly as you wrote them. The phrase stays hidden while you check.</p></div>
          <div className="grid gap-5 sm:grid-cols-2">{checkPositions.map((position, index) => <div key={position} className="space-y-2"><label htmlFor={`recovery-word-${position}`} className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Word {position}</label><input id={`recovery-word-${position}`} type="text" autoComplete="off" spellCheck={false} value={checkWords[index]} onChange={(event) => { const next = [...checkWords] as [string, string]; next[index] = event.target.value; setCheckWords(next); }} className="h-14 w-full border border-platinum/15 bg-platinum/[0.03] px-4 font-mono text-sm text-platinum outline-hidden focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40" /></div>)}</div>
          <button type="submit" disabled={busy} className="inline-flex min-h-11 items-center justify-center bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-[#0A0A1A] disabled:cursor-wait disabled:opacity-50">{busy ? "Creating wallet…" : "Check your backup"}</button>
        </form>
      )}

      {step === "created" && <div className="border border-emerald-300/30 bg-emerald-300/[0.06] p-6 font-mono text-sm leading-7 text-emerald-200"><p className="flex items-center gap-3 font-space-grotesk text-xl font-bold uppercase text-emerald-200"><Check className="h-5 w-5" aria-hidden="true" /> {mode === "restore" ? "Wallet restored" : replacement ? "New wallet created" : "Wallet created"}</p><p className="mt-3">{mode === "restore" ? "The original wallet address is protected by your new local password." : replacement ? "The old wallet was replaced only after the new encrypted wallet was successfully saved." : "Your wallet is protected on this device."}</p></div>}

      {mode === "create" && step === "phrase" && <p className="mt-6 flex max-w-3xl items-start gap-3 font-mono text-[10px] leading-6 text-platinum/40"><AlertTriangle className="mt-1 h-3.5 w-3.5 shrink-0 text-amber-200" aria-hidden="true" /> UltraNet support will never ask for your recovery phrase. Write it down offline; do not screenshot or send it.</p>}
    </section>
  );
}

function PasswordFields({ password, confirmPassword, setPassword, setConfirmPassword }: { password: string; confirmPassword: string; setPassword: (value: string) => void; setConfirmPassword: (value: string) => void }) {
  return <div className="grid gap-5 sm:grid-cols-2"><div className="space-y-2"><label htmlFor="wallet-password" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Wallet password</label><input id="wallet-password" name="password" type="password" autoComplete="new-password" required value={password} onChange={(event) => setPassword(event.target.value)} className="h-14 w-full border border-platinum/15 bg-platinum/[0.03] px-4 font-mono text-sm text-platinum outline-hidden focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40" /><p className="font-mono text-[10px] leading-5 text-platinum/40">At least 12 characters.</p></div><div className="space-y-2"><label htmlFor="wallet-password-confirm" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">Confirm password</label><input id="wallet-password-confirm" name="confirmPassword" type="password" autoComplete="new-password" required value={confirmPassword} onChange={(event) => setConfirmPassword(event.target.value)} className="h-14 w-full border border-platinum/15 bg-platinum/[0.03] px-4 font-mono text-sm text-platinum outline-hidden focus:border-cyan-glow focus:ring-1 focus:ring-cyan-glow/40" /></div></div>;
}

function PhraseWord({ index, word, visible }: { index: number; word: string; visible: boolean }) {
  return <div className="flex min-h-14 items-center gap-3 border-b border-cyan-glow/10 px-2 last:border-b-0 sm:px-3"><span className="w-5 text-[10px] text-platinum/35">{String(index).padStart(2, "0")}</span>{visible ? <span data-word-index={index} className="font-ibm-plex-sans text-sm font-bold text-platinum">{word}</span> : <span aria-hidden="true" className="text-platinum/45">••••••••</span>}</div>;
}
