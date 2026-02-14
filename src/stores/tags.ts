import { create } from 'zustand';
import { getTransport } from '../lib/transport';

export interface Tag {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
}

export interface TagWithCount extends Tag {
  atom_count: number;
  children: TagWithCount[];
}

export interface CompactionResult {
  tags_moved: number;
  tags_merged: number;
  atoms_retagged: number;
}

interface TagsStore {
  tags: TagWithCount[];
  isLoading: boolean;
  isCompacting: boolean;
  error: string | null;
  fetchTags: () => Promise<void>;
  createTag: (name: string, parentId?: string) => Promise<Tag>;
  updateTag: (id: string, name: string, parentId?: string) => Promise<Tag>;
  deleteTag: (id: string) => Promise<void>;
  compactTags: () => Promise<CompactionResult>;
  clearError: () => void;
}

export const useTagsStore = create<TagsStore>((set) => ({
  tags: [],
  isLoading: false,
  isCompacting: false,
  error: null,

  fetchTags: async () => {
    set({ isLoading: true, error: null });
    try {
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  createTag: async (name: string, parentId?: string) => {
    set({ error: null });
    try {
      const tag = await getTransport().invoke<Tag>('create_tag', {
        name,
        parentId: parentId || null,
      });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
      return tag;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  updateTag: async (id: string, name: string, parentId?: string) => {
    set({ error: null });
    try {
      const tag = await getTransport().invoke<Tag>('update_tag', {
        id,
        name,
        parentId: parentId || null,
      });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
      return tag;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  deleteTag: async (id: string) => {
    set({ error: null });
    try {
      await getTransport().invoke('delete_tag', { id });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  compactTags: async () => {
    set({ isCompacting: true, error: null });
    try {
      const result = await getTransport().invoke<CompactionResult>('compact_tags');
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags, isCompacting: false });
      return result;
    } catch (error) {
      set({ error: String(error), isCompacting: false });
      throw error;
    }
  },

  clearError: () => set({ error: null }),
}));

