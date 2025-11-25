import { create } from 'zustand';
import { persist } from 'zustand/middleware';

export type DrawerMode = 'editor' | 'viewer' | 'wiki';
export type ViewMode = 'canvas' | 'grid' | 'list';

interface DrawerState {
  isOpen: boolean;
  mode: DrawerMode;
  atomId: string | null;      // For editor/viewer modes
  tagId: string | null;       // For wiki mode
  tagName: string | null;     // For wiki mode (display purposes)
}

interface UIStore {
  selectedTagId: string | null;
  drawerState: DrawerState;
  viewMode: ViewMode;
  searchQuery: string;
  setSelectedTag: (tagId: string | null) => void;
  openDrawer: (mode: DrawerMode, atomId?: string) => void;
  openWikiDrawer: (tagId: string, tagName: string) => void;
  closeDrawer: () => void;
  setViewMode: (mode: ViewMode) => void;
  setSearchQuery: (query: string) => void;
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
      },
      viewMode: 'canvas',  // Default to canvas view
      searchQuery: '',

      setSelectedTag: (tagId: string | null) => set({ selectedTagId: tagId }),

      openDrawer: (mode: DrawerMode, atomId?: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode,
            atomId: atomId || null,
            tagId: null,
            tagName: null,
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
    }),
    {
      name: 'atomic-ui-storage',
      partialize: (state) => ({ viewMode: state.viewMode }),  // Only persist viewMode
    }
  )
);

