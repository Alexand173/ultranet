import type { Metadata } from "next";
import MyRewards from "@/components/games/MyRewards";

export const metadata: Metadata = { title: "My rewards | UltraNet Games" };

export default function MyRewardsPage() {
  return <MyRewards />;
}
