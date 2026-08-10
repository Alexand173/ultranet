import {
  Activity,
  Blocks,
  Globe2,
  ShieldCheck,
  Workflow,
} from "lucide-react";
import type { LearningTrack } from "@/lib/learn-content";

const ICONS = {
  Activity,
  Blocks,
  Globe: Globe2,
  ShieldCheck,
  Workflow,
} as const;

export default function LearnIcon({ name, className = "h-6 w-6" }: { name: LearningTrack["icon"]; className?: string }) {
  const Icon = ICONS[name];
  return <Icon className={className} aria-hidden="true" />;
}
