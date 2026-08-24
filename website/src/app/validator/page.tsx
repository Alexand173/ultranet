import type { Metadata } from "next";
import ValidatorOnboardingPage from "@/components/validator/ValidatorOnboardingPage";
import { RELEASE_TAG } from "@/lib/validator";

export const metadata: Metadata = {
  title: `Validator Onboarding | UltraNet ${RELEASE_TAG}`,
  description: "Run an UltraNet node, connect to Genesis, and submit a signed validator proposal.",
};

export default function ValidatorPage() {
  return <ValidatorOnboardingPage />;
}
