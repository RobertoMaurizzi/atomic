import { useState, Fragment, ReactNode } from 'react';
import { ChatMessageWithContext, ChatCitation } from '../../stores/chat';
import { CitationLink, CitationPopover } from '../wiki';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface ChatMessageProps {
  message: ChatMessageWithContext;
  isStreaming?: boolean;
  onViewAtom?: (atomId: string) => void;
}

export function ChatMessage({ message, isStreaming = false, onViewAtom }: ChatMessageProps) {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';

  const [activeCitation, setActiveCitation] = useState<ChatCitation | null>(null);
  const [anchorRect, setAnchorRect] = useState<{ top: number; left: number; bottom: number; width: number } | null>(null);

  // Create a map of citation index to citation object
  const citationMap = new Map(
    (message.citations || []).map((c) => [c.citation_index, c])
  );

  const handleCitationClick = (citation: ChatCitation, element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setActiveCitation(citation);
    setAnchorRect({ top: rect.top, left: rect.left, bottom: rect.bottom, width: rect.width });
  };

  const handleClosePopover = () => {
    setActiveCitation(null);
    setAnchorRect(null);
  };

  const handleViewAtom = (atomId: string) => {
    if (onViewAtom) {
      onViewAtom(atomId);
    }
    handleClosePopover();
  };

  // Process text to replace [N] patterns with CitationLink components
  const processTextWithCitations = (text: string): ReactNode[] => {
    const parts = text.split(/(\[\d+\])/g);
    return parts.map((part, i) => {
      const match = part.match(/\[(\d+)\]/);
      if (match) {
        const index = parseInt(match[1], 10);
        const citation = citationMap.get(index);
        if (citation) {
          return (
            <CitationLink
              key={`citation-${i}-${index}`}
              index={index}
              onClick={(e) => handleCitationClick(citation, e.currentTarget)}
            />
          );
        }
      }
      return <Fragment key={`text-${i}`}>{part}</Fragment>;
    });
  };

  // Process children recursively to handle citations in all text nodes
  const processChildren = (children: ReactNode): ReactNode => {
    if (typeof children === 'string') {
      return processTextWithCitations(children);
    }
    if (Array.isArray(children)) {
      return children.map((child, i) => (
        <Fragment key={i}>{processChildren(child)}</Fragment>
      ));
    }
    return children;
  };

  // Custom components for react-markdown with citation processing
  const markdownComponents = {
    p: ({ children }: { children?: ReactNode }) => (
      <p>{processChildren(children)}</p>
    ),
    li: ({ children }: { children?: ReactNode }) => (
      <li>{processChildren(children)}</li>
    ),
    td: ({ children }: { children?: ReactNode }) => (
      <td>{processChildren(children)}</td>
    ),
    th: ({ children }: { children?: ReactNode }) => (
      <th>{processChildren(children)}</th>
    ),
    strong: ({ children }: { children?: ReactNode }) => (
      <strong>{processChildren(children)}</strong>
    ),
    em: ({ children }: { children?: ReactNode }) => (
      <em>{processChildren(children)}</em>
    ),
    // Style links
    a: ({ children, href }: { children?: ReactNode; href?: string }) => (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="text-[#a78bfa] hover:underline"
      >
        {children}
      </a>
    ),
    // Style code blocks
    code: ({ className, children }: { className?: string; children?: ReactNode }) => {
      const isInline = !className;
      return isInline ? (
        <code className="px-1 py-0.5 bg-[#1e1e1e] rounded text-[#e5c07b]">
          {children}
        </code>
      ) : (
        <code className={className}>{children}</code>
      );
    },
  };

  return (
    <>
      <div className={`flex ${isUser ? 'justify-end' : 'justify-start'}`}>
        <div
          className={`max-w-[85%] rounded-lg px-4 py-3 ${
            isUser
              ? 'bg-[#7c3aed] text-white'
              : 'bg-[#2d2d2d] text-[#dcddde]'
          }`}
        >
          {/* Message content */}
          {isAssistant ? (
            <div className="prose prose-invert prose-sm max-w-none">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={markdownComponents}
              >
                {message.content}
              </ReactMarkdown>
            </div>
          ) : (
            <p className="whitespace-pre-wrap">{message.content}</p>
          )}

          {/* Streaming indicator */}
          {isStreaming && (
            <span className="inline-block w-2 h-4 ml-1 bg-[#a78bfa] animate-pulse" />
          )}

          {/* Citations (for assistant messages) */}
          {isAssistant && message.citations && message.citations.length > 0 && (
            <div className="mt-3 pt-3 border-t border-[#3d3d3d]">
              <p className="text-xs text-[#888888] mb-2">Sources:</p>
              <div className="flex flex-wrap gap-1">
                {message.citations.map((citation) => (
                  <button
                    key={citation.id}
                    onClick={(e) => handleCitationClick(citation, e.currentTarget)}
                    className="px-2 py-0.5 text-xs rounded bg-[#3d3d3d] hover:bg-[#4d4d4d] text-[#a78bfa] transition-colors cursor-pointer"
                    title={citation.excerpt}
                  >
                    [{citation.citation_index}]
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Tool calls (collapsible) */}
          {isAssistant && message.tool_calls && message.tool_calls.length > 0 && (
            <details className="mt-3 pt-3 border-t border-[#3d3d3d]">
              <summary className="text-xs text-[#888888] cursor-pointer hover:text-[#a78bfa]">
                {message.tool_calls.length} retrieval step{message.tool_calls.length !== 1 ? 's' : ''}
              </summary>
              <div className="mt-2 space-y-2">
                {message.tool_calls.map((toolCall) => (
                  <div
                    key={toolCall.id}
                    className="text-xs p-2 bg-[#1e1e1e] rounded"
                  >
                    <span className="text-[#7c3aed]">{toolCall.tool_name}</span>
                    <span className="text-[#666666] ml-2">
                      {toolCall.status === 'complete' ? '✓' : toolCall.status}
                    </span>
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>
      </div>

      {/* Citation popover */}
      {activeCitation && anchorRect && (
        <CitationPopover
          citation={activeCitation}
          anchorRect={anchorRect}
          onClose={handleClosePopover}
          onViewAtom={handleViewAtom}
        />
      )}
    </>
  );
}
