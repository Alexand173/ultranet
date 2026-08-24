import { ChevronDown, HelpCircle } from "lucide-react";
import type { ReactNode } from "react";

type FaqItem = {
  question: string;
  answer: ReactNode;
};

const FAQ_ITEMS: FaqItem[] = [
  {
    question: "What is a validator?",
    answer: (
      <>
        A validator runs the UltraNet node and helps receive, verify, execute, and commit network activity. Validators participate in the Bullshark / Mysticeti DAG and the Block-STM execution layer after their signed proposal is approved.
      </>
    ),
  },
  {
    question: "How do I mine or run a validator?",
    answer: (
      <>
        Keep the node online, allow inbound TCP and UDP port 9000, connect it to the Genesis multiaddr above, and submit a signed validator proposal. The node-admin <code className="text-platinum/80">/api/mine</code> route is protected for operators; never put an admin token in the website or share it as part of onboarding.
      </>
    ),
  },
  {
    question: "How much can a validator earn?",
    answer: (
      <>
        The current documented Era 0 base reward is <strong className="text-platinum">50.0 $ULTRA per successfully mined block</strong>, subject to halving and AI Governor adjustment. A validator also receives 100% of the transaction fees in its packaged block. Actual results depend on approval, uptime, participation, network conditions, and protocol state.
      </>
    ),
  },
  {
    question: "Is validator income guaranteed?",
    answer: (
      <>
        No. There is no fixed or guaranteed income. Validator admission requires a signed proposal and 2-of-3 Sovereign approval, and the protocol may jail a validator after repeated verification or state-root failures. Treat rewards as protocol outcomes, not a promise.
      </>
    ),
  },
  {
    question: "What must stay private?",
    answer: (
      <>
        Keep your private signing key, wallet backup, and <code className="text-platinum/80">ULTRANET_ADMIN_TOKEN</code> off the website, out of screenshots, and out of the repository. The admin token protects node operations; it is not a wallet key, public node identifier, or ordinary user login credential.
      </>
    ),
  },
];

export default function ValidatorFaq() {
  return (
    <section aria-labelledby="validator-faq-title" className="relative z-10 mx-auto max-w-7xl px-6 pb-24 sm:px-10 lg:px-12">
      <div className="grid gap-10 border-t border-platinum/10 pt-12 lg:grid-cols-[0.75fr_1.25fr] lg:gap-20">
        <div>
          <div className="flex items-center gap-3 text-cyan-glow">
            <HelpCircle className="h-5 w-5" aria-hidden="true" />
            <p className="font-mono text-[10px] uppercase tracking-[0.2em]">Operator questions</p>
          </div>
          <h2 id="validator-faq-title" className="mt-4 font-space-grotesk text-3xl font-bold tracking-tight text-platinum sm:text-4xl">
            Validator FAQ
          </h2>
          <p className="mt-4 max-w-md text-sm leading-relaxed text-platinum/55">
            Plain-language answers for the first node launch. Read the full operational requirements in the Validator Guide before exposing a production node.
          </p>
        </div>

        <div className="border-t border-platinum/15">
          {FAQ_ITEMS.map((item) => (
            <details key={item.question} className="group border-b border-platinum/15">
              <summary className="flex min-h-16 cursor-pointer list-none items-center justify-between gap-6 py-5 font-space-grotesk text-base font-bold uppercase tracking-[0.03em] text-platinum transition-colors hover:text-cyan-glow focus:outline-none focus-visible:text-cyan-glow [&::-webkit-details-marker]:hidden">
                {item.question}
                <ChevronDown className="h-4 w-4 shrink-0 text-cyan-glow transition-transform group-open:rotate-180" aria-hidden="true" />
              </summary>
              <p className="max-w-3xl pb-6 pr-8 text-sm leading-7 text-platinum/60">{item.answer}</p>
            </details>
          ))}
        </div>
      </div>
    </section>
  );
}
