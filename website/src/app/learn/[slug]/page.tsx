import { notFound } from "next/navigation";
import LearnArticle from "@/components/learn/LearnArticle";
import { getLearnArticle, LEARNING_TRACKS, type LearnSlug } from "@/lib/learn-content";

export function generateStaticParams() {
  return LEARNING_TRACKS.map((track) => ({ slug: track.slug }));
}

export default async function LearnArticlePage({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const article = getLearnArticle(slug);
  if (!article) notFound();

  return <LearnArticle article={article} />;
}
