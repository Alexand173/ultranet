import type { Metadata } from "next";
import OperatorConsole from "@/components/operator/OperatorConsole";

export const metadata: Metadata = {
  title: "UltraNet Operator Console | Authenticated Operations",
  description: "Authenticated operator telemetry and state-changing UltraNet operations.",
};

export default function OperatorPage() {
  return <OperatorConsole />;
}
