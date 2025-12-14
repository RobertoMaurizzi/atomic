import { useState, useCallback, useEffect, useMemo, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { PaletteMode, SemanticSearchResult, FuzzyMatch, TagWithCount } from './types';
import { commands } from './commands';
import { searchCommands } from './fuzzySearch';
import { useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { useAtomsStore } from '../../stores/atoms';

const RECENT_COMMANDS_KEY = 'atomic-recent-commands';
const MAX_RECENT_COMMANDS = 5;
const SEARCH_DEBOUNCE_MS = 300;

interface UseCommandPaletteOptions {
  isOpen: boolean;
  onClose: () => void;
  initialQuery?: string;
}

export function useCommandPalette({ isOpen, onClose, initialQuery = '' }: UseCommandPaletteOptions) {
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<PaletteMode>('commands');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [searchResults, setSearchResults] = useState<SemanticSearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [recentCommandIds, setRecentCommandIds] = useState<string[]>([]);

  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const tags = useTagsStore((state) => state.tags);

  // Load recent commands from localStorage
  useEffect(() => {
    try {
      const stored = localStorage.getItem(RECENT_COMMANDS_KEY);
      if (stored) {
        setRecentCommandIds(JSON.parse(stored));
      }
    } catch {
      // Ignore localStorage errors
    }
  }, []);

  // Reset state when palette opens/closes
  useEffect(() => {
    if (isOpen) {
      setQuery(initialQuery);
      setSelectedIndex(0);
      setSearchResults([]);
      setIsSearching(false);
      // Set mode based on initial query
      if (initialQuery.startsWith('/')) {
        setMode('search');
      } else if (initialQuery.startsWith('#')) {
        setMode('tags');
      } else {
        setMode('commands');
      }
    }
  }, [isOpen, initialQuery]);

  // Detect mode from query prefix
  useEffect(() => {
    if (query.startsWith('/')) {
      if (mode !== 'search') {
        setMode('search');
        setSelectedIndex(0);
      }
    } else if (query.startsWith('#')) {
      if (mode !== 'tags') {
        setMode('tags');
        setSelectedIndex(0);
      }
    } else {
      if (mode !== 'commands') {
        setMode('commands');
        setSelectedIndex(0);
      }
    }
  }, [query, mode]);

  // Get the actual search query (without prefix)
  const searchQuery = useMemo(() => {
    if (query.startsWith('/') || query.startsWith('#')) {
      return query.slice(1);
    }
    return query;
  }, [query]);

  // Debounced atom search
  useEffect(() => {
    if (mode !== 'search') {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }

    // Clear previous timeout
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    const trimmedQuery = searchQuery.trim();
    if (trimmedQuery.length < 2) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }

    setIsSearching(true);

    searchTimeoutRef.current = setTimeout(async () => {
      try {
        const results = await invoke<SemanticSearchResult[]>('search_atoms_hybrid', {
          query: trimmedQuery,
          limit: 10,
          threshold: 0.3,
        });
        setSearchResults(results);
      } catch (error) {
        console.error('Search failed:', error);
        setSearchResults([]);
      } finally {
        setIsSearching(false);
      }
    }, SEARCH_DEBOUNCE_MS);

    return () => {
      if (searchTimeoutRef.current) {
        clearTimeout(searchTimeoutRef.current);
      }
    };
  }, [mode, searchQuery]);

  // Fuzzy search commands
  const filteredCommands: FuzzyMatch[] = useMemo(() => {
    if (mode !== 'commands') return [];
    return searchCommands(query, commands);
  }, [mode, query]);

  // Recent commands (only when no query)
  const recentCommands = useMemo(() => {
    if (mode !== 'commands' || query.trim()) return [];

    return recentCommandIds
      .map((id) => commands.find((cmd) => cmd.id === id))
      .filter((cmd): cmd is NonNullable<typeof cmd> => cmd !== undefined)
      .filter((cmd) => cmd.isEnabled?.() ?? true);
  }, [mode, query, recentCommandIds]);

  // Filter tags by query
  const filteredTags = useMemo(() => {
    if (mode !== 'tags') return [];

    const trimmedQuery = searchQuery.toLowerCase().trim();

    // Flatten tags for searching
    const flattenTags = (tagList: TagWithCount[], result: TagWithCount[] = []): TagWithCount[] => {
      for (const tag of tagList) {
        result.push(tag);
        if (tag.children?.length) {
          flattenTags(tag.children, result);
        }
      }
      return result;
    };

    const allTags = flattenTags(tags);

    if (!trimmedQuery) {
      return allTags.slice(0, 10); // Show top 10 tags by default
    }

    return allTags
      .filter((tag) => tag.name.toLowerCase().includes(trimmedQuery))
      .sort((a, b) => {
        // Prioritize tags that start with the query
        const aStarts = a.name.toLowerCase().startsWith(trimmedQuery);
        const bStarts = b.name.toLowerCase().startsWith(trimmedQuery);
        if (aStarts && !bStarts) return -1;
        if (!aStarts && bStarts) return 1;
        // Then sort by atom count
        return b.atom_count - a.atom_count;
      })
      .slice(0, 10);
  }, [mode, searchQuery, tags]);

  // Total items for navigation
  const totalItems = useMemo(() => {
    switch (mode) {
      case 'commands':
        return (query.trim() ? filteredCommands.length : recentCommands.length + filteredCommands.length);
      case 'search':
        return searchResults.length;
      case 'tags':
        return filteredTags.length;
      default:
        return 0;
    }
  }, [mode, query, filteredCommands.length, recentCommands.length, searchResults.length, filteredTags.length]);

  // Keyboard navigation
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          setSelectedIndex((prev) => Math.min(prev + 1, totalItems - 1));
          break;
        case 'ArrowUp':
          e.preventDefault();
          setSelectedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case 'Enter':
          e.preventDefault();
          handleSelect(selectedIndex);
          break;
        case 'Escape':
          e.preventDefault();
          onClose();
          break;
        case 'Backspace':
          // If query is just the prefix, clear it and go back to commands mode
          if (query === '/' || query === '#') {
            e.preventDefault();
            setQuery('');
          }
          break;
      }
    },
    [selectedIndex, totalItems, onClose, query]
  );

  // Record a command as recently used
  const recordRecentCommand = useCallback((commandId: string) => {
    setRecentCommandIds((prev) => {
      const filtered = prev.filter((id) => id !== commandId);
      const updated = [commandId, ...filtered].slice(0, MAX_RECENT_COMMANDS);

      try {
        localStorage.setItem(RECENT_COMMANDS_KEY, JSON.stringify(updated));
      } catch {
        // Ignore localStorage errors
      }

      return updated;
    });
  }, []);

  // Handle item selection
  const handleSelect = useCallback(
    (index: number) => {
      switch (mode) {
        case 'commands': {
          let command;
          if (!query.trim() && index < recentCommands.length) {
            command = recentCommands[index];
          } else {
            const adjustedIndex = query.trim() ? index : index - recentCommands.length;
            command = filteredCommands[adjustedIndex]?.command;
          }

          if (command) {
            // Check for special commands that switch modes
            if (command.id === 'search-atoms') {
              setQuery('/');
              return;
            }
            if (command.id === 'filter-by-tag') {
              setQuery('#');
              return;
            }

            recordRecentCommand(command.id);
            onClose();
            command.action();
          }
          break;
        }
        case 'search': {
          const result = searchResults[index];
          if (result) {
            onClose();
            // Open the atom in viewer
            useUIStore.getState().openDrawer('viewer', result.id, result.matching_chunk_content);
          }
          break;
        }
        case 'tags': {
          const tag = filteredTags[index];
          if (tag) {
            onClose();

            // Build ancestor path for expansion
            const ancestorIds: string[] = [];
            let currentParentId = tag.parent_id;

            // Flatten all tags to find parents
            const flattenTags = (tagList: TagWithCount[]): Map<string, TagWithCount> => {
              const map = new Map<string, TagWithCount>();
              const traverse = (list: TagWithCount[]) => {
                for (const t of list) {
                  map.set(t.id, t);
                  if (t.children?.length) traverse(t.children);
                }
              };
              traverse(tagList);
              return map;
            };
            const tagMap = flattenTags(tags);

            // Walk up the tree to collect ancestor IDs
            while (currentParentId) {
              ancestorIds.push(currentParentId);
              const parentTag = tagMap.get(currentParentId);
              currentParentId = parentTag?.parent_id || null;
            }

            // Expand ancestors before selecting (so the tag is visible)
            if (ancestorIds.length > 0) {
              useUIStore.getState().expandTagPath(ancestorIds);
            }

            // Filter by this tag
            useUIStore.getState().setSelectedTag(tag.id);
            useAtomsStore.getState().fetchAtomsByTag(tag.id);
          }
          break;
        }
      }
    },
    [mode, query, recentCommands, filteredCommands, searchResults, filteredTags, tags, recordRecentCommand, onClose]
  );

  return {
    query,
    setQuery,
    mode,
    selectedIndex,
    setSelectedIndex,
    searchResults,
    isSearching,
    filteredCommands,
    recentCommands,
    filteredTags,
    handleKeyDown,
    handleSelect,
    totalItems,
  };
}
