interface TagChipProps {
  name: string;
  onClick?: (e: React.MouseEvent) => void;
  onRemove?: () => void;
  size?: 'sm' | 'md';
  className?: string;
}

export function TagChip({ name, onClick, onRemove, size = 'sm', className = '' }: TagChipProps) {
  const sizeStyles = {
    sm: 'px-2 py-0.5 text-xs',
    md: 'px-2.5 py-1 text-sm',
  };

  return (
    <span
      className={`inline-flex items-center gap-1 bg-[#7c3aed]/20 text-[#a78bfa] rounded-full ${sizeStyles[size]} ${
        onClick ? 'cursor-pointer hover:bg-[#7c3aed]/30 transition-colors' : ''
      } ${className}`}
      onClick={onClick}
    >
      {name}
      {onRemove && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="ml-0.5 hover:text-white transition-colors"
        >
          <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </span>
  );
}

