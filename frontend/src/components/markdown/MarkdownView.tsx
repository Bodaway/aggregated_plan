import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface MarkdownViewProps {
  readonly value: string;
  readonly emptyText?: string;
  readonly className?: string;
}

/**
 * Renders a markdown string with project-consistent styling.
 *
 * Uses GitHub-flavored markdown (tables, task lists, autolinks). Heading/list/code
 * styling is hand-tuned with the same Tailwind classes the rest of the app uses,
 * so we don't need the @tailwindcss/typography plugin.
 */
export function MarkdownView({ value, emptyText = 'No notes yet', className = '' }: MarkdownViewProps) {
  if (!value || value.trim().length === 0) {
    return <p className={`text-sm text-gray-400 italic ${className}`}>{emptyText}</p>;
  }

  return (
    <div className={`text-sm text-gray-700 leading-relaxed ${className}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ children }) => <h1 className="text-base font-semibold text-gray-900 mt-3 mb-1.5">{children}</h1>,
          h2: ({ children }) => <h2 className="text-sm font-semibold text-gray-900 mt-3 mb-1.5">{children}</h2>,
          h3: ({ children }) => <h3 className="text-sm font-semibold text-gray-800 mt-2 mb-1">{children}</h3>,
          p: ({ children }) => <p className="my-1.5">{children}</p>,
          ul: ({ children }) => <ul className="list-disc list-inside my-1.5 space-y-0.5">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal list-inside my-1.5 space-y-0.5">{children}</ol>,
          li: ({ children }) => <li className="ml-1">{children}</li>,
          a: ({ href, children }) => (
            <a
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-blue-600 hover:text-blue-800 underline"
            >
              {children}
            </a>
          ),
          code: ({ children, className: cls }) => {
            const isBlock = cls?.includes('language-');
            if (isBlock) {
              return (
                <code className="block bg-gray-50 border border-gray-200 rounded px-2 py-1.5 my-1.5 font-mono text-xs overflow-x-auto">
                  {children}
                </code>
              );
            }
            return <code className="bg-gray-100 rounded px-1 py-0.5 font-mono text-xs">{children}</code>;
          },
          pre: ({ children }) => <pre className="my-1.5">{children}</pre>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-gray-300 pl-3 my-1.5 text-gray-600 italic">
              {children}
            </blockquote>
          ),
          table: ({ children }) => (
            <table className="my-2 border-collapse text-xs">{children}</table>
          ),
          th: ({ children }) => (
            <th className="border border-gray-300 px-2 py-1 bg-gray-50 font-medium text-left">{children}</th>
          ),
          td: ({ children }) => <td className="border border-gray-300 px-2 py-1">{children}</td>,
          input: ({ checked, type }) => {
            if (type === 'checkbox') {
              return (
                <input
                  type="checkbox"
                  checked={checked ?? false}
                  readOnly
                  className="mr-1.5 align-middle accent-blue-600"
                />
              );
            }
            return null;
          },
          hr: () => <hr className="my-3 border-gray-200" />,
        }}
      >
        {value}
      </ReactMarkdown>
    </div>
  );
}
