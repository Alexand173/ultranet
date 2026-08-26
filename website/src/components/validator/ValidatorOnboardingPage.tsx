import Link from "next/link";
import {
  AlertTriangle,
  ArrowRight,
  Check,
  Download,
  ExternalLink,
  GitBranch,
  Key,
  Package,
  Server,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import GenesisMultiaddr from "@/components/validator/GenesisMultiaddr";
import ValidatorFaq from "@/components/validator/ValidatorFaq";
import TwoKeyProposalExplainer from "@/components/validator/TwoKeyProposalExplainer";
import ValidatorIdentityGuide from "@/components/validator/ValidatorIdentityGuide";
import ProposalWalletPrerequisite from "@/components/wallet/ProposalWalletPrerequisite";
import WalletConnectionStatus from "@/components/wallet/WalletConnectionStatus";
import {
  RELEASE_CHECKSUM_URL,
  RELEASE_TAG,
  RELEASE_URL,
  VALIDATOR_GUIDE_URL,
  WINDOWS_RELEASE_URL,
} from "@/lib/validator";

const ONBOARDING_STEPS = [
  {
    number: "01",
    title: "Download",
    description: "Start with the verified Windows x64 package. Download the checksum manifest beside it and verify the archive before extracting anything.",
    icon: Download,
  },
  {
    number: "02",
    title: "Run",
    description: "Extract the complete package, launch Start-UltraNetNode.bat, create a private admin token locally, and wait for the configuration, FHE, and Genesis connection checks.",
    icon: Terminal,
  },
  {
    number: "03",
    title: "Submit proposal",
    description: "Export DILITHIUM_PUB_KEY.hex locally, choose your node alias, and prepare the public key. UltraWallet signs the proposal locally; activation still requires 2-of-3 Sovereign approval.",
    icon: ShieldCheck,
  },
] as const;

const PACKAGE_FILES = [
  "UltraNetNode.exe",
  "Start-UltraNetNode.bat",
  "UltraNetNode.env.example",
  "README-WINDOWS.txt",
] as const;

export default function ValidatorOnboardingPage() {
  return (
    <main className="relative min-h-screen overflow-x-clip bg-ink-black terminal-overlay">
      <section className="relative mt-20 overflow-hidden border-b border-platinum/10 px-6 py-16 sm:px-10 lg:px-12 lg:py-24">
        <div className="pointer-events-none absolute right-0 top-0 h-full w-1/3 dot-grid opacity-20" aria-hidden="true" />
        <div className="relative z-10 mx-auto grid max-w-[1680px] gap-12 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] lg:items-stretch lg:gap-16">
          <div className="flex h-full flex-col">
            <p className="font-mono text-xs uppercase tracking-[0.24em] text-cyan-glow">VALIDATOR_ONBOARDING // {RELEASE_TAG}</p>
            <h1 className="mt-6 max-w-4xl font-space-grotesk text-5xl font-bold leading-[0.95] tracking-[-0.05em] text-platinum sm:text-7xl">
              Become a validator
            </h1>
            <p className="mt-6 max-w-2xl text-base leading-relaxed text-platinum/60 sm:text-lg">
              Run a node, connect to Genesis, and submit a signed proposal to join the UltraNet swarm. The first launch is three clear steps—not a command-line puzzle.
            </p>
            <div className="mt-8 flex flex-wrap gap-x-6 gap-y-3 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/50">
              <span className="inline-flex items-center gap-2"><Key className="h-3.5 w-3.5 text-cyan-glow" aria-hidden="true" /> Dilithium-5</span>
              <span className="inline-flex items-center gap-2"><Server className="h-3.5 w-3.5 text-cyan-glow" aria-hidden="true" /> P2P 9000 TCP + UDP</span>
              <span className="inline-flex items-center gap-2"><GitBranch className="h-3.5 w-3.5 text-cyan-glow" aria-hidden="true" /> 2-of-3 approval</span>
            </div>
            <div className="mt-12 border-t border-platinum/10 pt-6 lg:mt-auto lg:max-w-xl">
              <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Before you start</p>
              <ul className="mt-4 grid gap-3 text-sm leading-6 text-platinum/60 sm:grid-cols-2 lg:grid-cols-1">
                <li><span className="mr-2 text-cyan-glow">01</span>Download the verified node package and checksum manifest.</li>
                <li><span className="mr-2 text-cyan-glow">02</span>Run the node until the Genesis connection is visible.</li>
                <li><span className="mr-2 text-cyan-glow">03</span>Keep the public export file separate from the private node identity.</li>
                <li><span className="mr-2 text-cyan-glow">04</span>Unlock Send Ultra before signing the governance proposal.</li>
              </ul>
              <Link href="#steps" className="mt-5 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
                Follow the join sequence <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
              </Link>
            </div>
          </div>

          <div className="space-y-8">
            <div className="border-l border-cyan-glow/30 pl-5 font-mono text-xs leading-6 text-platinum/55">
              <p className="text-cyan-glow">ENTRY_CHECK // PUBLIC</p>
              <p className="mt-3">Node package: <span className="text-platinum/80">{RELEASE_TAG}</span></p>
              <p>Network: <span className="text-platinum/80">ultranet-sovereign-mainnet</span></p>
              <p>API access: <span className="text-amber-200">private / operator-controlled</span></p>
              <p className="mt-4 border-t border-platinum/10 pt-4 text-platinum/40">A public onboarding page never asks for your private key, wallet backup, or admin token.</p>
            </div>
            <TwoKeyProposalExplainer />
          </div>
        </div>
      </section>

      <section id="steps" aria-labelledby="validator-steps-title" className="relative z-10 mx-auto max-w-[1680px] px-6 py-16 sm:px-10 lg:px-12 lg:py-20">
        <div className="mb-8 flex flex-col justify-between gap-5 sm:flex-row sm:items-end">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Join sequence</p>
            <h2 id="validator-steps-title" className="mt-3 font-space-grotesk text-3xl font-bold tracking-tight text-platinum sm:text-4xl">Three steps to join</h2>
          </div>
          <a href={RELEASE_URL} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/55 transition-colors hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
            View GitHub release {RELEASE_TAG} <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </a>
        </div>

        <ol className="border-y border-platinum/15">
          {ONBOARDING_STEPS.map((step, index) => {
            const Icon = step.icon;
            return (
              <li key={step.number} className="grid gap-5 border-b border-platinum/15 py-7 last:border-b-0 md:grid-cols-[72px_28px_1fr_auto] md:items-start md:gap-6">
                <span className="font-mono text-sm text-cyan-glow/80">{step.number}</span>
                <Icon className="h-5 w-5 text-cyan-glow" aria-hidden="true" />
                <div>
                  <h3 className="font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">{step.title}</h3>
                  <p className="mt-2 max-w-2xl text-sm leading-7 text-platinum/55">{step.description}</p>
                  {index === 0 && (
                    <div className="mt-4 flex flex-wrap gap-4 font-mono text-[10px] uppercase tracking-[0.14em]">
                      <a href={WINDOWS_RELEASE_URL} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center gap-2 text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
                        Download Windows x64 <Download className="h-3.5 w-3.5" aria-hidden="true" />
                      </a>
                      <a href={RELEASE_CHECKSUM_URL} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center gap-2 text-platinum/50 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
                        Download checksums <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                      </a>
                    </div>
                  )}
                  {index === 1 && (
                    <a href="#windows-runbook" className="mt-4 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
                      See the Windows runbook <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
                    </a>
                  )}
                  {index === 2 && (
                    <Link href="/#swarm" className="mt-4 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
                      Submit your proposal <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
                    </Link>
                  )}
                </div>
                <span className="hidden font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/25 md:block">{step.number === "01" ? "GET_FILES" : step.number === "02" ? "START_NODE" : "GOVERNANCE"}</span>
              </li>
            );
          })}
        </ol>
      </section>

      <section id="windows-runbook" aria-labelledby="windows-runbook-title" className="relative z-10 mx-auto max-w-[1680px] px-6 pb-16 sm:px-10 lg:px-12">
        <div className="grid gap-px border border-platinum/10 bg-platinum/10 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
          <div className="bg-ink-black p-6 sm:p-8">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Windows x64 // first launch</p>
            <h2 id="windows-runbook-title" className="mt-3 font-space-grotesk text-2xl font-bold tracking-tight text-platinum sm:text-3xl">What the package contains</h2>
            <p className="mt-4 text-sm leading-7 text-platinum/55">Extract the complete release archive into a writable folder. The launcher creates the private sibling environment file on its first run.</p>
            <ul className="mt-7 space-y-3 border-t border-platinum/10 pt-5 font-mono text-xs text-platinum/70">
              {PACKAGE_FILES.map((file) => (
                <li key={file} className="flex items-center gap-3"><Check className="h-4 w-4 shrink-0 text-cyan-glow" aria-hidden="true" /><code>{file}</code></li>
              ))}
            </ul>
            <div className="mt-8 space-y-6 border-t border-platinum/10 pt-8">
              <article aria-labelledby="admin-token-explanation-title" className="min-w-0 border border-amber-300/30 bg-amber-300/10 p-5">
                <div className="flex items-start gap-3"><AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-200" aria-hidden="true" /><div><p className="font-mono text-[10px] uppercase tracking-[0.18em] text-amber-200">01 // Private node password</p><h3 id="admin-token-explanation-title" className="mt-2 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">What is ULTRANET_ADMIN_TOKEN?</h3></div></div>
                <p className="mt-4 text-sm leading-7 text-amber-100/85">Treat this token like a private key or a strong password for your node: anyone who has it can call protected administrator operations such as mining and maintenance. Technically, it is <strong className="text-amber-100">not</strong> your Dilithium wallet private key, not your <code> DILITHIUM_PUB_KEY.hex</code>, and not your public Peer ID. It cannot sign a wallet transfer or validator proposal.</p>
                <p className="mt-3 text-sm leading-7 text-amber-100/85"><strong className="text-amber-100">Generate it locally, never on an online website.</strong> An internet generator could record the value. Use one of these commands on the same computer that runs your node:</p>
                <div className="mt-5 grid gap-4 sm:grid-cols-2">
                  <div>
                    <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-amber-200/80">Linux // macOS // Git Bash</p>
                    <pre className="mt-2 overflow-x-auto border border-amber-200/20 bg-ink-black/50 p-3 font-mono text-[11px] leading-6 text-platinum/80"><code>{`openssl rand -hex 32`}</code></pre>
                  </div>
                  <div>
                    <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-amber-200/80">Windows PowerShell</p>
                    <pre className="mt-2 overflow-x-auto border border-amber-200/20 bg-ink-black/50 p-3 font-mono text-[11px] leading-6 text-platinum/80"><code>{`$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$token = [BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
$rng.Dispose()
$token`}</code></pre>
                  </div>
                </div>
                <ol className="mt-5 space-y-2 border-t border-amber-200/15 pt-4 text-xs leading-6 text-amber-100/80">
                  <li><span className="mr-2 font-mono text-amber-200">01</span>Copy the one-time output into <code>UltraNetNode.env</code> as <code>ULTRANET_ADMIN_TOKEN=your-64-hex-character-value</code>.</li>
                  <li><span className="mr-2 font-mono text-amber-200">02</span>Save the file, close Notepad, and run <code>Start-UltraNetNode.bat</code> again. Never commit, email, record, or paste the token into this website.</li>
                  <li><span className="mr-2 font-mono text-amber-200">03</span>In your first-run console, <code>Administrative bearer authentication: enabled</code> means the token was accepted. <code>P2P node is running</code> means the node started; <code>Peer ID</code> is only the public networking identity.</li>
                </ol>
              </article>
            </div>
          </div>

          <div className="bg-ink-black p-6 sm:p-8">
            <div className="flex items-center gap-3 text-cyan-glow"><Package className="h-5 w-5" aria-hidden="true" /><p className="font-mono text-xs uppercase tracking-[0.18em]">RUNBOOK // POWERSHELL</p></div>
            <h3 className="mt-6 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Verify, configure, start</h3>
            <pre className="mt-5 overflow-x-auto border border-platinum/10 bg-platinum/[0.03] p-4 font-mono text-xs leading-7 text-platinum/75"><code>{`# verify the v${RELEASE_TAG.replace("v", "")} Windows archive
$expected = (Get-Content .\\SHA256SUMS.txt | Where-Object { $_ -match 'UltraNetNode-windows-x64\\.zip$' }).Split()[0]
$actual = (Get-FileHash .\\UltraNetNode-windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
"Checksum OK"

# after extraction, launch the desktop-first flow
.\\Start-UltraNetNode.bat`}</code></pre>
            <ol className="mt-6 grid gap-3 border-t border-platinum/10 pt-5 text-sm leading-6 text-platinum/55 sm:grid-cols-2">
              <li><span className="font-mono text-cyan-glow">01</span> The launcher copies <code className="text-platinum/80">UltraNetNode.env.example</code> to <code className="text-platinum/80">UltraNetNode.env</code> and opens it in Notepad.</li>
              <li><span className="font-mono text-cyan-glow">02</span> Generate the token locally, save the sibling env file, and close Notepad.</li>
              <li><span className="font-mono text-cyan-glow">03</span> The launcher runs <code className="text-platinum/80">--check-config</code> and <code className="text-platinum/80">--check-fhe</code>.</li>
              <li><span className="font-mono text-cyan-glow">04</span> Keep the console open to confirm Genesis connection and recurring heartbeat lines.</li>
            </ol>
            <ValidatorIdentityGuide />
          </div>
        </div>
      </section>

      <div className="mx-auto max-w-[1680px] px-6 pb-16 sm:px-10 lg:px-12"><GenesisMultiaddr /></div>

      <section aria-labelledby="proposal-handoff-title" className="relative z-10 mx-auto max-w-[1680px] px-6 pb-16 sm:px-10 lg:px-12">
        <div className="flex flex-col justify-between gap-8 border border-platinum/15 bg-platinum/[0.03] p-6 sm:p-8 md:flex-row md:items-end">
          <div className="max-w-2xl">
            <ShieldCheck className="h-6 w-6 text-cyan-glow" aria-hidden="true" />
            <p className="mt-5 font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Proposal handoff</p>
            <h2 id="proposal-handoff-title" className="mt-3 font-space-grotesk text-2xl font-bold tracking-tight text-platinum sm:text-3xl">Your node is ready for governance review.</h2>
            <p className="mt-4 text-sm leading-7 text-platinum/55">Use the existing proposal flow to provide a node alias and public key. UltraWallet signs locally, the node receives only public transaction fields, and 2-of-3 Sovereign approval is required before activation.</p>
          </div>
          <div className="flex min-w-0 shrink-0 flex-col items-start gap-4 md:w-[22rem]">
            <WalletConnectionStatus variant="dark" />
            <ProposalWalletPrerequisite variant="dark" className="w-full" />
            <Link href="/#swarm" className="inline-flex min-h-11 items-center gap-3 bg-cyan-glow px-6 py-4 font-mono text-xs font-bold uppercase tracking-[0.16em] text-ink-black transition-colors hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
              Submit your proposal <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </Link>
            <a href={VALIDATOR_GUIDE_URL} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/50 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
              Read the Validator Guide <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            </a>
          </div>
        </div>
      </section>

      <ValidatorFaq />
    </main>
  );
}
