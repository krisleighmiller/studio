import DOMPurify from "dompurify";
import { marked } from "marked";

marked.setOptions({
  gfm: true,
  breaks: true,
});

const MARKDOWN_GUARD = "workbench-markdown-guard";
if (!(DOMPurify as unknown as Record<string, boolean>)[MARKDOWN_GUARD]) {
  (DOMPurify as unknown as Record<string, boolean>)[MARKDOWN_GUARD] = true;
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName === "A") {
      node.setAttribute("rel", "noopener noreferrer");
      node.removeAttribute("target");
    }
    if (node.tagName === "IMG") {
      const src = node.getAttribute("src")?.trim() ?? "";
      if (/^(https?:)?\/\//i.test(src)) {
        node.removeAttribute("src");
      }
    }
  });
}

export function renderMarkdown(source: string): string {
  const html = marked.parse(source, { async: false }) as string;
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
  });
}

export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return lower.endsWith(".md") || lower.endsWith(".markdown");
}
