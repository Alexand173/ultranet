import type { UltraWalletProvider } from "@/lib/ultra-wallet";

declare global {
  interface Window {
    ultraWallet?: UltraWalletProvider;
  }
}

export {};
