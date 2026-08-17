import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { marked } from "marked";

const scriptsDirectory = path.dirname(fileURLToPath(import.meta.url));
const sourcePath = path.resolve(scriptsDirectory, "../../ULTRA_NET_TECHNICAL_GUIDE.md");
const outputPath = path.resolve(scriptsDirectory, "../public/docs/ultranet-whitepaper.html");
const mermaidVersion = "11.12.1";

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function stripMarkup(value) {
  return value
    .replace(/<[^>]*>/g, "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[`*_~>#]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function createHeadingId(text, headingIds) {
  const baseId = stripMarkup(text)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "section";
  const count = headingIds.get(baseId) ?? 0;
  headingIds.set(baseId, count + 1);
  return count === 0 ? baseId : `${baseId}-${count + 1}`;
}

function createRenderer() {
  const renderer = new marked.Renderer();
  const headingIds = new Map();
  const defaultTable = renderer.table.bind(renderer);

  renderer.heading = function heading({ tokens, depth }) {
    const text = this.parser.parseInline(tokens);
    const id = createHeadingId(text, headingIds);
    return `<h${depth} id="${escapeHtml(id)}">${text}</h${depth}>\n`;
  };

  renderer.code = function code({ text, lang }) {
    if (lang?.trim().toLowerCase() === "mermaid") {
      return `<figure class="diagram-block" aria-labelledby="diagram-caption-${headingIds.size}">
  <div class="mermaid" role="img" aria-label="Mermaid architecture diagram">${escapeHtml(text)}</div>
  <figcaption id="diagram-caption-${headingIds.size}">Architecture diagram</figcaption>
  <details class="diagram-source">
    <summary>View diagram source</summary>
    <pre><code>${escapeHtml(text)}</code></pre>
  </details>
</figure>\n`;
    }

    const languageClass = lang?.trim()
      ? ` class="language-${escapeHtml(lang.trim())}"`
      : "";
    return `<pre><code${languageClass}>${escapeHtml(text)}</code></pre>\n`;
  };

  renderer.table = function table(token) {
    return `<div class="table-scroll">${defaultTable(token)}</div>\n`;
  };

  return renderer;
}

function documentTemplate(renderedMarkdown) {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="author" content="Vladan Jotov">
    <meta name="description" content="The canonical UltraNet v7.1 sovereign technical guide.">
    <title>UltraNet v7.1 Sovereign Technical Guide</title>
    <style>
      :root {
        color-scheme: light;
        --paper: #ffffff;
        --paper-muted: #f4f7f8;
        --ink: #12202a;
        --ink-muted: #52636e;
        --cyan: #007f91;
        --cyan-soft: #e3f7fa;
        --line: #d9e2e6;
        --code: #111c24;
      }

      * { box-sizing: border-box; }
      html { scroll-behavior: smooth; overflow-x: hidden; }
      body {
        margin: 0;
        overflow-x: hidden;
        background: #dfe5e8;
        color: var(--ink);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
        line-height: 1.7;
      }

      .document-shell {
        min-height: 100vh;
        max-width: 100%;
        margin: 0 auto;
        overflow-x: hidden;
        background: var(--paper);
        box-shadow: 0 0 2rem rgba(18, 32, 42, 0.16);
      }

      .document-cover {
        padding: clamp(1.5rem, 5vw, 4.5rem) clamp(1.25rem, 6vw, 5.5rem) 2rem;
        border-bottom: 1px solid var(--line);
        background: linear-gradient(135deg, var(--paper-muted), var(--paper));
      }

      .eyebrow {
        margin: 0 0 0.75rem;
        color: var(--cyan);
        font: 700 0.72rem/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
        letter-spacing: 0.18em;
        text-transform: uppercase;
      }

      .cover-meta {
        margin: 0;
        color: var(--ink-muted);
        font: 0.78rem/1.6 ui-monospace, SFMono-Regular, Menlo, monospace;
        letter-spacing: 0.04em;
      }

      .source-notice {
        max-width: 52rem;
        margin: 1.5rem 0 0;
        color: var(--ink-muted);
        font-size: 0.85rem;
      }

      .document-body {
        max-width: 72rem;
        min-width: 0;
        margin: 0 auto;
        padding: clamp(1.25rem, 5vw, 4.5rem) clamp(1.25rem, 6vw, 5.5rem) 5rem;
        overflow-wrap: anywhere;
      }

      .document-body > :first-child { margin-top: 0; }
      .document-body p,
      .document-body li,
      .document-body h1,
      .document-body h2,
      .document-body h3,
      .document-body h4 {
        min-width: 0;
        overflow-wrap: anywhere;
      }

      .document-body h1,
      .document-body h2,
      .document-body h3,
      .document-body h4 {
        scroll-margin-top: 1.5rem;
        color: #08141c;
        line-height: 1.18;
      }

      .document-body h1 {
        margin: 0 0 1.5rem;
        font-size: clamp(2rem, 5vw, 3.4rem);
        letter-spacing: -0.04em;
      }

      .document-body h2 {
        margin: 3.25rem 0 1rem;
        padding-bottom: 0.5rem;
        border-bottom: 1px solid var(--line);
        font-size: clamp(1.45rem, 3vw, 2.1rem);
        letter-spacing: -0.025em;
      }

      .document-body h3 {
        margin: 2rem 0 0.75rem;
        font-size: clamp(1.15rem, 2vw, 1.45rem);
      }

      .document-body h4 { margin: 1.5rem 0 0.5rem; font-size: 1.05rem; }
      .document-body p, .document-body ul, .document-body ol { margin: 0 0 1.1rem; }
      .document-body ul, .document-body ol { padding-left: 1.4rem; }
      .document-body li + li { margin-top: 0.35rem; }
      .document-body a { color: var(--cyan); text-decoration: underline; text-underline-offset: 0.18em; }
      .document-body a:focus-visible,
      .diagram-source summary:focus-visible { outline: 3px solid #00a9bd; outline-offset: 3px; }
      .document-body strong { color: #08141c; }
      .document-body blockquote {
        margin: 1.5rem 0;
        padding: 1rem 1.25rem;
        border-left: 4px solid var(--cyan);
        background: var(--cyan-soft);
        color: #29414d;
      }

      .document-body hr { margin: 2.5rem 0; border: 0; border-top: 1px solid var(--line); }
      .document-body img { max-width: 100%; height: auto; }
      .document-body code {
        padding: 0.12rem 0.3rem;
        border-radius: 0.2rem;
        background: #edf2f4;
        color: #173d4a;
        font: 0.9em/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
      }

      .document-body pre {
        max-width: 100%;
        margin: 1.25rem 0 1.5rem;
        overflow-x: auto;
        padding: 1rem 1.15rem;
        border-radius: 0.25rem;
        background: var(--code);
        color: #e7f7fa;
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
      }

      .document-body pre code {
        padding: 0;
        background: transparent;
        color: inherit;
        white-space: pre;
      }

      .table-scroll { max-width: 100%; margin: 1.25rem 0 1.5rem; overflow-x: auto; }
      .document-body table { width: 100%; min-width: 38rem; border-collapse: collapse; font-size: 0.92rem; }
      .document-body th, .document-body td { padding: 0.7rem 0.8rem; border: 1px solid var(--line); text-align: left; vertical-align: top; }
      .document-body th { background: var(--paper-muted); color: #08141c; font-weight: 700; }
      .document-body tr:nth-child(even) td { background: #fbfcfc; }

      .diagram-block {
        margin: 1.75rem 0 2rem;
        padding: 1rem;
        border: 1px solid #b9dce2;
        background: #f6fbfc;
      }

      .mermaid {
        min-height: 3rem;
        overflow-x: auto;
        padding: 1rem;
        text-align: center;
        white-space: pre;
        color: #29414d;
        font: 0.8rem/1.5 ui-monospace, SFMono-Regular, Menlo, monospace;
      }

      .mermaid svg { max-width: 100%; height: auto; }
      .diagram-block figcaption { margin-top: 0.75rem; color: var(--ink-muted); font-size: 0.78rem; text-align: center; }
      .diagram-source { margin-top: 0.8rem; color: var(--ink-muted); font-size: 0.8rem; }
      .diagram-source summary { cursor: pointer; color: var(--cyan); font-weight: 700; }
      .diagram-source pre { margin-bottom: 0; }

      @media (max-width: 42rem) {
        .document-body table { min-width: 32rem; }
        .document-body pre { font-size: 0.78rem; }
      }

      @media print {
        body { background: #fff; }
        .document-shell { max-width: none; box-shadow: none; }
        .document-cover, .document-body { padding-left: 0; padding-right: 0; }
        .diagram-source { display: none; }
        a { color: inherit; text-decoration: none; }
      }
    </style>
  </head>
  <body>
    <main class="document-shell">
      <header class="document-cover">
        <p class="eyebrow">UltraNet // Sovereign Technical Guide</p>
        <p class="cover-meta">DOCUMENT_VERSION // 7.1_SOVEREIGN &nbsp;|&nbsp; 34_CHAPTERS &nbsp;|&nbsp; PUBLIC / EDUCATIONAL</p>
        <p class="source-notice">Copyright (c) 2026 Vladan Jotov. This documentation is licensed under the ISC License. Third-party materials remain under their respective licenses.</p>
      </header>
      <article class="document-body" aria-label="UltraNet sovereign technical guide">
        ${renderedMarkdown}
      </article>
    </main>
    <noscript><p style="max-width:72rem;margin:1rem auto;padding:0 1.25rem;color:#12202a">JavaScript is disabled. Mermaid diagrams remain available as source below each diagram.</p></noscript>
    <script type="module">
      const diagrams = document.querySelectorAll(".mermaid");
      if (diagrams.length > 0) {
        try {
          const { default: mermaid } = await import("https://cdn.jsdelivr.net/npm/mermaid@${mermaidVersion}/dist/mermaid.esm.min.mjs");
          mermaid.initialize({
            startOnLoad: false,
            securityLevel: "strict",
            theme: "base",
            themeVariables: {
              primaryColor: "#e3f7fa",
              primaryTextColor: "#12202a",
              primaryBorderColor: "#007f91",
              lineColor: "#52636e",
              secondaryColor: "#f4f7f8",
              tertiaryColor: "#ffffff"
            }
          });
          await mermaid.run({ nodes: [...diagrams] });
        } catch {
          document.documentElement.dataset.diagramFallback = "true";
        }
      }
    </script>
  </body>
</html>
`;
}

const source = await readFile(sourcePath, "utf8");
const renderedMarkdown = marked.parse(source, {
  gfm: true,
  breaks: false,
  renderer: createRenderer(),
});
const output = documentTemplate(renderedMarkdown);
const temporaryOutputPath = `${outputPath}.tmp-${process.pid}`;

await mkdir(path.dirname(outputPath), { recursive: true });
await writeFile(temporaryOutputPath, output, "utf8");
await rename(temporaryOutputPath, outputPath);

console.log(`Generated ${path.relative(process.cwd(), outputPath)} from ${path.relative(process.cwd(), sourcePath)}`);
