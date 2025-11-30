import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type DrawerMode = 'editor' | 'viewer' | 'wiki' | 'chat';
export type ViewMode = 'canvas' | 'grid' | 'list';

interface DrawerState {
  isOpen: boolean;
  mode: DrawerMode;
  atomId: string | null;      // For editor/viewer modes
  tagId: string | null;       // For wiki and chat modes
  tagName: string | null;     // For wiki mode (display purposes)
  conversationId: string | null;  // For chat mode
}

interface LocalGraphState {
  isOpen: boolean;
  centerAtomId: string | null;
  depth: 1 | 2;
  navigationHistory: string[];  // For breadcrumb navigation
}

export interface LoadingOperation {
  id: string;
  message: string;
  timestamp: number;
}

interface UIStore {
  selectedTagId: string | null;
  drawerState: DrawerState;
  viewMode: ViewMode;
  searchQuery: string;
  loadingOperations: LoadingOperation[];
  // Local graph state
  localGraph: LocalGraphState;
  highlightedAtomId: string | null;
  // Actions
  setSelectedTag: (tagId: string | null) => void;
  openDrawer: (mode: DrawerMode, atomId?: string) => void;
  openWikiDrawer: (tagId: string, tagName: string) => void;
  openChatDrawer: (tagId?: string, conversationId?: string) => void;
  closeDrawer: () => void;
  setViewMode: (mode: ViewMode) => void;
  setSearchQuery: (query: string) => void;
  addLoadingOperation: (id: string, message: string) => void;
  removeLoadingOperation: (id: string) => void;
  // Local graph actions
  openLocalGraph: (atomId: string, depth?: 1 | 2) => void;
  navigateLocalGraph: (atomId: string) => void;
  goBackLocalGraph: () => void;
  closeLocalGraph: () => void;
  setLocalGraphDepth: (depth: 1 | 2) => void;
  setHighlightedAtom: (atomId: string | null) => void;
  // Canvas navigation
  locateOnCanvas: (atomId: string) => void;
}

export const useUIStore = create<UIStore>()(
  persist(
    (set) => ({
      selectedTagId: null,
      drawerState: {
        isOpen: false,
        mode: 'viewer',
        atomId: null,
        tagId: null,
        tagName: null,
        conversationId: null,
      },
      viewMode: 'canvas',  // Default to canvas view
      searchQuery: '',
      loadingOperations: [],
      localGraph: {
        isOpen: false,
        centerAtomId: null,
        depth: 1,
        navigationHistory: [],
      },
      highlightedAtomId: null,

      setSelectedTag: (tagId: string | null) => set({ selectedTagId: tagId }),

      openDrawer: (mode: DrawerMode, atomId?: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode,
            atomId: atomId || null,
            tagId: null,
            tagName: null,
            conversationId: null,
          },
        }),

      openWikiDrawer: (tagId: string, tagName: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode: 'wiki',
            atomId: null,
            tagId,
            tagName,
            conversationId: null,
          },
        }),

      openChatDrawer: (tagId?: string, conversationId?: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode: 'chat',
            atomId: null,
            tagId: tagId || null,
            tagName: null,
            conversationId: conversationId || null,
          },
        }),

      closeDrawer: () =>
        set((state) => ({
          drawerState: {
            ...state.drawerState,
            isOpen: false,
          },
        })),

      setViewMode: (mode: ViewMode) => set({ viewMode: mode }),

      setSearchQuery: (query: string) => set({ searchQuery: query }),

      addLoadingOperation: (id: string, message: string) =>
        set((state) => ({
          loadingOperations: [
            ...state.loadingOperations,
            { id, message, timestamp: Date.now() },
          ],
        })),

      removeLoadingOperation: (id: string) =>
        set((state) => ({
          loadingOperations: state.loadingOperations.filter((op) => op.id !== id),
        })),

      // Local graph actions
      openLocalGraph: (atomId: string, depth: 1 | 2 = 1) =>
        set({
          localGraph: {
            isOpen: true,
            centerAtomId: atomId,
            depth,
            navigationHistory: [atomId],
          },
        }),

      navigateLocalGraph: (atomId: string) =>
        set((state) => ({
          localGraph: {
            ...state.localGraph,
            centerAtomId: atomId,
            navigationHistory: [...state.localGraph.navigationHistory, atomId],
          },
        })),

      goBackLocalGraph: () =>
        set((state) => {
          const history = [...state.localGraph.navigationHistory];
          history.pop(); // Remove current
          const previousAtomId = history[history.length - 1] || null;
          return {
            localGraph: {
              ...state.localGraph,
              centerAtomId: previousAtomId,
              navigationHistory: history,
              isOpen: history.length > 0,
            },
          };
        }),

      closeLocalGraph: () =>
        set({
          localGraph: {
            isOpen: false,
            centerAtomId: null,
            depth: 1,
            navigationHistory: [],
          },
        }),

      setLocalGraphDepth: (depth: 1 | 2) =>
        set((state) => ({
          localGraph: {
            ...state.localGraph,
            depth,
          },
        })),

      setHighlightedAtom: (atomId: string | null) =>
        set({ highlightedAtomId: atomId }),

      // Canvas navigation - switch to canvas view and highlight the atom
      locateOnCanvas: (atomId: string) =>
        set((state) => ({
          viewMode: 'canvas',
          highlightedAtomId: atomId,
          drawerState: {
            ...state.drawerState,
            isOpen: false,
          },
          localGraph: {
            ...state.localGraph,
            isOpen: false,
          },
        })),
    }),
    {
      name: 'atomic-ui-storage',
      partialize: (state) => ({ viewMode: state.viewMode }),  // Only persist viewMode
    }
  )
);

