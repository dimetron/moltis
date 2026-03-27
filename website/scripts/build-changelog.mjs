import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, "..");
const projectRoot = path.resolve(rootDir, "..");
const sourcePath = path.join(projectRoot, "CHANGELOG.md");
const outputDir = path.join(rootDir, "changelog");
const outputPath = path.join(outputDir, "index.html");

function escapeHtml(value) {
	return value
		.replaceAll("&", "&amp;")
		.replaceAll("<", "&lt;")
		.replaceAll(">", "&gt;")
		.replaceAll('"', "&quot;")
		.replaceAll("'", "&#39;");
}

function renderInline(raw) {
	let value = escapeHtml(raw);
	value = value.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
	value = value.replace(/`([^`]+)`/g, "<code>$1</code>");
	value = value.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
	value = value.replace(/\*([^*]+)\*/g, "<em>$1</em>");
	return value;
}

/** Turn a version heading like "[20260327.02] - 2026-03-27" into a URL-safe id. */
function versionSlug(text) {
	const match = text.match(/\[([^\]]+)\]/);
	return match ? match[1] : text.toLowerCase().replace(/[^a-z0-9]+/g, "-");
}

function renderMarkdown(markdown) {
	const lines = markdown.replace(/\r\n/g, "\n").split("\n");
	const html = [];
	let paragraph = [];
	let inList = false;

	const flushParagraph = () => {
		if (paragraph.length === 0) return;
		const text = paragraph.join(" ").trim();
		if (text) html.push(`<p>${renderInline(text)}</p>`);
		paragraph = [];
	};

	const closeList = () => {
		if (!inList) return;
		html.push("</ul>");
		inList = false;
	};

	for (const line of lines) {
		// Skip the document title ("# Changelog") — we render our own header.
		if (line.match(/^#\s+Changelog/i)) continue;
		// Skip the "keep a changelog" boilerplate paragraph.
		if (line.match(/^All notable changes/i)) continue;
		if (line.match(/^and this project adheres/i)) continue;

		const heading = line.match(/^(#{1,6})\s+(.+)$/);
		if (heading) {
			flushParagraph();
			closeList();
			const level = heading[1].length;
			const text = heading[2].trim();

			if (level === 2) {
				// Version headings get anchor ids and special styling.
				const slug = versionSlug(text);
				const display = renderInline(text.replace(/^\[([^\]]+)\]/, "$1"));
				html.push(`<h2 id="${escapeHtml(slug)}">${display}</h2>`);
			} else {
				// Category headings (### Added, ### Fixed, etc.) get a tag style.
				const category = text.replace(/^#+\s*/, "");
				const cls = category.toLowerCase();
				html.push(`<h${level} class="category ${escapeHtml(cls)}">${renderInline(text)}</h${level}>`);
			}
			continue;
		}

		const listItem = line.match(/^\s*-\s+(.+)$/);
		if (listItem) {
			flushParagraph();
			if (!inList) {
				html.push("<ul>");
				inList = true;
			}
			html.push(`<li>${renderInline(listItem[1].trim())}</li>`);
			continue;
		}

		if (line.trim() === "") {
			flushParagraph();
			closeList();
			continue;
		}

		paragraph.push(line.trim());
	}

	flushParagraph();
	closeList();
	return html.join("\n");
}

function buildHtml(contentHtml) {
	return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="color-scheme" content="light dark">
  <title>Changelog - Moltis</title>
  <meta name="description" content="Release history and changelog for Moltis.">
  <link rel="icon" type="image/svg+xml" href="/favicon.svg">
  <link rel="preconnect" href="https://fonts.googleapis.com">
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
  <link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;700&family=JetBrains+Mono:wght@400&display=swap" rel="stylesheet">
  <style>
    :root {
      --bg: #fff8f3;
      --text: #171717;
      --muted: #525252;
      --card: #ffffff;
      --border: #fed7aa;
      --accent: #ea580c;
      --accent-soft: #fff1e8;
      --code-bg: #fff3e0;
      --tag-added: #16a34a;
      --tag-added-bg: #dcfce7;
      --tag-fixed: #2563eb;
      --tag-fixed-bg: #dbeafe;
      --tag-changed: #d97706;
      --tag-changed-bg: #fef3c7;
      --tag-removed: #dc2626;
      --tag-removed-bg: #fee2e2;
      --tag-security: #7c3aed;
      --tag-security-bg: #ede9fe;
      --tag-deprecated: #78716c;
      --tag-deprecated-bg: #f5f5f4;
    }

    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #111111;
        --text: #fafafa;
        --muted: #d4d4d4;
        --card: #1a1a1a;
        --border: #44403c;
        --accent: #fb923c;
        --accent-soft: #2a1e16;
        --code-bg: #21170f;
        --tag-added: #4ade80;
        --tag-added-bg: #052e16;
        --tag-fixed: #60a5fa;
        --tag-fixed-bg: #172554;
        --tag-changed: #fbbf24;
        --tag-changed-bg: #422006;
        --tag-removed: #f87171;
        --tag-removed-bg: #450a0a;
        --tag-security: #a78bfa;
        --tag-security-bg: #2e1065;
        --tag-deprecated: #a8a29e;
        --tag-deprecated-bg: #292524;
      }
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      font-family: "Space Grotesk", system-ui, sans-serif;
      background: radial-gradient(circle at 10% 10%, #fed7aa33 0%, transparent 50%),
                  radial-gradient(circle at 90% 80%, #fb923c22 0%, transparent 45%),
                  var(--bg);
      color: var(--text);
      line-height: 1.65;
      padding: 2rem 1rem 3rem;
    }

    .shell {
      max-width: 860px;
      margin: 0 auto;
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: 16px;
      padding: 2rem 1.5rem;
      box-shadow: 0 8px 40px rgba(0, 0, 0, 0.08);
    }

    .top {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 1rem;
      margin-bottom: 1.25rem;
    }

    .badge {
      display: inline-flex;
      align-items: center;
      padding: 0.3rem 0.65rem;
      border-radius: 999px;
      background: var(--accent-soft);
      color: var(--accent);
      font-size: 0.75rem;
      font-weight: 700;
      letter-spacing: 0.05em;
      text-transform: uppercase;
    }

    .home-link {
      color: var(--accent);
      text-decoration: none;
      font-size: 0.9rem;
      font-weight: 600;
    }

    .home-link:hover { text-decoration: underline; }

    h1, h2, h3 {
      line-height: 1.25;
      margin-top: 1.6em;
      margin-bottom: 0.65em;
    }

    h1 {
      margin-top: 0.2em;
      font-size: clamp(2rem, 4.5vw, 2.8rem);
      letter-spacing: -0.02em;
    }

    h2 {
      font-size: clamp(1.3rem, 3vw, 1.7rem);
      border-top: 1px solid var(--border);
      padding-top: 1rem;
      scroll-margin-top: 1rem;
    }

    h2 a.anchor {
      text-decoration: none;
      color: inherit;
    }

    h2 a.anchor:hover { color: var(--accent); }

    h3.category {
      font-size: 0.85rem;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      margin-top: 1.2em;
      margin-bottom: 0.4em;
      padding: 0.2rem 0.6rem;
      border-radius: 6px;
      display: inline-block;
    }

    h3.added     { color: var(--tag-added);      background: var(--tag-added-bg); }
    h3.fixed     { color: var(--tag-fixed);       background: var(--tag-fixed-bg); }
    h3.changed   { color: var(--tag-changed);     background: var(--tag-changed-bg); }
    h3.removed   { color: var(--tag-removed);     background: var(--tag-removed-bg); }
    h3.security  { color: var(--tag-security);    background: var(--tag-security-bg); }
    h3.deprecated { color: var(--tag-deprecated); background: var(--tag-deprecated-bg); }

    p { margin: 0.9em 0; color: var(--muted); }

    ul {
      margin: 0.5em 0 1.2em;
      padding-left: 1.25rem;
    }

    li { margin: 0.35em 0; color: var(--muted); }

    li code, p code {
      font-family: "JetBrains Mono", monospace;
      background: var(--code-bg);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 0.1em 0.35em;
      font-size: 0.88em;
    }

    a { color: var(--accent); }
  </style>
</head>
<body>
  <main class="shell">
    <div class="top">
      <span class="badge">Changelog</span>
      <a class="home-link" href="/">Back to home</a>
    </div>
    <h1>Changelog</h1>
    ${contentHtml}
  </main>
</body>
</html>
`;
}

async function main() {
	const markdown = await readFile(sourcePath, "utf8");
	const contentHtml = renderMarkdown(markdown);
	const html = buildHtml(contentHtml);
	await mkdir(outputDir, { recursive: true });
	await writeFile(outputPath, html, "utf8");
	process.stdout.write(`Built changelog/index.html from CHANGELOG.md\n`);
}

main().catch((error) => {
	process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
	process.exit(1);
});
