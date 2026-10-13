import { isValidElement, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

function textContent(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textContent).join("");
  if (isValidElement<{ children?: ReactNode }>(node)) return textContent(node.props.children);
  return "";
}

function codeLanguage(node: ReactNode): string {
  if (!isValidElement<{ className?: string }>(node)) return "";
  return /(?:^|\s)language-([^\s]+)/.exec(node.props.className ?? "")?.[1] ?? "";
}

const components: Components = {
  h1: ({ node: _node, ...props }) => <h2 className="conversation-heading conversation-heading-1" {...props} />,
  h2: ({ node: _node, ...props }) => <h3 className="conversation-heading conversation-heading-2" {...props} />,
  h3: ({ node: _node, ...props }) => <h4 className="conversation-heading conversation-heading-3" {...props} />,
  h4: ({ node: _node, ...props }) => <h5 className="conversation-heading conversation-heading-4" {...props} />,
  h5: ({ node: _node, ...props }) => <h6 className="conversation-heading conversation-heading-5" {...props} />,
  h6: ({ node: _node, ...props }) => <h6 className="conversation-heading conversation-heading-6" {...props} />,
  ul: ({ node: _node, ...props }) => <ul className="conversation-list" {...props} />,
  ol: ({ node: _node, ...props }) => <ol className="conversation-list" {...props} />,
  table: ({ node: _node, children, ...props }) => <div className="conversation-table-scroll" tabIndex={0} role="region" aria-label="Markdown 表格"><table {...props}>{children}</table></div>,
  pre: ({ node: _node, children }) => {
    const language = codeLanguage(children);
    const code = textContent(children).replace(/\n$/, "");
    return <div className="conversation-code-card"><div className="conversation-code-toolbar"><span><span className="conversation-code-icon" aria-hidden="true">〈/〉</span>{language || "纯文本"}</span><button className="conversation-copy" type="button" title="复制代码" aria-label="复制代码" onClick={() => { if (navigator.clipboard) void navigator.clipboard.writeText(code).catch(() => undefined); }}>▢</button></div><pre className="conversation-code">{children}</pre></div>;
  },
  a: ({ node: _node, href, children, ...props }) => href ? <a {...props} href={href} target="_blank" rel="noreferrer">{children}</a> : <span className="conversation-unsafe-link">{children}</span>,
};

export function MarkdownContent({ text }: { text: string }) {
  return <div className="conversation-markdown"><ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml components={components}>{text}</ReactMarkdown></div>;
}
