import type { Metadata } from "next";
import NodeDownloadPage from "@/components/download/NodeDownloadPage";
import { RELEASE_TAG } from "@/lib/releases";

export const metadata: Metadata = {
  title: `Download UltraNet Node | ${RELEASE_TAG}`,
  description: `Download the verified UltraNet ${RELEASE_TAG} sovereign node packages for Windows, Linux, or macOS. All platform builds and published asset verification passed.`,
};

export default function DownloadPage() {
  return <NodeDownloadPage />;
}
