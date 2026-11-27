import type { Metadata } from "next";
import OwnerHub from "@/components/games/OwnerHub";

export const metadata: Metadata = { title: "My games | UltraNet Games" };

export default function ManageGamesPage() {
  return <OwnerHub />;
}
