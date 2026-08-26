import { redirect } from "next/navigation";
import { SEND_ULTRA_PATH } from "@/lib/links";

export default function TransactPage() {
  redirect(SEND_ULTRA_PATH);
}
