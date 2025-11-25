import { memo } from 'react';
import { AtomWithTags } from '../../stores/atoms';

interface AtomNodeProps {
  atom: AtomWithTags;
  x: number;
  y: number;
  isFaded: boolean;
  onClick: () => void;
}

export const AtomNode = memo(function AtomNode({
  atom,
  x,
  y,
  isFaded,
  onClick,
}: AtomNodeProps) {
  // Get first line of content, truncated to ~50 characters
  const displayContent = getDisplayContent(atom.content);

  return (
    <div
      className={`absolute cursor-pointer select-none transition-all duration-150 ${
        isFaded ? 'opacity-20 pointer-events-none' : 'opacity-100'
      }`}
      style={{
        left: x,
        top: y,
        transform: 'translate(-50%, -50%)',
        width: '160px',
      }}
      onClick={onClick}
    >
      <div
        className={`
          bg-[#2d2d2d] border border-[#3d3d3d] rounded-md px-3 py-2
          hover:scale-[1.02] hover:border-[#4d4d4d] transition-all duration-150
        `}
      >
        <p className="text-sm text-[#dcddde] line-clamp-2 break-words">
          {displayContent}
        </p>
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

