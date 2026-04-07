interface CitationLinkProps {
  index: number;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
}

export function CitationLink({ index, onClick }: CitationLinkProps) {
  return (
    <sup>
      <button
        onClick={onClick}
        className="text-[var(--color-accent)] hover:text-[var(--color-accent-light)] hover:underline transition-colors text-[10px] font-medium mx-px"
      >
        [{index}]
      </button>
    </sup>
  );
}

