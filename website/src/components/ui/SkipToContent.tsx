/**
 * First focusable element on every page.
 *
 * Nine navigation links preceded the content on every route, so a keyboard or
 * screen-reader user had to tab through all of them before reaching the page
 * body. Hidden until focused, so the visual design is unaffected.
 *
 * The target is `#main-content`, carried by the single <main id="main-content" tabIndex={-1}> each route
 * renders; globals.css gives it scroll-margin-top so the fixed 80px nav does
 * not cover the first heading after the jump.
 */
export default function SkipToContent() {
  return (
    <a
      href="#main-content"
      className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[110] focus:inline-flex focus:min-h-11 focus:items-center focus:border focus:border-cyan-glow focus:bg-ink-black focus:px-5 focus:font-mono focus:text-xs focus:font-bold focus:uppercase focus:tracking-[0.16em] focus:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
    >
      Skip to content
    </a>
  );
}
