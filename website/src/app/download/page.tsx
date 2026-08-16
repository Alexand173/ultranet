import type { Metadata } from "next";
import NodeDownloadPage from "@/components/download/NodeDownloadPage";

export const metadata: Metadata = {
  title: "Download UltraNet Node | v7.1.3",
  description: "Download the signed UltraNet v7.1.3 sovereign node packages for Windows, Linux, or macOS.",
};

export default function DownloadPage() {
  return <NodeDownloadPage />;
}
