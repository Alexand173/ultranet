"use client";

import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { CreateAppChainInput } from "@/lib/appchains.types";

export type RegistrationState = "idle" | "submitting" | "error";

interface RegisterAppChainModalProps {
  open: boolean;
  state: RegistrationState;
  error: string;
  onClose: () => void;
  onSubmit: (input: CreateAppChainInput) => Promise<void>;
}

type FormErrors = Partial<Record<keyof CreateAppChainInput, string>>;

function validate(values: CreateAppChainInput): FormErrors {
  const errors: FormErrors = {};
  const name = values.name.trim();
  const owner = values.owner.trim();
  if (!name) errors.name = "AppChain name is required.";
  else if (name.length > 80) errors.name = "Use 80 characters or fewer.";
  else if (!/^[a-z0-9 _-]+$/i.test(name)) errors.name = "Use letters, numbers, spaces, hyphens, or underscores.";
  if (!owner) errors.owner = "Owner address or alias is required.";
  else if (owner.length > 120) errors.owner = "Use 120 characters or fewer.";
  return errors;
}

export default function RegisterAppChainModal({ open, state, error, onClose, onSubmit }: RegisterAppChainModalProps) {
  const [values, setValues] = useState<CreateAppChainInput>({ name: "", owner: "" });
  const [errors, setErrors] = useState<FormErrors>({});
  const dialogRef = useRef<HTMLDivElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    restoreFocusRef.current = document.activeElement as HTMLElement | null;
    setValues({ name: "", owner: "" });
    setErrors({});
    const frame = window.requestAnimationFrame(() => nameRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [open]);

  useEffect(() => {
    if (!open) {
      restoreFocusRef.current?.focus();
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && state !== "submitting") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = Array.from(dialogRef.current.querySelectorAll<HTMLElement>("button, input, [href], [tabindex]:not([tabindex=\"-1\"])"))
        .filter((element) => !element.hasAttribute("disabled"));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.body.style.overflow = previousOverflow;
    };
  }, [onClose, open, state]);

  const updateField = (field: keyof CreateAppChainInput, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
    if (errors[field]) setErrors((current) => ({ ...current, [field]: undefined }));
  };

  const handleBlur = (field: keyof CreateAppChainInput) => {
    const nextErrors = validate(values);
    setErrors((current) => ({ ...current, [field]: nextErrors[field] }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const nextErrors = validate(values);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) {
      if (nextErrors.name) nameRef.current?.focus();
      return;
    }
    await onSubmit({ name: values.name.trim(), owner: values.owner.trim() });
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div className="fixed inset-0 z-[80] flex items-center justify-center overflow-y-auto bg-black/75 px-4 py-8 backdrop-blur-sm sm:px-6" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onMouseDown={(event) => { if (event.target === event.currentTarget && state !== "submitting") onClose(); }}>
          <motion.div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="register-appchain-title" aria-describedby="register-appchain-description" className="w-full max-w-md rounded-md border border-cyan-glow/35 bg-ink-black/95 p-6 shadow-[0_0_45px_rgba(15,255,255,0.12)] sm:p-7" initial={{ opacity: 0, scale: 0.96, y: 14 }} animate={{ opacity: 1, scale: 1, y: 0 }} exit={{ opacity: 0, scale: 0.98, y: 8 }} transition={{ duration: 0.18 }}>
            <div className="flex items-start justify-between gap-5">
              <div>
                <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow/70">L3 // REGISTRY_WRITE</p>
                <h2 id="register-appchain-title" className="mt-3 font-space-grotesk text-2xl font-bold tracking-tight text-platinum">Register New AppChain (L3)</h2>
              </div>
              <button type="button" onClick={onClose} disabled={state === "submitting"} aria-label="Close registration dialog" title="Close" className="inline-flex h-9 w-9 shrink-0 items-center justify-center border border-platinum/10 text-platinum/45 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-40"><X className="h-4 w-4" aria-hidden="true" /></button>
            </div>
            <p id="register-appchain-description" className="mt-4 text-sm leading-6 text-platinum/50">Create a registry record with a display owner. The node derives a dedicated L1 treasury address; fund it with a normal transfer before anchoring.</p>

            {error && <div className="mt-5 flex items-start gap-2 border border-red-300/30 bg-red-300/10 p-3 text-xs leading-5 text-red-100/80" role="alert" aria-live="assertive"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-red-200" aria-hidden="true" /><span>{error}</span></div>}

            <form className="mt-6 space-y-5" onSubmit={handleSubmit} noValidate>
              <div>
                <label htmlFor="appchain-name" className="font-mono text-[10px] font-bold uppercase tracking-[0.16em] text-platinum/45">AppChain name <span className="text-cyan-glow" aria-hidden="true">*</span></label>
                <input ref={nameRef} id="appchain-name" name="name" value={values.name} onChange={(event) => updateField("name", event.target.value)} onBlur={() => handleBlur("name")} maxLength={80} placeholder="e.g. UltraDex, GameLayer" required aria-required="true" aria-invalid={Boolean(errors.name)} aria-describedby={errors.name ? "appchain-name-error" : undefined} className="mt-2 block min-h-11 w-full rounded border border-platinum/15 bg-black/45 px-4 text-sm text-platinum outline-hidden transition-colors placeholder:text-platinum/25 focus:border-cyan-glow/70 focus:ring-2 focus:ring-cyan-glow/20" />
                {errors.name && <p id="appchain-name-error" className="mt-2 font-mono text-[10px] leading-5 text-red-200" role="alert">{errors.name}</p>}
              </div>
              <div>
                <label htmlFor="appchain-owner" className="font-mono text-[10px] font-bold uppercase tracking-[0.16em] text-platinum/45">Owner address / alias <span className="text-cyan-glow" aria-hidden="true">*</span></label>
                <input id="appchain-owner" name="owner" value={values.owner} onChange={(event) => updateField("owner", event.target.value)} onBlur={() => handleBlur("owner")} maxLength={120} placeholder="0x... or Ultra Labs" required aria-required="true" aria-invalid={Boolean(errors.owner)} aria-describedby={errors.owner ? "appchain-owner-error" : "appchain-owner-help"} className="mt-2 block min-h-11 w-full rounded border border-platinum/15 bg-black/45 px-4 text-sm text-platinum outline-hidden transition-colors placeholder:text-platinum/25 focus:border-cyan-glow/70 focus:ring-2 focus:ring-cyan-glow/20" />
                {errors.owner ? <p id="appchain-owner-error" className="mt-2 font-mono text-[10px] leading-5 text-red-200" role="alert">{errors.owner}</p> : <p id="appchain-owner-help" className="mt-2 text-xs leading-5 text-platinum/35">Canonical addresses can be linked to account data. Aliases remain display-only.</p>}
              </div>
              <div className="flex flex-col-reverse gap-3 border-t border-platinum/10 pt-5 sm:grid sm:grid-cols-2">
                <button type="button" onClick={onClose} disabled={state === "submitting"} className="inline-flex min-h-11 items-center justify-center rounded bg-platinum/[0.06] px-4 font-mono text-xs font-bold uppercase tracking-[0.14em] text-platinum/60 transition-colors hover:bg-platinum/10 hover:text-platinum focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-50">Cancel</button>
                <button type="submit" disabled={state === "submitting"} aria-busy={state === "submitting"} className="inline-flex min-h-11 items-center justify-center gap-2 rounded bg-cyan-glow px-4 font-mono text-xs font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-60">{state === "submitting" ? "Creating chain…" : "Create Chain"}</button>
              </div>
            </form>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
