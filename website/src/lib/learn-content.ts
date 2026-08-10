export type LearnSlug =
  | "what-is-ultranet"
  | "how-it-works"
  | "use-cases"
  | "validators"
  | "network";

export interface LearnSection {
  id: string;
  title: string;
  paragraphs: string[];
  bullets?: string[];
  callout?: string;
}

export interface LearnArticle {
  slug: LearnSlug;
  eyebrow: string;
  title: string;
  intro: string;
  readTime: string;
  sections: LearnSection[];
}

export interface LearningTrack {
  slug: LearnSlug;
  title: string;
  description: string;
  icon: "Globe" | "Workflow" | "Blocks" | "ShieldCheck" | "Activity";
}

export const LEARNING_TRACKS: LearningTrack[] = [
  {
    slug: "what-is-ultranet",
    title: "What is UltraNet?",
    description: "A plain-language introduction to the network, $ULTRA, and shared state.",
    icon: "Globe",
  },
  {
    slug: "how-it-works",
    title: "How UltraNet works",
    description: "Follow a transaction from your wallet through consensus, execution, and finality.",
    icon: "Workflow",
  },
  {
    slug: "use-cases",
    title: "What can it be used for?",
    description: "Explore payments, private applications, appchains, and verifiable digital ownership.",
    icon: "Blocks",
  },
  {
    slug: "validators",
    title: "Become a validator",
    description: "Learn what a validator does, what equipment it needs, and how joining actually works.",
    icon: "ShieldCheck",
  },
  {
    slug: "network",
    title: "Read the live network",
    description: "Understand block height, validators, transactions, shards, and what the live node can prove.",
    icon: "Activity",
  },
];

export const LEARN_ARTICLES: Record<LearnSlug, LearnArticle> = {
  "what-is-ultranet": {
    slug: "what-is-ultranet",
    eyebrow: "Start here",
    title: "What is UltraNet?",
    intro: "UltraNet is a shared network for recording and verifying digital actions without asking one company to keep the only copy of the truth.",
    readTime: "4 min read",
    sections: [
      {
        id: "shared-network",
        title: "A shared network, not a single database",
        paragraphs: [
          "When you use a normal app, one company usually decides which records are valid. UltraNet distributes that responsibility across nodes that independently verify the same history.",
          "That makes the ledger useful for payments, ownership records, and applications that need a common source of truth across organisations.",
        ],
        callout: "Think of UltraNet as a public record that many independent computers check together.",
      },
      {
        id: "what-is-recorded",
        title: "What gets recorded",
        paragraphs: [
          "Transactions describe a requested change, such as moving $ULTRA, calling a Move module, or submitting a validator proposal. Nodes check the signature, rules, and available state before accepting it.",
          "Accepted transactions become part of blocks. Blocks form the history that new nodes can verify from the network's published state.",
        ],
        bullets: ["Wallet actions and payments", "Move smart-contract calls", "Validator governance proposals", "Appchain state commitments"],
      },
      {
        id: "ultra-token",
        title: "What $ULTRA is for",
        paragraphs: [
          "$ULTRA is the native network asset used by the protocol's transaction and validator economy. The exact balance, reward, and supply values are determined by the node and genesis configuration, not by this page.",
          "This learning area intentionally avoids inventing prices, yields, or balances. When a number is live, it is labelled as live and sourced from the connected node.",
        ],
      },
    ],
  },
  "how-it-works": {
    slug: "how-it-works",
    eyebrow: "The journey of a transaction",
    title: "How UltraNet works",
    intro: "A transaction moves through several checks before it becomes part of the durable ledger: signing, gossip, ordering, execution, and verification.",
    readTime: "6 min read",
    sections: [
      {
        id: "sign-and-send",
        title: "1. Sign and send",
        paragraphs: [
          "Your wallet creates the transaction and signs it locally. The node receives only the public transaction fields and signature needed to verify the request.",
          "For validator proposals, UltraWallet also binds the applicant's public key and metadata to the signing envelope.",
        ],
      },
      {
        id: "gossip-and-consensus",
        title: "2. Gossip and consensus",
        paragraphs: [
          "The peer-to-peer layer shares valid transactions and block information with other nodes. UltraNet's DAG-oriented consensus records what validators observed and establishes an agreed order.",
          "Consensus is not the same as execution: it answers which history the network is accepting, while execution checks what that history does to state.",
        ],
      },
      {
        id: "parallel-execution",
        title: "3. Execute and update state",
        paragraphs: [
          "The Block-STM execution engine can process independent work in parallel, while conflict checks preserve deterministic results. Sharded state storage keeps unrelated parts of the ledger from contending for the same path.",
          "Move modules run with resource-oriented rules. A transaction is accepted only when its execution and resulting state transition are valid.",
        ],
        bullets: ["Optimistic parallel execution", "16 logical state shards in the current protocol configuration", "Deterministic state-root verification", "Move resource safety"],
      },
      {
        id: "finality",
        title: "4. Verify finality",
        paragraphs: [
          "Validators re-check the resulting state and cryptographic proofs. Once the network commits the block, later nodes can use the recorded roots and signatures to verify the same transition.",
          "The live dashboard reports only measurements exposed by the node. It does not substitute a generated animation for finality or proving time.",
        ],
      },
    ],
  },
  "use-cases": {
    slug: "use-cases",
    eyebrow: "Why build on it",
    title: "What can UltraNet be used for?",
    intro: "UltraNet is infrastructure. The best use cases are the ones that benefit from shared verification, programmable ownership, or a durable cross-organisation record.",
    readTime: "5 min read",
    sections: [
      {
        id: "payments",
        title: "Payments and settlement",
        paragraphs: [
          "Applications can use the ledger to move digital value and verify settlement without relying on a private database maintained by one intermediary.",
          "Real-world payment design still needs compliant wallets, user protection, and clear recovery paths. A blockchain does not remove those product responsibilities.",
        ],
      },
      {
        id: "private-applications",
        title: "Private and verifiable applications",
        paragraphs: [
          "Zero-knowledge and FHE-related components in UltraNet are designed to support stronger privacy and verifiability. Teams should validate the exact circuit, proof, and performance guarantees for their application before treating a feature as production-ready.",
          "The safe promise is verifiable computation with explicit cryptographic boundaries—not a claim that every piece of application data is automatically private.",
        ],
      },
      {
        id: "appchains",
        title: "Dedicated appchains",
        paragraphs: [
          "An organisation can use an appchain to separate its application workload while anchoring state commitments to the UltraNet ecosystem. This can make operational boundaries clearer without abandoning shared verification.",
          "Appchain creation and anchoring remain operator-controlled actions on the node API and should never be represented as automatic or permissionless in user documentation.",
        ],
      },
      {
        id: "ownership",
        title: "Digital ownership and records",
        paragraphs: [
          "Resource-oriented Move contracts can represent assets, permissions, and ownership rules. This is useful when multiple parties need to inspect the same state transitions and no single party should be able to rewrite the history unilaterally.",
        ],
      },
    ],
  },
  validators: {
    slug: "validators",
    eyebrow: "Operate the network",
    title: "What does a validator do?",
    intro: "A validator is an infrastructure operator that helps receive, verify, execute, and commit network activity. UltraNet uses validator—not miner—as the ordinary term for this role.",
    readTime: "7 min read",
    sections: [
      {
        id: "validator-role",
        title: "The validator's job",
        paragraphs: [
          "Validators run the UltraNet node software, maintain a synchronized copy of the network state, participate in peer-to-peer communication, and verify proposed transitions.",
          "They are not simply servers that display a dashboard. A healthy validator must be able to validate cryptographic signatures, execute the supported transaction rules, and keep its state consistent with the network.",
        ],
        bullets: ["Receive and gossip network data", "Verify signatures and transaction rules", "Execute transactions and compare state roots", "Participate in the consensus and governance lifecycle"],
      },
      {
        id: "hardware",
        title: "What a validator needs",
        paragraphs: [
          "The current onboarding guide recommends Linux, at least eight physical CPU cores, 32 GB memory, 1 TB NVMe storage, and low-latency 1 Gbps connectivity. These are operational targets from the repository guide, not a promise that every workload has identical requirements.",
          "Operators also need secure key handling, persistent storage, monitoring, firewall rules, and a TLS reverse proxy for the private API.",
        ],
      },
      {
        id: "join-flow",
        title: "How joining works",
        paragraphs: [
          "An applicant creates a Dilithium public key and submits a proposal through UltraWallet. The website never creates an unsigned fallback proposal and never receives the private key.",
          "The proposal enters the node's governance queue. Activation requires the documented sovereign approval flow; submitting a form is not the same as becoming an active validator.",
        ],
        callout: "Validator registration is a signed governance process—not a browser-only toggle.",
      },
      {
        id: "operator-safety",
        title: "Operate responsibly",
        paragraphs: [
          "Keep sovereign keys offline, restrict administrative API routes, monitor state roots and peer health, and practice recovery before handling production value. A validator should fail safely rather than report fabricated health when its node is unavailable.",
        ],
      },
    ],
  },
  network: {
    slug: "network",
    eyebrow: "Read the source of truth",
    title: "How to read the live network",
    intro: "The network page explains what each live number means and makes a clear distinction between a node measurement, a configured limit, and a value that the API does not currently expose.",
    readTime: "4 min read",
    sections: [
      {
        id: "block-height",
        title: "Block height",
        paragraphs: [
          "Block height is the node's current count of committed blocks. It is a progress marker for the local chain, not a measure of how many users or transactions the network has.",
        ],
      },
      {
        id: "validators-and-weight",
        title: "Validators and total weight",
        paragraphs: [
          "Validator count reports the active validator records known to the node. Total weight is the sum exposed by the node's validator set. Neither number should be replaced by a marketing estimate when the API is unavailable.",
        ],
      },
      {
        id: "throughput",
        title: "TPS and proving time",
        paragraphs: [
          "Transactions per second and proving time require an actual measurement source. If the connected node does not expose a current value, UltraNet displays “Unavailable” rather than generating a random sample.",
          "This protects readers from confusing a protocol target, a benchmark, and a current production observation.",
        ],
      },
      {
        id: "activity",
        title: "Ledger activity",
        paragraphs: [
          "The live ledger stream is populated from the node's latest transaction endpoint. An empty stream means the node has no recent transactions to return; it does not mean that example rows should be inserted.",
        ],
      },
    ],
  },
};

export function getLearnArticle(slug: string): LearnArticle | null {
  return slug in LEARN_ARTICLES ? LEARN_ARTICLES[slug as LearnSlug] : null;
}

export function isLearnSlug(value: string): value is LearnSlug {
  return value in LEARN_ARTICLES;
}
