"use client";

import { FormEvent, useEffect, useRef, useState } from "react";
import { Terminal, Key, ChevronRight } from "lucide-react";
import { API_BASE_URL } from "@/lib/links";
import {
  ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL,
  ULTRA_WALLET_SIGNING_ENVELOPE_VERSION,
  buildValidatorProposalRequest,
  getUltraWalletProvider,
  isSignedValidatorProposal,
} from "@/lib/ultra-wallet";

type ProposalStatus = "idle" | "signing" | "success";

type FormErrors = {
  alias?: string;
  pubKey?: string;
};

export default function SwarmOnboarding() {
  const [formState, setFormState] = useState({ alias: "", pubKey: "" });
  const [formErrors, setFormErrors] = useState<FormErrors>({});
  const [proposalStatus, setProposalStatus] = useState<ProposalStatus>("idle");
  const [proposalMessage, setProposalMessage] = useState("");
  const [swarmStats, setSwarmStats] = useState({ nodes: "Unavailable", staked: "Unavailable" });
  const aliasInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const controller = new AbortController();
    const fetchSwarm = async () => {
      try {
        const response = await fetch(`${API_BASE_URL}/api/stats`, { signal: controller.signal, cache: "no-store" });
        if (!response.ok) throw new Error(`Stats request failed with ${response.status}`);
        const data = await response.json();
        if (data.success && data.data) {
          const validatorCount = Number(data.data.validator_count);
          const totalWeight = Number(data.data.total_weight);
          setSwarmStats({
            nodes: Number.isFinite(validatorCount) ? new Intl.NumberFormat("en-US").format(validatorCount) : "Unavailable",
            staked: Number.isFinite(totalWeight) ? new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 2 }).format(totalWeight) : "Unavailable",
          });
        }
      } catch {
        if (!controller.signal.aborted) setSwarmStats({ nodes: "Unavailable", staked: "Unavailable" });
      }
    };
    void fetchSwarm();
    const interval = window.setInterval(() => void fetchSwarm(), 10000);
    return () => {
      controller.abort();
      window.clearInterval(interval);
    };
  }, []);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setProposalMessage("");

    const nextErrors: FormErrors = {};
    if (!formState.alias.trim()) nextErrors.alias = "Node alias is required.";
    if (!formState.pubKey.trim()) nextErrors.pubKey = "Dilithium public key is required.";
    setFormErrors(nextErrors);

    if (Object.keys(nextErrors).length > 0) {
      setProposalStatus("idle");
      if (nextErrors.alias) aliasInputRef.current?.focus();
      return;
    }

    const wallet = getUltraWalletProvider();
    if (!wallet) {
      setProposalStatus("idle");
      setProposalMessage("Connect UltraWallet to sign this proposal locally. No unsigned payload was sent.");
      return;
    }

    setProposalStatus("signing");

    try {
      const signedProposal = await wallet.request({
        method: ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL,
        params: {
          metadata: formState.alias.trim(),
          proposalPublicKey: formState.pubKey.trim(),
          version: ULTRA_WALLET_SIGNING_ENVELOPE_VERSION,
        },
      });

      if (!isSignedValidatorProposal(signedProposal)) {
        throw new Error("Wallet returned an incomplete signed proposal.");
      }

      const response = await fetch(`${API_BASE_URL}/api/governance/propose`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(
          buildValidatorProposalRequest(signedProposal, formState.alias),
        ),
      });
      const data = await response.json().catch(() => null);

      if (!response.ok || !data?.success) {
        throw new Error(data?.message || "Validator proposal was rejected by the node.");
      }

      setProposalStatus("success");
      setProposalMessage(data.message || "Validator proposal submitted to the governance queue.");
      setFormState({ alias: "", pubKey: "" });
      setFormErrors({});
    } catch (error) {
      setProposalStatus("idle");
      setProposalMessage(error instanceof Error ? error.message : "Unable to submit validator proposal.");
    }
  };

  const clearFieldError = (field: keyof FormErrors) => {
    setFormErrors((current) => ({ ...current, [field]: undefined }));
    setProposalMessage("");
    setProposalStatus("idle");
  };

  return (
    <section id="swarm" className="min-h-[80vh] grid grid-cols-1 md:grid-cols-2 relative z-10 border-y border-platinum/5">
      {/* Left Panel: Visual/Context */}
      <div className="relative overflow-hidden flex flex-col justify-end p-12 bg-zinc-900 border-r border-platinum/10">
        <div className="absolute inset-0 opacity-40 mix-blend-overlay">
          <img
            src="https://images.unsplash.com/photo-1680992046617-e2e35451bcdb?crop=entropy&cs=srgb&fm=jpg&q=85"
            alt="Server Room"
            className="w-full h-full object-cover"
          />
        </div>
        <div className="absolute inset-0 bg-gradient-to-t from-ink-black via-ink-black/20 to-transparent" />

        <div className="relative z-10 space-y-8">
          <div className="space-y-4">
            <h2 className="text-4xl md:text-5xl font-bold leading-tight">Join the<br /><span className="text-cyan-glow">Validator Swarm</span></h2>
            <p className="max-w-md text-platinum/60">
              UltraNet is bootstrapped by a global swarm of infrastructure providers.
              Validators help anchor state transitions; active rewards follow the node's protocol and governance configuration.
            </p>
          </div>

          <div className="grid grid-cols-2 gap-8 py-8 border-y border-platinum/10 font-mono">
            <div>
              <p className="text-xs text-platinum/40 uppercase">Active Nodes</p>
              <p className="text-2xl text-cyan-glow">{swarmStats.nodes}</p>
            </div>
            <div>
              <p className="text-xs text-platinum/40 uppercase">Validator Weight</p>
              <p className="text-2xl text-cyan-glow">{swarmStats.staked}</p>
            </div>
          </div>
        </div>
      </div>

      {/* Right Panel: Form */}
      <div className="p-8 md:p-24 flex flex-col justify-center space-y-12">
        <div className="space-y-4">
          <p className="text-cyan-glow font-mono text-xs tracking-widest uppercase">Identity Verification</p>
          <h3 className="text-3xl font-bold uppercase">Node Registration</h3>
        </div>

        {proposalMessage && (
          <div
            role={proposalStatus === "success" ? "status" : "alert"}
            aria-live="polite"
            className={`border px-4 py-3 font-mono text-xs leading-relaxed ${
              proposalStatus === "success"
                ? "border-cyan-glow/40 bg-cyan-glow/10 text-cyan-glow"
                : "border-red-300/40 bg-red-300/10 text-red-200"
            }`}
          >
            {proposalMessage}
          </div>
        )}

        <form className="space-y-8 max-w-md" onSubmit={handleSubmit} noValidate aria-busy={proposalStatus === "signing"}>
          <div className="space-y-2 group">
            <label htmlFor="node-alias" className="text-xs font-mono text-platinum/40 flex items-center gap-2 group-focus-within:text-cyan-glow transition-colors">
              <Terminal className="w-3 h-3" aria-hidden="true" /> NODE_ALIAS.exe
            </label>
            <input
              ref={aliasInputRef}
              id="node-alias"
              name="nodeAlias"
              type="text"
              placeholder="e.g. Genesis-Alpha-01"
              autoComplete="organization"
              required
              aria-invalid={Boolean(formErrors.alias)}
              aria-describedby={formErrors.alias ? "node-alias-error" : undefined}
              className="w-full bg-transparent border-b-2 border-platinum/10 py-4 focus:border-cyan-glow outline-none transition-colors font-mono text-platinum placeholder:text-platinum/20 aria-[invalid=true]:border-red-300"
              value={formState.alias}
              onChange={(event) => {
                setFormState({ ...formState, alias: event.target.value });
                clearFieldError("alias");
              }}
            />
            {formErrors.alias && <p id="node-alias-error" role="alert" className="text-xs font-mono text-red-300">{formErrors.alias}</p>}
          </div>

          <div className="space-y-2 group">
            <label htmlFor="dilithium-public-key" className="text-xs font-mono text-platinum/40 flex items-center gap-2 group-focus-within:text-cyan-glow transition-colors">
              <Key className="w-3 h-3" aria-hidden="true" /> DILITHIUM_PUB_KEY.hex
            </label>
            <textarea
              id="dilithium-public-key"
              name="dilithiumPublicKey"
              rows={3}
              placeholder="0x6c6dd0c8..."
              required
              aria-invalid={Boolean(formErrors.pubKey)}
              aria-describedby={formErrors.pubKey ? "dilithium-public-key-error" : undefined}
              className="w-full bg-transparent border-b-2 border-platinum/10 py-4 focus:border-cyan-glow outline-none transition-colors font-mono text-platinum placeholder:text-platinum/20 resize-none aria-[invalid=true]:border-red-300"
              value={formState.pubKey}
              onChange={(event) => {
                setFormState({ ...formState, pubKey: event.target.value });
                clearFieldError("pubKey");
              }}
            />
            {formErrors.pubKey && <p id="dilithium-public-key-error" role="alert" className="text-xs font-mono text-red-300">{formErrors.pubKey}</p>}
          </div>

          <button
            type="submit"
            disabled={proposalStatus === "signing"}
            className="w-full py-6 bg-platinum text-ink-black font-bold uppercase tracking-[0.2em] flex items-center justify-center gap-2 hover:bg-cyan-glow disabled:cursor-wait disabled:opacity-60 transition-colors group"
          >
            {proposalStatus === "signing" ? "SIGNING_PROPOSAL" : "SUBMIT_JOIN_PROPOSAL"}
            <ChevronRight className="w-5 h-5 group-hover:translate-x-1 transition-transform" aria-hidden="true" />
          </button>

          <p className="text-[10px] text-platinum/30 leading-relaxed font-mono italic">
            * Wallet signatures are created locally. All proposals require 2-of-3 Sovereign Multi-Sig approval.
            Once accepted, your node identity will be broadcast to the gossip layer.
          </p>
        </form>
      </div>
    </section>
  );
}
