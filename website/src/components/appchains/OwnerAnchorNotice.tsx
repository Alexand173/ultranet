/**
 * Anchoring is an owner-signed L1 transaction, and this dashboard holds no owner key, so there is no
 * button here to press. It replaces the old "ANCHOR NOW" control, which posted to
 * `/api/appchain/{id}/anchor` — a route the node now answers with 410 Gone.
 *
 * The notice is deliberately inert: no onClick, no link that starts a write, no spinner that implies
 * something is happening. It says who can do the thing and where they do it.
 */
interface OwnerAnchorNoticeProps {
  chainId: number;
  /** Registry chains predate the ledger and have a treasury; ledger chains do not. */
  hasTreasury?: boolean;
}

export default function OwnerAnchorNotice({ chainId, hasTreasury = false }: OwnerAnchorNoticeProps) {
  return (
    <div
      className="min-w-44 border border-cyan-glow/20 bg-cyan-glow/[0.04] px-3 py-2"
      title={`Anchors for AppChain #${chainId} are submitted by its owner signing a version-5 transaction in the UltraNet mobile wallet.`}
    >
      <p className="font-mono text-[10px] font-bold uppercase tracking-[0.12em] text-cyan-glow/75">
        ANCHOR_FROM_OWNER_WALLET
      </p>
      <p className="mt-1 max-w-56 text-[11px] leading-5 text-platinum-muted">
        Chain #{chainId}: its owner signs the anchor in the UltraNet mobile wallet. This dashboard
        holds no owner key and cannot sign for you.
        {hasTreasury ? " This chain also predates the ledger model; the node no longer anchors it." : ""}
      </p>
    </div>
  );
}
