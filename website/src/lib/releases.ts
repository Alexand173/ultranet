export type ReleaseAssetPlatform = "windows" | "linux" | "macos";

export interface ReleaseAsset {
  readonly platform: ReleaseAssetPlatform;
  readonly label: string;
  readonly fileName: string;
  readonly containedBinary: string;
  readonly architecture: string;
  readonly description: string;
}

export const RELEASE_TAG = "v7.1.3" as const;
export const RELEASE_REPOSITORY_URL = "https://github.com/Alexand173/ultranet";
export const RELEASE_URL = `${RELEASE_REPOSITORY_URL}/releases/tag/${RELEASE_TAG}`;
export const RELEASE_ASSET_BASE_URL = `${RELEASE_REPOSITORY_URL}/releases/download/${RELEASE_TAG}`;
export const RELEASE_CHECKSUM_FILE_NAME = "SHA256SUMS.txt" as const;
export const RELEASE_CHECKSUM_URL = `${RELEASE_ASSET_BASE_URL}/${RELEASE_CHECKSUM_FILE_NAME}`;

export const RELEASE_ASSETS = [
  {
    platform: "windows",
    label: "Windows x64",
    fileName: "UltraNetNode-windows-x64.zip",
    containedBinary: "UltraNetNode.exe",
    architecture: "x86_64 / PE",
    description: "Precompiled Windows executable for x64 systems.",
  },
  {
    platform: "linux",
    label: "Linux x64",
    fileName: "UltraNetNode-linux-x64.tar.gz",
    containedBinary: "UltraNetNode",
    architecture: "x86_64 / ELF",
    description: "Precompiled Linux ELF node for x64 systems.",
  },
  {
    platform: "macos",
    label: "macOS x64",
    fileName: "UltraNetNode-macos-x64.tar.gz",
    containedBinary: "UltraNetNode",
    architecture: "x86_64 / Mach-O",
    description: "Precompiled Intel macOS node binary.",
  },
] as const satisfies readonly ReleaseAsset[];

export function releaseAssetUrl(fileName: string): string {
  return `${RELEASE_ASSET_BASE_URL}/${fileName}`;
}
