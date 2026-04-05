import { useMemo, useState } from 'react';
import { diffLines } from 'diff';
import { Button } from '../ui/Button';
import { formatRelativeTime } from '../../lib/date';

interface WikiProposalDiffProps {
  liveContent: string;
  proposalContent: string;
  newAtomCount: number;
  createdAt: string;
  onAccept: () => void;
  onDismiss: () => void;
  onCancel: () => void;
  isAccepting: boolean;
  isDismissing: boolean;
}

type DiffLine =
  | { kind: 'context'; text: string }
  | { kind: 'add'; text: string }
  | { kind: 'remove'; text: string };

type DiffBlock =
  | { kind: 'visible'; lines: DiffLine[] }
  | { kind: 'collapsed'; lines: DiffLine[] };

const COLLAPSE_THRESHOLD = 5;

function buildDiffBlocks(live: string, proposal: string): DiffBlock[] {
  const parts = diffLines(live, proposal);
  const lines: DiffLine[] = [];

  for (const part of parts) {
    const partLines = part.value.split('\n');
    // diffLines always leaves a trailing empty string after the last newline —
    // drop it so we don't emit empty phantom lines.
    if (partLines.length > 0 && partLines[partLines.length - 1] === '') {
      partLines.pop();
    }
    for (const text of partLines) {
      if (part.added) {
        lines.push({ kind: 'add', text });
      } else if (part.removed) {
        lines.push({ kind: 'remove', text });
      } else {
        lines.push({ kind: 'context', text });
      }
    }
  }

  // Group runs of context lines ≥ COLLAPSE_THRESHOLD into collapsed blocks.
  const blocks: DiffBlock[] = [];
  let buf: DiffLine[] = [];
  let contextRun: DiffLine[] = [];

  const flushContextRun = () => {
    if (contextRun.length >= COLLAPSE_THRESHOLD) {
      if (buf.length > 0) {
        blocks.push({ kind: 'visible', lines: buf });
        buf = [];
      }
      blocks.push({ kind: 'collapsed', lines: contextRun });
    } else {
      buf.push(...contextRun);
    }
    contextRun = [];
  };

  for (const line of lines) {
    if (line.kind === 'context') {
      contextRun.push(line);
    } else {
      flushContextRun();
      buf.push(line);
    }
  }
  flushContextRun();
  if (buf.length > 0) {
    blocks.push({ kind: 'visible', lines: buf });
  }

  return blocks;
}

function countChanges(blocks: DiffBlock[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const block of blocks) {
    if (block.kind === 'visible') {
      for (const line of block.lines) {
        if (line.kind === 'add') added++;
        else if (line.kind === 'remove') removed++;
      }
    }
  }
  return { added, removed };
}

export function WikiProposalDiff({
  liveContent,
  proposalContent,
  newAtomCount,
  createdAt,
  onAccept,
  onDismiss,
  onCancel,
  isAccepting,
  isDismissing,
}: WikiProposalDiffProps) {
  const blocks = useMemo(
    () => buildDiffBlocks(liveContent, proposalContent),
    [liveContent, proposalContent]
  );
  const { added, removed } = useMemo(() => countChanges(blocks), [blocks]);

  const [expanded, setExpanded] = useState<Set<number>>(new Set());

  const toggleCollapsed = (idx: number) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) {
        next.delete(idx);
      } else {
        next.add(idx);
      }
      return next;
    });
  };

  return (
    <div className="flex flex-col flex-1 min-h-0">
      {/* Diff header */}
      <div className="flex items-center justify-between px-6 py-3 border-b border-[var(--color-border)] bg-[var(--color-bg-card)]/40">
        <div className="flex flex-col">
          <span className="text-sm font-medium text-[var(--color-text-primary)]">
            Suggested update
          </span>
          <span className="text-xs text-[var(--color-text-secondary)]">
            Based on {newAtomCount} new atom{newAtomCount !== 1 ? 's' : ''} • computed {formatRelativeTime(createdAt)}
            {' • '}
            <span className="text-green-400">+{added}</span>{' / '}
            <span className="text-red-400">−{removed}</span> lines
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onCancel} disabled={isAccepting || isDismissing}>
            Cancel
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={onDismiss}
            disabled={isAccepting || isDismissing}
          >
            {isDismissing ? 'Dismissing…' : 'Dismiss'}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={onAccept}
            disabled={isAccepting || isDismissing}
          >
            {isAccepting ? 'Applying…' : 'Accept'}
          </Button>
        </div>
      </div>

      {/* Diff body */}
      <div className="flex-1 overflow-y-auto px-6 py-4 font-mono text-xs leading-relaxed">
        {blocks.map((block, blockIdx) => {
          if (block.kind === 'collapsed' && !expanded.has(blockIdx)) {
            return (
              <button
                key={blockIdx}
                onClick={() => toggleCollapsed(blockIdx)}
                className="block w-full text-left py-1 px-2 my-1 rounded bg-[var(--color-bg-card)]/60 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors text-[11px] italic"
              >
                … {block.lines.length} unchanged line{block.lines.length !== 1 ? 's' : ''} (click to expand)
              </button>
            );
          }

          const lines = block.lines;

          return (
            <div key={blockIdx}>
              {block.kind === 'collapsed' && (
                <button
                  onClick={() => toggleCollapsed(blockIdx)}
                  className="block w-full text-left py-1 px-2 my-1 rounded bg-[var(--color-bg-card)]/60 text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-hover)] transition-colors text-[11px] italic"
                >
                  ⌃ collapse {block.lines.length} unchanged lines
                </button>
              )}
              {lines.map((line, lineIdx) => {
                if (line.kind === 'add') {
                  return (
                    <div
                      key={lineIdx}
                      className="whitespace-pre-wrap px-2 border-l-2 border-green-500/60 bg-green-500/10 text-green-200"
                    >
                      <span className="select-none text-green-500/70 mr-2">+</span>
                      {line.text || '\u00A0'}
                    </div>
                  );
                }
                if (line.kind === 'remove') {
                  return (
                    <div
                      key={lineIdx}
                      className="whitespace-pre-wrap px-2 border-l-2 border-red-500/60 bg-red-500/10 text-red-200"
                    >
                      <span className="select-none text-red-500/70 mr-2">−</span>
                      {line.text || '\u00A0'}
                    </div>
                  );
                }
                return (
                  <div
                    key={lineIdx}
                    className="whitespace-pre-wrap px-2 text-[var(--color-text-secondary)]"
                  >
                    <span className="select-none opacity-40 mr-2">&nbsp;</span>
                    {line.text || '\u00A0'}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
