import { useState, useEffect } from 'react';
import { useAtomsStore } from '../../stores/atoms';

export function SemanticSearch() {
  const {
    semanticSearchQuery,
    searchSemantic,
    clearSemanticSearch,
    isSearching,
  } = useAtomsStore();

  const [inputValue, setInputValue] = useState(semanticSearchQuery);

  // Debounce search by 300ms
  useEffect(() => {
    const timer = setTimeout(() => {
      if (inputValue.trim()) {
        searchSemantic(inputValue);
      } else {
        clearSemanticSearch();
      }
    }, 300);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inputValue]);

  // Handle keyboard (Escape to clear)
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setInputValue('');
      clearSemanticSearch();
    }
  };

  return (
    <div className="relative flex-1 max-w-md">
      {/* Search icon or loading spinner */}
      {isSearching ? (
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[#888888] animate-spin"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
          />
        </svg>
      ) : (
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[#888888]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
          />
        </svg>
      )}

      <input
        type="text"
        value={inputValue}
        onChange={(e) => setInputValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search by meaning..."
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="w-full pl-10 pr-10 py-2 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md text-[#dcddde] placeholder-[#888888] focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:border-transparent transition-colors text-sm"
      />

      {/* Clear button */}
      {inputValue && (
        <button
          onClick={() => {
            setInputValue('');
            clearSemanticSearch();
          }}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-[#888888] hover:text-[#dcddde] transition-colors"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      )}
    </div>
  );
}

