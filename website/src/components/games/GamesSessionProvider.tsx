"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useWalletSession } from "@/components/wallet/WalletSessionProvider";
import { logoutSession, requestChallenge, verifyChallenge } from "@/lib/games-api";
import type { AdminProof, GamesSession } from "@/lib/games.types";
import {
  ULTRA_WALLET_SIGN_GAME_ADMIN,
  ULTRA_WALLET_SIGN_GAMES_CHALLENGE,
  type SignGameAdminParams,
  type SignedGamesDigest,
} from "@/lib/ultra-wallet";

export type GamesAuthStatus = "signed-out" | "signing" | "signed-in" | "error";

interface GamesAuthValue {
  status: GamesAuthStatus;
  error: string | null;
  /** The address of the local wallet, known even while it is locked. */
  address: string | null;
  walletState: "checking" | "none" | "locked" | "unlocked";
  session: GamesSession | null;
  /** A valid session token, signing in first if there is none. Throws with a plain reason if it cannot. */
  ensureToken(): Promise<string>;
  /** Forget the session (for instance after the service refused it). */
  dropSession(): void;
  signOut(): Promise<void>;
  /** Sign one owner action with the unlocked local wallet. The service checks the signer against the L1 owner. */
  signAdmin(params: SignGameAdminParams): Promise<AdminProof>;
}

const Context = createContext<GamesAuthValue | null>(null);

/** A session is replaced this long before it expires, so a request never races the expiry. */
const MARGIN_SECONDS = 30;

/**
 * Wallet-signed sign-in for the Games pages. It never sees a recovery phrase: the unlocked local wallet signs a
 * one-time challenge in this tab and only the public key and signature leave it. The session is held in memory and
 * is dropped when the wallet locks, changes, or the tab closes.
 */
export default function GamesSessionProvider({ children }: { children: React.ReactNode }) {
  const wallet = useWalletSession();
  const [session, setSession] = useState<GamesSession | null>(null);
  const [status, setStatus] = useState<GamesAuthStatus>("signed-out");
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef<Promise<string> | null>(null);
  const address = wallet.walletAddress ?? wallet.storedWallet?.address ?? null;

  useEffect(() => {
    if (wallet.sessionState !== "unlocked" || (session && session.address !== wallet.walletAddress)) {
      setSession(null);
      setStatus("signed-out");
    }
  }, [wallet.sessionState, wallet.walletAddress, session]);

  const ensureToken = useCallback(async (): Promise<string> => {
    const now = Math.floor(Date.now() / 1000);
    if (session && session.expiresAt - MARGIN_SECONDS > now) return session.token;
    if (inFlight.current) return inFlight.current;
    if (wallet.sessionState !== "unlocked" || !wallet.localProvider || !wallet.walletAddress) {
      throw new Error("Unlock your wallet to sign in to Games.");
    }
    const provider = wallet.localProvider;
    const walletAddress = wallet.walletAddress;
    setStatus("signing");
    setError(null);
    const attempt = (async () => {
      try {
        const challenge = await requestChallenge(walletAddress);
        if (challenge.address !== walletAddress) throw new Error("The Games service answered for a different wallet. Nothing was signed.");
        const signed = (await provider.request({
          method: ULTRA_WALLET_SIGN_GAMES_CHALLENGE,
          params: {
            challengeId: challenge.challengeId,
            address: walletAddress,
            purpose: challenge.purpose,
            issuedAt: challenge.issuedAt,
            expiresAt: challenge.expiresAt,
            offeredDigest: challenge.digest,
          },
        })) as SignedGamesDigest;
        const verified = await verifyChallenge(challenge.challengeId, signed.publicKeyHex, signed.signatureHex);
        if (verified.address !== walletAddress) throw new Error("The Games service signed you in as a different wallet. The session was not used.");
        setSession(verified);
        setStatus("signed-in");
        return verified.token;
      } catch (failure) {
        const message = failure instanceof Error ? failure.message : "Sign-in failed.";
        setStatus("error");
        setError(message);
        throw new Error(message);
      } finally {
        inFlight.current = null;
      }
    })();
    inFlight.current = attempt;
    return attempt;
  }, [session, wallet.localProvider, wallet.sessionState, wallet.walletAddress]);

  const dropSession = useCallback(() => {
    setSession(null);
    setStatus("signed-out");
  }, []);

  const signOut = useCallback(async () => {
    const token = session?.token;
    setSession(null);
    setStatus("signed-out");
    if (token) {
      try {
        await logoutSession(token);
      } catch {
        // The session is gone from this tab either way; the service expires it on its own.
      }
    }
  }, [session]);

  const signAdmin = useCallback(
    async (params: SignGameAdminParams): Promise<AdminProof> => {
      if (wallet.sessionState !== "unlocked" || !wallet.localProvider) throw new Error("Unlock your wallet to sign this action.");
      const signed = (await wallet.localProvider.request({ method: ULTRA_WALLET_SIGN_GAME_ADMIN, params })) as SignedGamesDigest;
      if (signed.nonce === undefined || signed.timestamp === undefined) throw new Error("The wallet returned an incomplete signature.");
      return { nonce: signed.nonce, timestamp: signed.timestamp, publicKey: signed.publicKeyHex, signature: signed.signatureHex };
    },
    [wallet.localProvider, wallet.sessionState],
  );

  const value = useMemo<GamesAuthValue>(
    () => ({ status, error, address, walletState: wallet.sessionState, session, ensureToken, dropSession, signOut, signAdmin }),
    [address, dropSession, ensureToken, error, session, signAdmin, signOut, status, wallet.sessionState],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useGamesAuth(): GamesAuthValue {
  const value = useContext(Context);
  if (!value) throw new Error("useGamesAuth must be used inside GamesSessionProvider.");
  return value;
}
