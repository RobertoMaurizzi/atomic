import { memo, useMemo } from 'react';
import { AtomWithTags } from '../../stores/atoms';

interface AtomNodeProps {
  atom: AtomWithTags;
  x: number;
  y: number;
  isFaded: boolean;
  isHub?: boolean;
  isHighlighted?: boolean;
  connectionCount?: number;
  onClick: (atomId: string) => void;
  atomId: string;
}

// Generate a consistent color from a string (tag name)
interface TagColor {
  h: number;
  s: number;
  l: number;
  hsl: string;
}

function stringToColor(str: string): TagColor {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }

  // Convert to HSL for better control over saturation and lightness
  const h = Math.abs(hash % 360);
  const s = 50 + (hash % 20); // 50-70% saturation
  const l = 45 + (hash % 10); // 45-55% lightness

  return { h, s, l, hsl: `hsl(${h}, ${s}%, ${l}%)` };
}

function colorWithAlpha(color: TagColor, alpha: number): string {
  return `hsla(${color.h}, ${color.s}%, ${color.l}%, ${alpha})`;
}

export const AtomNode = memo(function AtomNode({
  atom,
  x,
  y,
  isFaded,
  isHub = false,
  isHighlighted = false,
  connectionCount = 0,
  onClick,
  atomId,
}: AtomNodeProps) {
  // Get first line of content, truncated to ~50 characters
  const displayContent = getDisplayContent(atom.content);

  // Get color from primary tag
  const tagColor = useMemo(() => {
    if (atom.tags.length === 0) return null;
    return stringToColor(atom.tags[0].name);
  }, [atom.tags]);

  // Calculate node width based on connection count (for hubs)
  const nodeWidth = isHub ? 180 : 160;

  return (
    <div
      className={`absolute cursor-pointer select-none transition-all duration-150 ${
        isFaded ? 'opacity-20 pointer-events-none' : 'opacity-100'
      }`}
      style={{
        left: x,
        top: y,
        transform: 'translate(-50%, -50%)',
        width: `${nodeWidth}px`,
      }}
      onClick={() => onClick(atomId)}
    >
      <div
        className={`
          bg-[#2d2d2d] border rounded-md px-3 py-2
          hover:scale-[1.02] transition-all duration-150
          relative overflow-hidden
          ${isHighlighted
            ? 'border-[#22c55e] shadow-[0_0_20px_rgba(34,197,94,0.5)] animate-pulse ring-2 ring-[#22c55e] ring-opacity-50'
            : isHub
            ? 'border-[#7c3aed] shadow-[0_0_12px_rgba(124,58,237,0.3)]'
            : 'border-[#3d3d3d] hover:border-[#4d4d4d]'}
        `}
      >
        {/* Tag color indicator */}
        {tagColor && (
          <div
            className="absolute left-0 top-0 bottom-0 w-1 rounded-l"
            style={{ backgroundColor: tagColor.hsl }}
          />
        )}

        {/* Hub indicator */}
        {isHub && (
          <div className="absolute top-1 right-1">
            <div
              className="w-2 h-2 rounded-full bg-[#7c3aed] animate-pulse"
              title={`Hub: ${connectionCount} connections`}
            />
          </div>
        )}

        <p className={`text-sm text-[#dcddde] line-clamp-2 break-words ${isHub ? 'font-medium' : ''}`}>
          {displayContent}
        </p>

        {/* Show tag count indicator */}
        {atom.tags.length > 0 && (
          <div className="flex items-center gap-1 mt-1.5">
            <span
              className="text-[10px] px-1.5 py-0.5 rounded"
              style={{
                backgroundColor: tagColor ? colorWithAlpha(tagColor, 0.35) : '#3d3d3d',
                color: '#e0e0e0'
              }}
            >
              {atom.tags[0].name.length > 12
                ? atom.tags[0].name.substring(0, 10) + '...'
                : atom.tags[0].name}
            </span>
            {atom.tags.length > 1 && (
              <span className="text-[10px] text-[#666666]">
                +{atom.tags.length - 1}
              </span>
            )}
            {/* Show connection count for hubs */}
            {isHub && connectionCount > 0 && (
              <span className="text-[10px] text-[#7c3aed] ml-auto">
                {connectionCount}
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  );
});

function getDisplayContent(content: string): string {
  // Get first line
  const firstLine = content.split('\n')[0] || '';
  // Remove markdown formatting
  const cleaned = firstLine
    .replace(/^#+\s*/, '') // Remove heading markers
    .replace(/\*\*/g, '')  // Remove bold
    .replace(/\*/g, '')    // Remove italic
    .replace(/`/g, '')     // Remove code
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1') // Replace links with text
    .trim();

  // Truncate to ~50 characters
  if (cleaned.length > 50) {
    return cleaned.substring(0, 47) + '...';
  }
  return cleaned || 'Empty atom';
}
