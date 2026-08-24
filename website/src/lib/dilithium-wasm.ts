const PUBLIC_KEY_BYTES = 2_592;
const SECRET_KEY_BYTES = 4_896;
const SIGNATURE_BYTES = 4_627;
const STACK_POINTER = 8 * 1024 * 1024;

type WasmExports = {
  memory: WebAssembly.Memory;
  __stack_pointer: WebAssembly.Global;
  malloc(size: number): number;
  ultra_reset_heap(): void;
  ultra_public_key_bytes(): number;
  ultra_secret_key_bytes(): number;
  ultra_signature_bytes(): number;
  ultra_keypair(seed: number, publicKey: number, secretKey: number): number;
  ultra_sign(message: number, messageLength: number, secretKey: number, signature: number): number;
  ultra_verify(
    signature: number,
    signatureLength: number,
    message: number,
    messageLength: number,
    publicKey: number,
  ): number;
};

type WasmContext = {
  exports: WasmExports;
  bytes: Uint8Array;
};

let contextPromise: Promise<WasmContext> | undefined;
let operationQueue = Promise.resolve();

function copyBytes(bytes: Uint8Array): Uint8Array {
  return new Uint8Array(bytes);
}

function wipe(bytes: Uint8Array): void {
  bytes.fill(0);
}

async function loadContext(): Promise<WasmContext> {
  if (!contextPromise) {
    contextPromise = (async () => {
      const response = await fetch("/crypto/ultra-dilithium5.wasm", { cache: "force-cache" });
      if (!response.ok) throw new Error("Unable to load the local Dilithium-5 module.");
      const bytes = await response.arrayBuffer();
      const { instance } = await WebAssembly.instantiate(bytes, {});
      const exports = instance.exports as unknown as WasmExports;

      if (
        exports.ultra_public_key_bytes() !== PUBLIC_KEY_BYTES ||
        exports.ultra_secret_key_bytes() !== SECRET_KEY_BYTES ||
        exports.ultra_signature_bytes() !== SIGNATURE_BYTES
      ) {
        throw new Error("The local Dilithium-5 module has incompatible byte sizes.");
      }

      exports.__stack_pointer.value = STACK_POINTER;
      return { exports, bytes: new Uint8Array(exports.memory.buffer) };
    })();
  }

  return contextPromise;
}

async function withOperation<T>(operation: (context: WasmContext) => Promise<T> | T): Promise<T> {
  const result = operationQueue.then(() => loadContext().then(operation));
  operationQueue = result.then(
    () => undefined,
    () => undefined,
  );
  return result;
}

function allocate(context: WasmContext, bytes: Uint8Array | number): number {
  const length = typeof bytes === "number" ? bytes : bytes.length;
  const pointer = context.exports.malloc(length);
  if (!pointer) throw new Error("Dilithium-5 scratch memory is exhausted.");
  if (typeof bytes !== "number") context.bytes.set(bytes, pointer);
  return pointer;
}

export interface DilithiumKeyPair {
  publicKey: Uint8Array;
  secretKey: Uint8Array;
}

export const DILITHIUM5_SIZES = {
  publicKey: PUBLIC_KEY_BYTES,
  secretKey: SECRET_KEY_BYTES,
  signature: SIGNATURE_BYTES,
} as const;

export async function generateDilithium5KeyPair(seed: Uint8Array): Promise<DilithiumKeyPair> {
  if (seed.length !== 32) throw new Error("Dilithium-5 key generation requires a 32-byte seed.");

  return withOperation((context) => {
    const { exports, bytes } = context;
    exports.ultra_reset_heap();
    const seedPointer = allocate(context, seed);
    const publicKeyPointer = allocate(context, PUBLIC_KEY_BYTES);
    const secretKeyPointer = allocate(context, SECRET_KEY_BYTES);

    try {
      if (exports.ultra_keypair(seedPointer, publicKeyPointer, secretKeyPointer) !== 0) {
        throw new Error("Dilithium-5 key generation failed.");
      }
      return {
        publicKey: copyBytes(bytes.subarray(publicKeyPointer, publicKeyPointer + PUBLIC_KEY_BYTES)),
        secretKey: copyBytes(bytes.subarray(secretKeyPointer, secretKeyPointer + SECRET_KEY_BYTES)),
      };
    } finally {
      bytes.fill(0, seedPointer, seedPointer + seed.length);
      bytes.fill(0, publicKeyPointer, publicKeyPointer + PUBLIC_KEY_BYTES);
      bytes.fill(0, secretKeyPointer, secretKeyPointer + SECRET_KEY_BYTES);
    }
  });
}

export async function signDilithium5(
  message: Uint8Array,
  secretKey: Uint8Array,
): Promise<Uint8Array> {
  if (secretKey.length !== SECRET_KEY_BYTES) throw new Error("Invalid Dilithium-5 secret key length.");

  return withOperation((context) => {
    const { exports, bytes } = context;
    exports.ultra_reset_heap();
    const messagePointer = allocate(context, message);
    const secretKeyPointer = allocate(context, secretKey);
    const signaturePointer = allocate(context, SIGNATURE_BYTES);

    try {
      if (exports.ultra_sign(messagePointer, message.length, secretKeyPointer, signaturePointer) !== 0) {
        throw new Error("Dilithium-5 signing failed.");
      }
      return copyBytes(bytes.subarray(signaturePointer, signaturePointer + SIGNATURE_BYTES));
    } finally {
      bytes.fill(0, messagePointer, messagePointer + message.length);
      bytes.fill(0, secretKeyPointer, secretKeyPointer + SECRET_KEY_BYTES);
      bytes.fill(0, signaturePointer, signaturePointer + SIGNATURE_BYTES);
    }
  });
}

export async function verifyDilithium5(
  signature: Uint8Array,
  message: Uint8Array,
  publicKey: Uint8Array,
): Promise<boolean> {
  if (signature.length !== SIGNATURE_BYTES) return false;
  if (publicKey.length !== PUBLIC_KEY_BYTES) return false;

  return withOperation((context) => {
    const { exports, bytes } = context;
    exports.ultra_reset_heap();
    const signaturePointer = allocate(context, signature);
    const messagePointer = allocate(context, message);
    const publicKeyPointer = allocate(context, publicKey);

    try {
      return exports.ultra_verify(
        signaturePointer,
        signature.length,
        messagePointer,
        message.length,
        publicKeyPointer,
      ) === 0;
    } finally {
      bytes.fill(0, signaturePointer, signaturePointer + signature.length);
      bytes.fill(0, messagePointer, messagePointer + message.length);
      bytes.fill(0, publicKeyPointer, publicKeyPointer + publicKey.length);
    }
  });
}

export function clearKeyPair(keyPair: DilithiumKeyPair): void {
  wipe(keyPair.publicKey);
  wipe(keyPair.secretKey);
}
