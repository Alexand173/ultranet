import React from "react";
import { 
  Book, 
  Terminal, 
  Shield, 
  Cpu, 
  Activity, 
  Brain, 
  Lock, 
  Network, 
  Database, 
  Zap,
  Globe,
  Settings,
  HelpCircle,
  Code
} from "lucide-react";

export interface Chapter {
  id: number;
  title: string;
  category: string;
  slug?: string;
  iconName: string; // Storing as string to avoid React element serialization issues in some contexts if needed, but here we can just export a helper
}

export const CHAPTERS = [
  { id: 1, title: "Executive Summary", iconName: "Book", category: "Core" },
  { id: 2, title: "Protocol Philosophy & Design Goals", iconName: "Brain", category: "Core" },
  { id: 3, title: "System Architecture Overview", iconName: "Network", category: "Architecture" },
  { id: 4, title: "The UltraNet Mining Lifecycle", iconName: "Activity", category: "Mining" },
  { id: 5, title: "Block-STM: Parallel Execution", iconName: "Cpu", category: "Execution", slug: "real-time-finality" },
  { id: 6, title: "Consensus: Mysticeti DAG & Bullshark", iconName: "Zap", category: "Consensus", slug: "layer-0-foundation" },
  { id: 7, title: "Cryptographic Foundations", iconName: "Lock", category: "Crypto" },
  { id: 8, title: "Privacy Layer: Zero-Knowledge Proofs", iconName: "Shield", category: "Crypto", slug: "trusted-execution" },
  { id: 9, title: "Fully Homomorphic Encryption (FHE)", iconName: "Lock", category: "Crypto", slug: "trusted-execution" },
  { id: 10, title: "Recursive SNARKs & Proof Chaining", iconName: "Cpu", category: "Execution", slug: "unified-scalability" },
  { id: 11, title: "STARK Engine & PQ Verifiability", iconName: "Shield", category: "Crypto" },
  { id: 12, title: "State Management: Sharded MPT", iconName: "Database", category: "State" },
  { id: 13, title: "Cross-Shard Messaging & Atomicity", iconName: "Network", category: "Architecture", slug: "unified-scalability" },
  { id: 14, title: "The Move Virtual Machine", iconName: "Code", category: "Execution" },
  { id: 15, title: "Networking Layer (P2P / libp2p)", iconName: "Globe", category: "Architecture" },
  { id: 16, title: "Sovereign Governance", iconName: "Shield", category: "Governance" },
  { id: 17, title: "AI Governor: Difficulty Tuning", iconName: "Brain", category: "Governance", slug: "autonomous-intelligence" },
  { id: 18, title: "AppChains: Layer-3 Sub-Networks", iconName: "Network", category: "Architecture" },
  { id: 19, title: "Dashboard Interface Reference", iconName: "Terminal", category: "Interface" },
  { id: 20, title: "REST API Reference", iconName: "Code", category: "Interface" },
  { id: 21, title: "Tokenomics & Economic Model", iconName: "Activity", category: "Economics" },
  { id: 22, title: "How the Public Earns $ULTRA", iconName: "Zap", category: "Economics" },
  { id: 23, title: "Validator Onboarding", iconName: "Settings", category: "Operations" },
  { id: 24, title: "Node Operations & Maintenance", iconName: "Settings", category: "Operations" },
  { id: 25, title: "Security Model & Threat Analysis", iconName: "Shield", category: "Core" },
  { id: 26, title: "Disaster Recovery", iconName: "HelpCircle", category: "Operations" },
  { id: 27, title: "Performance Benchmarks", iconName: "Activity", category: "Operations" },
  { id: 28, title: "Troubleshooting Guide", iconName: "HelpCircle", category: "Operations" },
  { id: 29, title: "Glossary of Terms", iconName: "Book", category: "Core" },
  { id: 30, title: "Frequently Asked Questions", iconName: "HelpCircle", category: "Core" },
  { id: 31, title: "Appendix A: Constants", iconName: "Code", category: "Technical" },
  { id: 32, title: "Appendix B: Data Structures", iconName: "Code", category: "Technical" },
  { id: 33, title: "Roadmap", iconName: "Activity", category: "Core" },
  { id: 34, title: "Official Seal & Attestation", iconName: "Shield", category: "Core" }
];

export const getIcon = (name: string) => {
  switch (name) {
    case "Book": return <Book className="w-4 h-4" />;
    case "Terminal": return <Terminal className="w-4 h-4" />;
    case "Shield": return <Shield className="w-4 h-4" />;
    case "Cpu": return <Cpu className="w-4 h-4" />;
    case "Activity": return <Activity className="w-4 h-4" />;
    case "Brain": return <Brain className="w-4 h-4" />;
    case "Lock": return <Lock className="w-4 h-4" />;
    case "Network": return <Network className="w-4 h-4" />;
    case "Database": return <Database className="w-4 h-4" />;
    case "Zap": return <Zap className="w-4 h-4" />;
    case "Globe": return <Globe className="w-4 h-4" />;
    case "Settings": return <Settings className="w-4 h-4" />;
    case "HelpCircle": return <HelpCircle className="w-4 h-4" />;
    case "Code": return <Code className="w-4 h-4" />;
    default: return <Book className="w-4 h-4" />;
  }
};
