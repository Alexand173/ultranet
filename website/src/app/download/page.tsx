import type { Metadata } from "next";
import NodeDownloadPage from "@/components/download/NodeDownloadPage";

export const metadata: Metadata = {
  title: "Download UltraNet Node | v7.1.6",
  description:
    "Download the verified UltraNet v7.1.6 sovereign node packages for Windows, Linux, or macOS. All platform builds and published asset verification passed.",
};

export default function DownloadPage() {
  return <NodeDownloadPage />;
}
