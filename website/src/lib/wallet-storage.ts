import type { StoredWallet } from "@/lib/wallet-crypto";

const DATABASE_NAME = "ultranet-wallet";
const DATABASE_VERSION = 1;
const STORE_NAME = "keystore";
const WALLET_ID = "primary";

function ensureIndexedDb(): IDBFactory {
  if (typeof indexedDB === "undefined") {
    throw new Error("This browser does not support secure local wallet storage.");
  }
  return indexedDB;
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = ensureIndexedDb().open(DATABASE_NAME, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(STORE_NAME)) {
        request.result.createObjectStore(STORE_NAME);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error("Unable to open secure local wallet storage."));
  });
}

export async function loadStoredWallet(): Promise<StoredWallet | null> {
  const database = await openDatabase();
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, "readonly");
    const request = transaction.objectStore(STORE_NAME).get(WALLET_ID);
    request.onsuccess = () => resolve((request.result as StoredWallet | undefined) ?? null);
    request.onerror = () => reject(new Error("Unable to read the local wallet."));
    transaction.oncomplete = () => database.close();
    transaction.onerror = () => reject(new Error("Unable to read the local wallet."));
  });
}

export async function saveStoredWallet(wallet: StoredWallet): Promise<void> {
  const database = await openDatabase();
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).put(wallet, WALLET_ID);
    transaction.oncomplete = () => {
      database.close();
      resolve();
    };
    transaction.onerror = () => {
      database.close();
      reject(new Error("This browser could not securely save the wallet."));
    };
    transaction.onabort = () => {
      database.close();
      reject(new Error("This browser could not securely save the wallet."));
    };
  });
}

export async function removeStoredWallet(): Promise<void> {
  const database = await openDatabase();
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).delete(WALLET_ID);
    transaction.oncomplete = () => {
      database.close();
      resolve();
    };
    transaction.onerror = () => {
      database.close();
      reject(new Error("Unable to remove the local wallet."));
    };
  });
}
