"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { createLocalTransactionProvider } from "@/lib/transaction-signing";
import { createLocalValidatorProvider } from "@/lib/validator-signing";
import {
  clearLocalWalletKeyMaterial,
  type LocalWalletKeyMaterial,
  type StoredWallet,
} from "@/lib/wallet-crypto";
import { getUltraWalletProvider, type UltraWalletProvider } from "@/lib/ultra-wallet";
import { loadStoredWallet } from "@/lib/wallet-storage";
import type {
  ExternalWalletStatus,
  WalletSessionContextValue,
  WalletSessionState,
} from "@/components/wallet/wallet.types";

// The local session lasts as long as the tab is open. The provider is mounted in the root layout,
// so moving between pages keeps the unlocked key material in memory; only an explicit lock — or
// losing the tab, which discards that material — ends it. There is deliberately no inactivity
// timer and no lock-on-tab-hidden: both ended the session while the user was reading or briefly
// switching windows, which is not a boundary this client can enforce anyway.

const WalletSessionContext = createContext<WalletSessionContextValue | null>(null);

export default function WalletSessionProvider({ children }: { children: React.ReactNode }) {
  const [sessionState, setSessionState] = useState<WalletSessionState>("checking");
  const [externalWalletStatus, setExternalWalletStatus] = useState<ExternalWalletStatus>("checking");
  const [storedWallet, setStoredWallet] = useState<StoredWallet | null>(null);
  const [storageError, setStorageError] = useState<string | null>(null);
  const materialRef = useRef<LocalWalletKeyMaterial | null>(null);
  const storedWalletRef = useRef<StoredWallet | null>(null);
  const activeSigningRef = useRef(0);
  const pendingLockRef = useRef(false);
  const deferredMaterialWipeRef = useRef<Set<LocalWalletKeyMaterial>>(new Set());

  useEffect(() => {
    let active = true;
    void loadStoredWallet()
      .then((wallet) => {
        if (!active) return;
        storedWalletRef.current = wallet;
        setStoredWallet(wallet);
        setSessionState(wallet ? "locked" : "none");
      })
      .catch((error) => {
        if (!active) return;
        setStorageError(error instanceof Error ? error.message : "Unable to read the local wallet.");
        setSessionState("none");
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const updateExternalWalletStatus = () => {
      setExternalWalletStatus(getUltraWalletProvider() ? "detected" : "not-detected");
    };

    updateExternalWalletStatus();
    window.addEventListener("focus", updateExternalWalletStatus);
    window.addEventListener("pageshow", updateExternalWalletStatus);
    document.addEventListener("visibilitychange", updateExternalWalletStatus);
    const interval = window.setInterval(updateExternalWalletStatus, 2000);

    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", updateExternalWalletStatus);
      window.removeEventListener("pageshow", updateExternalWalletStatus);
      document.removeEventListener("visibilitychange", updateExternalWalletStatus);
    };
  }, []);

  const performLock = useCallback(() => {
    const material = materialRef.current;
    materialRef.current = null;
    pendingLockRef.current = false;
    if (material) clearLocalWalletKeyMaterial(material);
    for (const deferredMaterial of deferredMaterialWipeRef.current) clearLocalWalletKeyMaterial(deferredMaterial);
    deferredMaterialWipeRef.current.clear();
    setSessionState(storedWalletRef.current ? "locked" : "none");
  }, []);

  const lock = useCallback(() => {
    if (activeSigningRef.current > 0) {
      pendingLockRef.current = true;
      return;
    }
    performLock();
  }, [performLock]);

  const beginSigning = useCallback(() => {
    activeSigningRef.current += 1;
  }, []);

  const endSigning = useCallback(() => {
    activeSigningRef.current = Math.max(0, activeSigningRef.current - 1);
    if (activeSigningRef.current !== 0) return;
    if (pendingLockRef.current) {
      performLock();
      return;
    }
    for (const deferredMaterial of deferredMaterialWipeRef.current) clearLocalWalletKeyMaterial(deferredMaterial);
    deferredMaterialWipeRef.current.clear();
  }, [performLock]);

  const unlock = useCallback((material: LocalWalletKeyMaterial, wallet?: StoredWallet) => {
    const previousMaterial = materialRef.current;
    if (previousMaterial && previousMaterial !== material) {
      if (activeSigningRef.current > 0) deferredMaterialWipeRef.current.add(previousMaterial);
      else clearLocalWalletKeyMaterial(previousMaterial);
    }
    materialRef.current = material;
    pendingLockRef.current = false;
    if (wallet) {
      storedWalletRef.current = wallet;
      setStoredWallet(wallet);
    }
    setStorageError(null);
    setSessionState("unlocked");
  }, []);

  // Wipe the in-memory key when the app unmounts (tab closed or full navigation away). No timer
  // and no visibility handler run here: those would end the session while the tab is still open.
  useEffect(() => () => {
    const material = materialRef.current;
    materialRef.current = null;
    if (material) clearLocalWalletKeyMaterial(material);
    for (const deferredMaterial of deferredMaterialWipeRef.current) clearLocalWalletKeyMaterial(deferredMaterial);
    deferredMaterialWipeRef.current.clear();
  }, []);

  const localProvider = useMemo<UltraWalletProvider | null>(() => {
    if (sessionState !== "unlocked") return null;

    return {
      async request(request) {
        const material = materialRef.current;
        if (!material) throw new Error("The local wallet is locked. Unlock it before signing.");

        beginSigning();
        try {
          if (request.method === "ultranet_signValidatorProposal") {
            return await createLocalValidatorProvider(() => material).request(request);
          }
          return await createLocalTransactionProvider(material).request(request);
        } finally {
          endSigning();
        }
      },
    };
  }, [beginSigning, endSigning, sessionState]);

  const value = useMemo<WalletSessionContextValue>(() => ({
    sessionState,
    externalWalletStatus,
    walletAddress: materialRef.current?.address ?? null,
    storedWallet,
    storageError,
    localProvider,
    unlock,
    lock,
    beginSigning,
    endSigning,
  }), [
    beginSigning,
    endSigning,
    externalWalletStatus,
    localProvider,
    lock,
    sessionState,
    storageError,
    storedWallet,
    unlock,
  ]);

  return <WalletSessionContext.Provider value={value}>{children}</WalletSessionContext.Provider>;
}

export function useWalletSession(): WalletSessionContextValue {
  const context = useContext(WalletSessionContext);
  if (!context) throw new Error("useWalletSession must be used inside WalletSessionProvider.");
  return context;
}
