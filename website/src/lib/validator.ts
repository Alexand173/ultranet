import {
  RELEASE_ASSETS,
  RELEASE_CHECKSUM_URL,
  RELEASE_REPOSITORY_URL,
  RELEASE_TAG,
  RELEASE_URL,
  releaseAssetUrl,
} from "@/lib/releases";

export const GENESIS_MULTIADDR =
  "/ip4/167.233.161.115/tcp/9000/p2p/12D3KooWRFWD4VDW7g2t4VEmajjyfrGh5ZuQUoPVxFeq7ffRetgP" as const;

export const GENESIS_P2P_PORT = 9000 as const;
export const GENESIS_API_PORT = 8081 as const;
export const GENESIS_TRANSPORTS = ["TCP", "UDP"] as const;

export const WINDOWS_VALIDATOR_PRIVATE_KEY_PATH = String.raw`C:\Users\YourName\AppData\Local\UltraNet\data\validator_dilithium5_key.json`;

const windowsReleaseAsset = RELEASE_ASSETS.find((asset) => asset.platform === "windows");

if (!windowsReleaseAsset) {
  throw new Error(`The ${RELEASE_TAG} release is missing its Windows asset.`);
}

export const WINDOWS_RELEASE_ASSET = windowsReleaseAsset;
export const WINDOWS_RELEASE_URL = releaseAssetUrl(WINDOWS_RELEASE_ASSET.fileName);

export const VALIDATOR_GUIDE_URL = `${RELEASE_REPOSITORY_URL}/blob/main/VALIDATOR_GUIDE.md`;

export { RELEASE_CHECKSUM_URL, RELEASE_TAG, RELEASE_URL };
