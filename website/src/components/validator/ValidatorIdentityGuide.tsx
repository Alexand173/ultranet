import { Key } from "lucide-react";
import { WINDOWS_VALIDATOR_PRIVATE_KEY_PATH } from "@/lib/validator";

export default function ValidatorIdentityGuide() {
  return (
    <article aria-labelledby="validator-identity-explanation-title" className="mt-6 min-w-0 border border-cyan-glow/25 bg-cyan-glow/[0.03] p-5 sm:mt-8">
      <div className="flex items-start gap-3">
        <Key className="mt-0.5 h-4 w-4 shrink-0 text-cyan-glow" aria-hidden="true" />
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow">Validator identity</p>
          <h3 id="validator-identity-explanation-title" className="mt-2 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Where the validator key lives</h3>
        </div>
      </div>
      <p className="mt-4 text-sm leading-7 text-platinum/65"><strong className="text-platinum">Node alias</strong> is the human-readable name shown to operators and governance, such as <code className="text-platinum/85">Genesis-Alpha-01</code>. It is not a password, login, wallet address, or private key.</p>
      <p className="mt-3 text-sm leading-7 text-platinum/65"><strong className="text-platinum"><code>DILITHIUM_PUB_KEY.hex</code></strong> is the public half of the validator&apos;s Dilithium-5 identity. Paste the complete 5,184-character public value into the proposal form. The matching private/secret half stays on the node computer and must never be uploaded.</p>
      <div className="mt-4 border border-amber-200/20 bg-amber-200/[0.04] p-4 text-xs leading-6 text-platinum/70">
        <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-amber-200">Windows default private file</p>
        <code className="mt-2 block break-all font-mono text-[10px] leading-5 text-amber-100/90">{WINDOWS_VALIDATOR_PRIVATE_KEY_PATH}</code>
        <p className="mt-3">This is the private node identity created by the node, not the public export file. The default is based on <code className="text-platinum/85">%LOCALAPPDATA%\UltraNet\data</code>; if <code className="text-platinum/85">ULTRANET_DB_PATH</code> is set, that explicit directory wins.</p>
      </div>
      <div className="mt-4 border border-cyan-glow/15 bg-ink-black/60 p-4 text-xs leading-6 text-platinum/65">
        <p><strong className="text-cyan-glow">Do not copy the Peer ID.</strong> The first-run console shows a public libp2p networking identity. <code className="text-platinum/85">P2P node is running</code> confirms the process started; neither value is the Dilithium key.</p>
        <p className="mt-3">Run the export command from the folder containing the node binary. It creates the stable local identity once and writes only its public key; it refuses to overwrite an existing public-key file.</p>
        <pre className="mt-4 max-w-full overflow-x-auto whitespace-pre-wrap break-words border border-cyan-glow/15 bg-ink-black/70 p-3 font-mono text-[11px] leading-6 text-platinum/80"><code>{`# Windows PowerShell
.\\UltraNetNode.exe --export-validator-public-key .\\DILITHIUM_PUB_KEY.hex

# Linux / macOS
./UltraNetNode --export-validator-public-key ./DILITHIUM_PUB_KEY.hex`}</code></pre>
        <p className="mt-3">Open <code className="text-platinum/85">DILITHIUM_PUB_KEY.hex</code>, copy its complete line into the proposal form, and keep <code className="text-platinum/85">validator_dilithium5_key.json</code> private. Never invent or download a key from an online generator.</p>
      </div>
      <ol className="mt-5 space-y-2 border-t border-cyan-glow/15 pt-4 text-xs leading-6 text-platinum/65">
        <li><span className="mr-2 font-mono text-cyan-glow">01</span>Keep the node running and open inbound TCP and UDP port <code className="text-platinum/85">9000</code>. A first heartbeat with <code className="text-platinum/85">tracked peers: 0</code> means no peer is connected yet; it is not the Dilithium key.</li>
        <li><span className="mr-2 font-mono text-cyan-glow">02</span>Enter the alias and complete public key in the proposal form, connect your personal wallet, and let it sign locally.</li>
        <li><span className="mr-2 font-mono text-cyan-glow">03</span>Submission places the proposal in governance; 2-of-3 Sovereign approval is still required before activation.</li>
      </ol>
    </article>
  );
}
