import { create } from 'zustand';
import { getTransport } from '../lib/transport';

// Types matching the Rust structs
export interface WikiArticle {
  id: string;
  tag_id: string;
  content: string;
  created_at: string;
  updated_at: string;
  atom_count: number;
}

export interface WikiCitation {
  id: string;
  citation_index: number;
  atom_id: string;
  chunk_index: number | null;
  excerpt: string;
}

export interface WikiArticleWithCitations {
  article: WikiArticle;
  citations: WikiCitation[];
}

export interface WikiArticleStatus {
  has_article: boolean;
  article_atom_count: number;
  current_atom_count: number;
  new_atoms_available: number;
  updated_at: string | null;
}

export interface WikiArticleSummary {
  id: string;
  tag_id: string;
  tag_name: string;
  updated_at: string;
  atom_count: number;
}

type WikiView = 'list' | 'article';

interface WikiStore {
  // View state
  view: WikiView;
  currentTagId: string | null;
  currentTagName: string | null;

  // Articles list state
  articles: WikiArticleSummary[];
  isLoadingList: boolean;

  // Current article state
  currentArticle: WikiArticleWithCitations | null;
  articleStatus: WikiArticleStatus | null;

  // Loading states
  isLoading: boolean;
  isGenerating: boolean;
  isUpdating: boolean;
  error: string | null;

  // List actions
  fetchAllArticles: () => Promise<void>;
  showList: () => void;
  openArticle: (tagId: string, tagName: string) => void;
  openAndGenerate: (tagId: string, tagName: string) => void;
  goBack: () => void;

  // Article actions
  fetchArticle: (tagId: string) => Promise<void>;
  fetchArticleStatus: (tagId: string) => Promise<void>;
  generateArticle: (tagId: string, tagName: string) => Promise<void>;
  updateArticle: (tagId: string, tagName: string) => Promise<void>;
  deleteArticle: (tagId: string) => Promise<void>;
  clearArticle: () => void;
  clearError: () => void;
  reset: () => void;
}

export const useWikiStore = create<WikiStore>((set, get) => ({
  // View state
  view: 'list',
  currentTagId: null,
  currentTagName: null,

  // Articles list state
  articles: [],
  isLoadingList: false,

  // Current article state
  currentArticle: null,
  articleStatus: null,
  isLoading: false,
  isGenerating: false,
  isUpdating: false,
  error: null,

  fetchAllArticles: async () => {
    set({ isLoadingList: true, error: null });
    try {
      const articles = await getTransport().invoke<WikiArticleSummary[]>('get_all_wiki_articles');
      set({ articles, isLoadingList: false });
    } catch (error) {
      set({ error: String(error), isLoadingList: false });
    }
  },

  showList: () => {
    set({
      view: 'list',
      currentTagId: null,
      currentTagName: null,
      currentArticle: null,
      articleStatus: null,
      error: null,
    });
  },

  openArticle: (tagId: string, tagName: string) => {
    set({
      view: 'article',
      currentTagId: tagId,
      currentTagName: tagName,
      currentArticle: null,
      articleStatus: null,
      isLoading: true,
      error: null,
    });
    // Fetch article and status
    get().fetchArticle(tagId);
    get().fetchArticleStatus(tagId);
  },

  // Open article view and immediately start generating (for new wikis)
  openAndGenerate: (tagId: string, tagName: string) => {
    set({
      view: 'article',
      currentTagId: tagId,
      currentTagName: tagName,
      currentArticle: null,
      articleStatus: null,
      isLoading: false,
      isGenerating: true,
      error: null,
    });
    // Fetch status for display during generation
    get().fetchArticleStatus(tagId);
    // Start generation
    get().generateArticle(tagId, tagName);
  },

  goBack: () => {
    set({
      view: 'list',
      currentTagId: null,
      currentTagName: null,
      currentArticle: null,
      articleStatus: null,
      error: null,
    });
    // Refresh list in case changes were made
    get().fetchAllArticles();
  },

  fetchArticle: async (tagId: string) => {
    set({ isLoading: true, error: null });
    try {
      const article = await getTransport().invoke<WikiArticleWithCitations | null>('get_wiki_article', { tagId });
      set({ currentArticle: article, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  fetchArticleStatus: async (tagId: string) => {
    try {
      const status = await getTransport().invoke<WikiArticleStatus>('get_wiki_article_status', { tagId });
      set({ articleStatus: status });
    } catch (error) {
      console.error('Failed to fetch article status:', error);
    }
  },

  generateArticle: async (tagId: string, tagName: string) => {
    set({ isGenerating: true, error: null });
    try {
      const article = await getTransport().invoke<WikiArticleWithCitations>('generate_wiki_article', { tagId, tagName });
      set({ currentArticle: article, isGenerating: false });
      // Refresh status after generation
      get().fetchArticleStatus(tagId);
      // Also refresh the list to include the new article
      get().fetchAllArticles();
    } catch (error) {
      set({ error: String(error), isGenerating: false });
    }
  },

  updateArticle: async (tagId: string, tagName: string) => {
    set({ isUpdating: true, error: null });
    try {
      const article = await getTransport().invoke<WikiArticleWithCitations>('update_wiki_article', { tagId, tagName });
      set({ currentArticle: article, isUpdating: false });
      // Refresh status after update
      get().fetchArticleStatus(tagId);
      // Also refresh the list
      get().fetchAllArticles();
    } catch (error) {
      set({ error: String(error), isUpdating: false });
    }
  },

  deleteArticle: async (tagId: string) => {
    try {
      await getTransport().invoke('delete_wiki_article', { tagId });
      set({ currentArticle: null, articleStatus: null });
      // Refresh the list
      get().fetchAllArticles();
    } catch (error) {
      set({ error: String(error) });
    }
  },

  clearArticle: () => {
    set({ currentArticle: null, articleStatus: null, error: null });
  },

  clearError: () => {
    set({ error: null });
  },

  reset: () => {
    set({
      view: 'list',
      currentTagId: null,
      currentTagName: null,
      articles: [],
      isLoadingList: false,
      currentArticle: null,
      articleStatus: null,
      isLoading: false,
      isGenerating: false,
      isUpdating: false,
      error: null,
    });
  },
}));
