"use client";

import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import { ArrowRight, ClipboardPaste, Shield, Trash2, User, WalletCards } from "lucide-react";
import Link from "next/link";
import { EXPLORER_URL } from "@/lib/links";
import {
  fetchAuthSession,
  loginWithAuthLoginPayload,
  loginWithSignedChallenge,
  requestAuthChallenge,
  signAuthChallenge,
} from "@/lib/auth";
import {
  AUTH_LOGIN_PAYLOAD_MAX_CHARS,
  parseAuthLoginPayload,
} from "@/lib/auth-payload";
import { getSafeReturnPath } from "@/lib/redirects";

type LoginMode = "wallet" | "cli";

type LoginStatus =
  | "checking"
  | "idle"
  | "requesting"
  | "signing"
  | "submitting"
  | "validating-import"
  | "success"
  | "error";

type LoginErrors = {
  nodeIdentifier?: string;
};

export default function LoginPage() {
  const [mode, setMode] = useState<LoginMode>("wallet");
  const [nodeIdentifier, setNodeIdentifier] = useState("");
  const [cliPayload, setCliPayload] = useState("");
  const [errors, setErrors] = useState<LoginErrors>({});
  const [cliError, setCliError] = useState("");
  const [status, setStatus] = useState<LoginStatus>("checking");
  const [notice, setNotice] = useState("Checking for an active wallet session...");
  const nodeInputRef = useRef<HTMLInputElement>(null);
  const cliPayloadRef = useRef<HTMLTextAreaElement>(null);
  const returnPath = getSafeReturnPath(
    typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("next"),
  );

  useEffect(() => {
    let active = true;
    void fetchAuthSession()
      .then((session) => {
        if (!active) return;
        if (session) {
          window.location.assign(returnPath ?? EXPLORER_URL);
          return;
        }
        setNotice("");
        setStatus("idle");
      })
      .catch(() => {
        if (!active) return;
        setNotice("");
        setStatus("idle");
      });
    return () => {
      active = false;
    };
  }, [returnPath]);

  const finishLogin = (message: string) => {
    setStatus("success");
    setNotice(message);
    setCliPayload("");
    window.setTimeout(() => window.location.assign(returnPath ?? EXPLORER_URL), 350);
  };

  const handleWalletSubmit = async () => {
    const nextErrors: LoginErrors = {};
    if (!nodeIdentifier.trim()) {
      nextErrors.nodeIdentifier = "Node identifier is required.";
    }
    setErrors(nextErrors);

    if (Object.keys(nextErrors).length > 0) {
      setStatus("idle");
      nodeInputRef.current?.focus();
      return;
    }

    setStatus("requesting");
    try {
      const challenge = await requestAuthChallenge(nodeIdentifier);
      setStatus("signing");
      const signedChallenge = await signAuthChallenge(challenge);
      setStatus("submitting");
      await loginWithSignedChallenge(challenge, signedChallenge);
      finishLogin("WALLET_SIGNATURE_VERIFIED // REDIRECTING_TO_COMMAND_CENTER");
    } catch (error) {
      setStatus("error");
      setNotice(error instanceof Error ? error.message : "Unable to initialize a wallet session.");
    }
  };

  const handleCliSubmit = async () => {
    setCliError("");
    setStatus("validating-import");

    let payload: ReturnType<typeof parseAuthLoginPayload>;
    try {
      payload = parseAuthLoginPayload(cliPayload);
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to validate the signed payload.";
      setCliError(message);
      setStatus("error");
      setNotice("SIGNED_PAYLOAD_REJECTED");
      return;
    }

    try {
      setStatus("submitting");
      await loginWithAuthLoginPayload(payload);
      finishLogin("SIGNED_PAYLOAD_ACCEPTED // REDIRECTING_TO_COMMAND_CENTER");
    } catch (error) {
      setStatus("error");
      setNotice(error instanceof Error ? error.message : "Unable to import the signed payload.");
    }
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setNotice("");

    if (mode === "cli") {
      await handleCliSubmit();
      return;
    }

    await handleWalletSubmit();
  };

  const switchMode = (nextMode: LoginMode) => {
    if (isBusy || status === "success" || nextMode === mode) return;
    setMode(nextMode);
    setErrors({});
    setCliError("");
    setNotice("");
    setStatus("idle");
  };

  const resetFieldError = () => {
    setErrors((current) => ({ ...current, nodeIdentifier: undefined }));
    if (status === "error") setStatus("idle");
    setNotice("");
  };

  const clearCliPayload = () => {
    setCliPayload("");
    setCliError("");
    if (status === "error") setStatus("idle");
    setNotice("CLI_PAYLOAD_CLEARED // NO_KEYS_STORED");
    cliPayloadRef.current?.focus();
  };

  const isBusy = [
    "checking",
    "requesting",
    "signing",
    "submitting",
    "validating-import",
  ].includes(status);
  const buttonLabel =
    status === "checking"
      ? "CHECKING_SESSION"
      : status === "requesting"
        ? "REQUESTING_CHALLENGE"
        : status === "signing"
          ? "SIGN_IN_ULTRAWALLET"
          : status === "validating-import"
            ? "VALIDATING_PAYLOAD"
            : status === "submitting"
              ? "VERIFYING_SIGNATURE"
              : status === "success"
                ? "SESSION_INITIALIZED"
                : mode === "cli"
                  ? "IMPORT_SIGNED_PAYLOAD"
                  : "INITIALIZE_SESSION";

  return (
    <main className="relative min-h-screen overflow-hidden bg-ink-black px-4 py-8 dot-grid sm:px-8 md:px-0 md:pt-[68px]">
      <motion.div
        initial={{ opacity: 0, scale: 0.97 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.35, ease: "easeOut" }}
        className="flex min-h-[590px] w-full max-w-[448px] flex-col p-8 neon-inset sm:p-10 md:ml-[8.4vw] md:p-12"
      >
        <div className="space-y-4 text-center">
          <Shield className="mx-auto h-12 w-12 animate-pulse text-cyan-glow" aria-hidden="true" />
          <h1 className="font-space-grotesk text-[30px] font-bold leading-none tracking-[-0.06em] text-platinum">
            Vault Access
          </h1>
          <p className="font-mono text-[11px] tracking-[0.08em] text-platinum/30">
            SECURE_AUTHENTICATION_GATEWAY_V7.1
          </p>
        </div>

        <div
          aria-label="Authentication method"
          className="mt-8 grid grid-cols-2 border border-platinum/10 bg-[#0a0a1a] p-1 font-mono text-[10px] uppercase tracking-[0.16em]"
          role="tablist"
        >
          {(["wallet", "cli"] as const).map((option) => {
            const isActive = mode === option;
            const label = option === "wallet" ? "ULTRAWALLET" : "CLI_SIGNED_PAYLOAD";
            return (
              <button
                key={option}
                type="button"
                role="tab"
                id={`${option}-auth-tab`}
                aria-selected={isActive}
                aria-controls={`${option}-auth-panel`}
                disabled={isBusy || status === "success"}
                onClick={() => switchMode(option)}
                className={`min-h-11 px-2 py-3 transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-not-allowed disabled:opacity-50 ${
                  isActive ? "bg-cyan-glow text-ink-black" : "text-platinum/45 hover:text-cyan-glow"
                }`}
              >
                {label}
              </button>
            );
          })}
        </div>

        {notice && (
          <div
            role={status === "error" ? "alert" : "status"}
            aria-live="polite"
            className={`mt-6 border px-4 py-3 text-center font-mono text-[10px] uppercase tracking-[0.16em] ${
              status === "error"
                ? "border-red-300/40 bg-red-300/10 text-red-200"
                : "border-cyan-glow/40 bg-cyan-glow/10 text-cyan-glow"
            }`}
          >
            {notice}
            {status === "signing" && (
              <div className="mt-2 text-[10px] normal-case tracking-normal text-platinum/50">
                Approve the canonical challenge in UltraWallet. Your private key never leaves the wallet.
              </div>
            )}
          </div>
        )}

        <form className="mt-8 min-w-0" onSubmit={handleSubmit} noValidate aria-busy={isBusy}>
          {mode === "wallet" ? (
            <section id="wallet-auth-panel" role="tabpanel" aria-labelledby="wallet-auth-tab" className="space-y-6">
              <div className="space-y-2">
                <div className="group relative">
                  <User
                    className="absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-platinum/20 transition-colors group-focus-within:text-cyan-glow"
                    aria-hidden="true"
                  />
                  <label htmlFor="node-identifier" className="sr-only">
                    Node identifier
                  </label>
                  <input
                    ref={nodeInputRef}
                    id="node-identifier"
                    name="nodeIdentifier"
                    type="text"
                    placeholder="NODE_IDENTIFIER"
                    autoComplete="username"
                    required
                    aria-invalid={Boolean(errors.nodeIdentifier)}
                    aria-describedby={errors.nodeIdentifier ? "node-identifier-error" : "node-identifier-help"}
                    value={nodeIdentifier}
                    onChange={(event) => {
                      setNodeIdentifier(event.target.value);
                      resetFieldError();
                    }}
                    className="h-[54px] w-full rounded-md border border-platinum/10 bg-[#151522] py-4 pl-12 pr-4 font-mono text-sm text-platinum outline-hidden transition-colors placeholder:text-platinum/35 focus:border-cyan-glow/70 focus:ring-1 focus:ring-cyan-glow/30 aria-[invalid=true]:border-red-300"
                  />
                </div>
                {errors.nodeIdentifier ? (
                  <p id="node-identifier-error" role="alert" className="pl-12 font-mono text-xs text-red-300">
                    {errors.nodeIdentifier}
                  </p>
                ) : (
                  <p id="node-identifier-help" className="pl-12 font-mono text-[10px] text-platinum/30">
                    Use the 64-character identifier derived from your approved UltraWallet public key.
                  </p>
                )}
              </div>

              <button
                type="submit"
                disabled={isBusy || status === "success"}
                className="group flex h-[60px] w-full items-center justify-center gap-2 bg-cyan-glow font-mono text-sm font-black uppercase tracking-[0.3em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black disabled:cursor-wait disabled:opacity-60"
              >
                {buttonLabel}
                {status === "signing" ? (
                  <WalletCards className="h-5 w-5 animate-pulse" aria-hidden="true" />
                ) : (
                  <ArrowRight className="h-5 w-5 transition-transform group-hover:translate-x-1" aria-hidden="true" />
                )}
              </button>
            </section>
          ) : (
            <section id="cli-auth-panel" role="tabpanel" aria-labelledby="cli-auth-tab" className="space-y-5">
              <div className="space-y-2">
                <label htmlFor="cli-auth-payload" className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/60">
                  Public signed login payload
                </label>
                <textarea
                  ref={cliPayloadRef}
                  id="cli-auth-payload"
                  name="cliAuthPayload"
                  value={cliPayload}
                  onChange={(event) => {
                    setCliPayload(event.target.value);
                    setCliError("");
                    if (status === "error") setStatus("idle");
                    setNotice("");
                  }}
                  placeholder={'{\n  "challenge_id": "...",\n  "public_key": [...],\n  "signature": [...]\n}'}
                  autoComplete="off"
                  spellCheck={false}
                  maxLength={AUTH_LOGIN_PAYLOAD_MAX_CHARS}
                  wrap="soft"
                  aria-invalid={Boolean(cliError)}
                  aria-describedby={cliError ? "cli-payload-error" : "cli-payload-help"}
                  className="min-h-[220px] w-full resize-y rounded-md border border-platinum/10 bg-[#151522] p-4 font-mono text-xs leading-6 text-platinum outline-hidden transition-colors placeholder:text-platinum/25 focus:border-cyan-glow/70 focus:ring-1 focus:ring-cyan-glow/30 aria-[invalid=true]:border-red-300"
                />
                {cliError ? (
                  <p id="cli-payload-error" role="alert" className="font-mono text-xs text-red-300">
                    {cliError}
                  </p>
                ) : (
                  <p id="cli-payload-help" className="font-mono text-[10px] leading-5 text-platinum/40">
                    Paste the public JSON emitted by ultranet-auth. Private keys and sovereign key files are rejected;
                    the payload stays in memory only.
                  </p>
                )}
              </div>

              <div className="flex flex-col gap-3 sm:flex-row">
                <button
                  type="submit"
                  disabled={isBusy || status === "success"}
                  className="group flex h-[60px] min-w-0 flex-1 items-center justify-center gap-2 bg-cyan-glow px-3 font-mono text-xs font-black uppercase tracking-[0.2em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black disabled:cursor-wait disabled:opacity-60"
                >
                  {buttonLabel}
                  <ClipboardPaste className="h-5 w-5 shrink-0 transition-transform group-hover:-translate-y-0.5" aria-hidden="true" />
                </button>
                <button
                  type="button"
                  onClick={clearCliPayload}
                  disabled={isBusy || status === "success" || cliPayload.length === 0}
                  className="flex h-[60px] items-center justify-center gap-2 border border-platinum/10 px-4 font-mono text-[10px] font-bold uppercase tracking-[0.16em] text-platinum/50 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <Trash2 className="h-4 w-4" aria-hidden="true" />
                  Clear
                </button>
              </div>
            </section>
          )}
        </form>

        <div className="mt-auto flex items-center justify-between border-t border-platinum/10 pt-8 font-mono text-[10px] uppercase text-platinum/30">
          <button
            type="button"
            onClick={() => setNotice("Key recovery is an offline operator flow. No private keys are stored here.")}
            className="transition-colors hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black"
          >
            forgot_keys?
          </button>
          <Link
            href="/#swarm"
            className="transition-colors hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black"
          >
            register_node
          </Link>
        </div>
      </motion.div>

      <div className="pointer-events-none absolute right-[11%] top-[22%] hidden font-mono text-[8px] uppercase tracking-[0.35em] text-cyan-glow/20 lg:block">
        SYSTEM_STATUS: SECURE
      </div>
      <div className="pointer-events-none fixed bottom-6 left-4 right-4 hidden justify-between font-mono text-[9px] uppercase tracking-[0.2em] text-platinum/10 sm:flex md:left-8 md:right-8">
        <span>sys_auth: {mode === "cli" ? "cli_payload" : "wallet"}</span>
        <span>quantum_resistance_enabled: true</span>
        <span>session_timeout: server</span>
      </div>
    </main>
  );
}
