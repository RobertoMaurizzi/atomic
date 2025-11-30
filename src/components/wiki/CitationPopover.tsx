import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useKeyboard } from '../../hooks/useKeyboard';

// Generic citation interface that works with both WikiCitation and ChatCitation
export interface CitationForPopover {
  citation_index: number;
  atom_id: string;
  excerpt: string;
}

interface CitationPopoverProps {
  citation: CitationForPopover;
  anchorRect: { top: number; left: number; bottom: number; width: number } | null;
  onClose: () => void;
  onViewAtom: (atomId: string) => void;
}

// Calculate position based on anchor rect
function calculatePosition(
  anchorRect: { top: number; left: number; bottom: number; width: number },
  popoverHeight: number,
  popoverWidth: number
): { top: number; left: number } {
  // Calculate position - prefer below, but go above if not enough space
  const spaceBelow = window.innerHeight - anchorRect.bottom;
  const spaceAbove = anchorRect.top;

  let top: number;
  if (spaceBelow >= popoverHeight + 8 || spaceBelow >= spaceAbove) {
    // Position below
    top = anchorRect.bottom + 8;
  } else {
    // Position above
    top = anchorRect.top - popoverHeight - 8;
  }

  // Horizontal positioning - center on anchor, but keep within viewport
  let left = anchorRect.left + anchorRect.width / 2 - popoverWidth / 2;
  left = Math.max(8, Math.min(left, window.innerWidth - popoverWidth - 8));

  return { top, left };
}

export function CitationPopover({ citation, anchorRect, onClose, onViewAtom }: CitationPopoverProps) {
  const popoverRef = useRef<HTMLDivElement>(null);

  // Calculate initial position immediately (synchronously)
  const initialPosition = anchorRect ? calculatePosition(anchorRect, 180, 400) : null;
  const [position, setPosition] = useState<{ top: number; left: number } | null>(initialPosition);

  // Close on Escape
  useKeyboard('Escape', onClose, true);

  // Close on click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  // Update position when anchorRect changes
  useEffect(() => {
    if (!anchorRect) return;
    const pos = calculatePosition(anchorRect, 180, 400);
    setPosition(pos);
  }, [anchorRect]);

  // Refine position after render with actual dimensions
  useLayoutEffect(() => {
    if (!anchorRect || !popoverRef.current) return;

    const popoverRect = popoverRef.current.getBoundingClientRect();
    const refinedPos = calculatePosition(anchorRect, popoverRect.height, popoverRect.width);

    setPosition(refinedPos);
  }, [anchorRect]);

  const handleViewAtom = () => {
    onViewAtom(citation.atom_id);
    onClose();
  };

  // Truncate excerpt if needed
  const displayExcerpt = citation.excerpt.length > 300
    ? citation.excerpt.slice(0, 297) + '...'
    : citation.excerpt;

  // Don't render until we have a position
  if (!position) {
    return null;
  }

  // Render in a portal to avoid transform containment issues
  return createPortal(
    <div
      ref={popoverRef}
      className="fixed z-[100] w-[400px] max-w-[calc(100vw-16px)] bg-[#2d2d2d] border border-[#3d3d3d] rounded-lg shadow-xl"
      style={{ top: position.top, left: position.left }}
    >
      {/* Citation number badge */}
      <div className="px-4 py-2 border-b border-[#3d3d3d] flex items-center gap-2">
        <span className="inline-flex items-center justify-center w-6 h-6 rounded bg-[#7c3aed]/20 text-[#a78bfa] text-xs font-medium">
          {citation.citation_index}
        </span>
        <span className="text-xs text-[#888888]">Source excerpt</span>
      </div>

      {/* Excerpt content */}
      <div className="px-4 py-3">
        <p className="text-sm text-[#dcddde] leading-relaxed whitespace-pre-wrap">
          {displayExcerpt}
        </p>
      </div>

      {/* Footer with link */}
      <div className="px-4 py-2 border-t border-[#3d3d3d]">
        <button
          onClick={handleViewAtom}
          className="flex items-center gap-1 text-sm text-[#7c3aed] hover:text-[#a78bfa] transition-colors"
        >
          View full atom
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14 5l7 7m0 0l-7 7m7-7H3" />
          </svg>
        </button>
      </div>
    </div>,
    document.body
  );
}

