import Link from "next/link";
import {
  ArrowRight,
  Download,
  ExternalLink,
  GitBranch,
  Laptop,
  Monitor,
  Package,
  Server,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import {
  RELEASE_ASSETS,
  RELEASE_CHECKSUM_FILE_NAME,
  RELEASE_CHECKSUM_URL,
  RELEASE_PIPELINE_STATUS,
  RELEASE_PIPELINE_STATUS_DESCRIPTION,
  RELEASE_REPOSITORY_URL,
  RELEASE_TAG,
  RELEASE_URL,
  releaseAssetUrl,
} from "@/lib/releases";

const PLATFORM_ICONS = {
  windows: Monitor,
  linux: Server,
  macos: Laptop,
} as const;

export default function NodeDownloadPage() {
  return (
    <main className="relative min-h-screen overflow-x-clip bg-ink-black terminal-overlay">
      <section className="relative mt-20 overflow-hidden border-b border-platinum/10 px-6 py-16 sm:px-10 lg:px-12 lg:py-24">
        <div className="absolute inset-0 dot-grid opacity-20" aria-hidden="true" />
        <div className="data-rain-col pointer-events-none absolute inset-0 opacity-20" aria-hidden="true" />
        <div className="relative z-10 mx-auto max-w-7xl">
          <p className="font-mono text-xs uppercase tracking-[0.24em] text-cyan-glow">
            root@ultranet:~/releases/{RELEASE_TAG}# <span className="blink-cursor" aria-hidden="true">_</span>
          </p>
          <h1
            data-text="SOVEREIGN_NODE_v7.1"
            className="glitch-text mt-6 max-w-4xl font-space-grotesk text-5xl font-bold leading-[0.95] tracking-[-0.05em] text-platinum sm:text-7xl"
          >
            Sovereign Node v7.1
          </h1>
          <p className="mt-6 max-w-2xl text-base leading-relaxed text-platinum/60 sm:text-lg">
            Download the public UltraNet node packages, verify their checksums, and start a validator before submitting a swarm proposal.
          </p>

          <dl className="mt-10 grid max-w-4xl grid-cols-1 border border-platinum/10 sm:grid-cols-3">
            <div className="border-b border-platinum/10 p-4 sm:border-b-0 sm:border-r">
              <dt className="font-mono text-[10px] uppercase tracking-[0.18em] text-platinum/40">Release tag</dt>
              <dd className="mt-2 font-mono text-sm text-cyan-glow">{RELEASE_TAG}</dd>
            </div>
            <div className="border-b border-platinum/10 p-4 sm:border-b-0 sm:border-r">
              <dt className="font-mono text-[10px] uppercase tracking-[0.18em] text-platinum/40">Targets</dt>
              <dd className="mt-2 font-mono text-sm text-platinum/80">WINDOWS / LINUX / MACOS</dd>
            </div>
            <div className="p-4">
              <dt className="font-mono text-[10px] uppercase tracking-[0.18em] text-platinum/40">Channel</dt>
              <dd className="mt-2 font-mono text-sm text-cyan-glow">STABLE // PUBLIC</dd>
            </div>
          </dl>
          <div className="mt-3 flex flex-col gap-1 border border-emerald-300/20 bg-emerald-300/[0.04] px-4 py-3 font-mono sm:flex-row sm:items-center sm:justify-between sm:gap-6">
            <div>
              <p className="text-[10px] uppercase tracking-[0.18em] text-platinum/40">Release pipeline</p>
              <p className="mt-1 text-sm text-emerald-300">{RELEASE_PIPELINE_STATUS}</p>
            </div>
            <p className="text-xs leading-relaxed text-platinum/50">{RELEASE_PIPELINE_STATUS_DESCRIPTION}</p>
          </div>
        </div>
      </section>

      <section className="relative z-10 mx-auto max-w-7xl px-6 py-16 sm:px-10 lg:px-12" aria-labelledby="node-packages-title">
        <div className="mb-8 flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">UltraNet Node binaries</p>
            <h2 id="node-packages-title" className="mt-3 font-space-grotesk text-3xl font-bold tracking-[-0.03em] text-platinum sm:text-4xl">
              Choose your platform
            </h2>
          </div>
          <a
            href={RELEASE_URL}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-2 self-start font-mono text-xs uppercase tracking-[0.16em] text-platinum/50 transition-colors hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black sm:self-auto"
          >
            View GitHub release <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </a>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {RELEASE_ASSETS.map((asset) => {
            const PlatformIcon = PLATFORM_ICONS[asset.platform];
            return (
              <article
                key={asset.fileName}
                className="group cut-corner flex min-h-80 flex-col border border-platinum/15 bg-platinum/[0.03] p-6 transition-all hover:-translate-y-1 hover:border-cyan-glow/60 hover:bg-cyan-glow/[0.06]"
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="flex h-11 w-11 items-center justify-center border border-cyan-glow/30 bg-cyan-glow/[0.06] text-cyan-glow transition-colors group-hover:border-cyan-glow">
                    <PlatformIcon className="h-5 w-5" aria-hidden="true" />
                  </div>
                  <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-cyan-glow/60">{asset.platform}_x64</span>
                </div>
                <h3 className="mt-8 font-space-grotesk text-xl font-bold text-platinum">{asset.label}</h3>
                <code className="mt-3 break-all font-mono text-xs leading-relaxed text-cyan-glow">{asset.fileName}</code>
                <p className="mt-3 text-sm leading-relaxed text-platinum/50">{asset.description}</p>

                <dl className="mt-6 space-y-2 border-t border-platinum/10 pt-4 font-mono text-[10px] uppercase tracking-[0.12em]">
                  <div className="flex justify-between gap-4">
                    <dt className="text-platinum/35">Contains</dt>
                    <dd className="text-right text-platinum/70">{asset.containedBinary}</dd>
                  </div>
                  <div className="flex justify-between gap-4">
                    <dt className="text-platinum/35">Format</dt>
                    <dd className="text-right text-platinum/70">{asset.architecture}</dd>
                  </div>
                </dl>

                <a
                  href={releaseAssetUrl(asset.fileName)}
                  target="_blank"
                  rel="noreferrer"
                  aria-label={`Download ${asset.fileName}`}
                  className="mt-auto inline-flex items-center justify-between gap-3 border-t border-platinum/10 pt-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-platinum transition-colors hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
                >
                  Download archive <Download className="h-4 w-4 transition-transform group-hover:translate-y-0.5" aria-hidden="true" />
                </a>
              </article>
            );
          })}
        </div>
      </section>

      <section className="relative z-10 mx-auto max-w-7xl px-6 pb-16 sm:px-10 lg:px-12" aria-labelledby="verify-title">
        <div className="crosshair-lines relative grid gap-8 border border-cyan-glow/20 bg-cyan-glow/[0.03] p-6 sm:p-8 lg:grid-cols-[0.8fr_1.2fr]">
          <div className="relative z-10">
            <ShieldCheck className="h-7 w-7 text-cyan-glow" aria-hidden="true" />
            <p className="mt-5 font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Security // verify before execution</p>
            <h2 id="verify-title" className="mt-3 font-space-grotesk text-2xl font-bold text-platinum">Trust the checksum, not the screenshot.</h2>
            <p className="mt-4 max-w-lg text-sm leading-relaxed text-platinum/55">
              Download <code className="text-platinum/80">{RELEASE_CHECKSUM_FILE_NAME}</code> from the same HTTPS release, compare the archive hash before extracting it, and never run an unfamiliar node as root.
            </p>
            <div className="mt-6 flex flex-wrap gap-4 font-mono text-[10px] uppercase tracking-[0.14em]">
              <a
                href={RELEASE_CHECKSUM_URL}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-2 text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
              >
                Download checksums <Download className="h-3.5 w-3.5" aria-hidden="true" />
              </a>
              <a
                href={RELEASE_URL}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-2 text-platinum/50 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
              >
                Inspect release <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
              </a>
            </div>
          </div>
          <pre className="relative z-10 overflow-x-auto border-l-2 border-cyan-glow bg-ink-black/80 p-5 font-mono text-xs leading-7 text-platinum/70"><code>{`# after downloading the archive and SHA256SUMS.txt
sha256sum --check SHA256SUMS.txt

# Linux / macOS example
chmod +x UltraNetNode
./UltraNetNode`}</code></pre>
        </div>
      </section>

      <section className="relative z-10 mx-auto max-w-7xl px-6 pb-16 sm:px-10 lg:px-12" aria-labelledby="deployment-title">
        <div className="mb-8">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Deployment matrix</p>
          <h2 id="deployment-title" className="mt-3 font-space-grotesk text-3xl font-bold tracking-[-0.03em] text-platinum sm:text-4xl">Start the node your way</h2>
        </div>
        <div className="grid gap-px bg-platinum/10 lg:grid-cols-2">
          <article className="bg-ink-black p-6 sm:p-8">
            <div className="flex items-center gap-3 text-cyan-glow">
              <Package className="h-5 w-5" aria-hidden="true" />
              <p className="font-mono text-xs uppercase tracking-[0.18em]">DOCKER_COMPOSE_MODE</p>
            </div>
            <h3 className="mt-6 font-space-grotesk text-2xl font-bold text-platinum">Fastest path for beginners</h3>
            <p className="mt-3 text-sm leading-relaxed text-platinum/50">From the repository root, Docker builds and runs the validator with the included compose configuration.</p>
            <pre className="mt-6 overflow-x-auto border border-platinum/10 bg-platinum/[0.03] p-4 font-mono text-xs leading-7 text-platinum/75"><code>{`# modern Docker CLI
docker compose up -d

# legacy Docker Compose command
docker-compose up -d`}</code></pre>
          </article>

          <article className="bg-ink-black p-6 sm:p-8">
            <div className="flex items-center gap-3 text-cyan-glow">
              <GitBranch className="h-5 w-5" aria-hidden="true" />
              <p className="font-mono text-xs uppercase tracking-[0.18em]">SOURCE_BUILD_MODE</p>
            </div>
            <h3 className="mt-6 font-space-grotesk text-2xl font-bold text-platinum">For advanced operators</h3>
            <p className="mt-3 text-sm leading-relaxed text-platinum/50">Install Rust, clone the repository, and build the locked production binary locally.</p>
            <pre className="mt-6 overflow-x-auto border border-platinum/10 bg-platinum/[0.03] p-4 font-mono text-xs leading-7 text-platinum/75"><code>{`# install Rust: https://rustup.rs/
curl --proto '=https' --tlsv1.2 -sSf \\
  https://sh.rustup.rs | sh
git clone ${RELEASE_REPOSITORY_URL}.git
cd ultranet
cargo build --release --locked --bin UltraNet`}</code></pre>
          </article>
        </div>
      </section>

      <section className="relative z-10 mx-auto max-w-7xl px-6 pb-24 sm:px-10 lg:px-12" aria-labelledby="join-cta-title">
        <div className="relative overflow-hidden border border-platinum/15 bg-platinum/[0.03] p-8 sm:p-12">
          <div className="absolute inset-0 hex-grid opacity-20" aria-hidden="true" />
          <div className="relative z-10 flex flex-col items-start justify-between gap-8 md:flex-row md:items-end">
            <div className="max-w-2xl">
              <Terminal className="h-6 w-6 text-cyan-glow" aria-hidden="true" />
              <p className="mt-5 font-mono text-xs uppercase tracking-[0.2em] text-cyan-glow">Next operation</p>
              <h2 id="join-cta-title" className="mt-3 font-space-grotesk text-3xl font-bold text-platinum sm:text-4xl">Run a node, then join the swarm.</h2>
              <p className="mt-4 text-sm leading-relaxed text-platinum/55">Once your node is running and your public key is ready, submit the signed validator proposal through the public onboarding flow.</p>
            </div>
            <Link
              href="/#swarm"
              className="inline-flex shrink-0 items-center gap-3 bg-platinum px-6 py-4 font-mono text-xs font-bold uppercase tracking-[0.16em] text-ink-black transition-colors hover:bg-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
            >
              Join the validator swarm <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </Link>
          </div>
        </div>
      </section>
    </main>
  );
}
