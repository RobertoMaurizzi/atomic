import { getTransport } from './transport';

// Type-safe wrapper for checking sqlite-vec
export async function checkSqliteVec(): Promise<string> {
  return getTransport().invoke<string>('check_sqlite_vec');
}

// Semantic search
export async function searchAtomsSemantic(
  query: string,
  limit: number = 20,
  threshold: number = 0.3
): Promise<any[]> {
  return getTransport().invoke('search_atoms_semantic', { query, limit, threshold });
}

// Find similar atoms
export async function findSimilarAtoms(
  atomId: string,
  limit: number = 5,
  threshold: number = 0.7
): Promise<any[]> {
  return getTransport().invoke('find_similar_atoms', { atomId, limit, threshold });
}

// Retry embedding
export async function retryEmbedding(atomId: string): Promise<void> {
  return getTransport().invoke('retry_embedding', { atomId });
}

// Reset atoms stuck in 'processing' state (call on app startup)
export async function resetStuckProcessing(): Promise<number> {
  return getTransport().invoke('reset_stuck_processing');
}

// Process pending embeddings
export async function processPendingEmbeddings(): Promise<number> {
  return getTransport().invoke('process_pending_embeddings');
}

// Process pending tagging (for atoms with completed embeddings)
export async function processPendingTagging(): Promise<number> {
  return getTransport().invoke('process_pending_tagging');
}

// Get embedding status
export async function getEmbeddingStatus(atomId: string): Promise<string> {
  return getTransport().invoke('get_embedding_status', { atomId });
}

// Wiki commands
export async function getWikiArticle(tagId: string): Promise<any | null> {
  return getTransport().invoke('get_wiki_article', { tagId });
}

export async function getWikiArticleStatus(tagId: string): Promise<any> {
  return getTransport().invoke('get_wiki_article_status', { tagId });
}

export async function generateWikiArticle(tagId: string, tagName: string): Promise<any> {
  return getTransport().invoke('generate_wiki_article', { tagId, tagName });
}

export async function updateWikiArticle(tagId: string, tagName: string): Promise<any> {
  return getTransport().invoke('update_wiki_article', { tagId, tagName });
}

export async function deleteWikiArticle(tagId: string): Promise<void> {
  return getTransport().invoke('delete_wiki_article', { tagId });
}

// Canvas position commands
export interface AtomPosition {
  atom_id: string;
  x: number;
  y: number;
}

export interface AtomWithEmbedding {
  id: string;
  content: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: string;
  tags: Array<{
    id: string;
    name: string;
    parent_id: string | null;
    created_at: string;
  }>;
  embedding: number[] | null;
}

export async function getAtomPositions(): Promise<AtomPosition[]> {
  return getTransport().invoke('get_atom_positions');
}

export async function saveAtomPositions(positions: AtomPosition[]): Promise<void> {
  return getTransport().invoke('save_atom_positions', { positions });
}

export async function getAtomsWithEmbeddings(): Promise<AtomWithEmbedding[]> {
  return getTransport().invoke('get_atoms_with_embeddings');
}

// Semantic graph types and commands
export interface SemanticEdge {
  id: string;
  source_atom_id: string;
  target_atom_id: string;
  similarity_score: number;
  source_chunk_index: number | null;
  target_chunk_index: number | null;
  created_at: string;
}

export interface NeighborhoodAtom {
  id: string;
  content: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: string;
  tags: Array<{
    id: string;
    name: string;
    parent_id: string | null;
    created_at: string;
  }>;
  depth: number; // 0 = center, 1 = direct connection, 2 = friend-of-friend
}

export interface NeighborhoodEdge {
  source_id: string;
  target_id: string;
  edge_type: 'tag' | 'semantic' | 'both';
  strength: number; // 0-1
  shared_tag_count: number;
  similarity_score: number | null;
}

export interface NeighborhoodGraph {
  center_atom_id: string;
  atoms: NeighborhoodAtom[];
  edges: NeighborhoodEdge[];
}

export async function getSemanticEdges(minSimilarity: number = 0.5): Promise<SemanticEdge[]> {
  return getTransport().invoke('get_semantic_edges', { minSimilarity });
}

export async function getAtomNeighborhood(
  atomId: string,
  depth: number = 1,
  minSimilarity: number = 0.5
): Promise<NeighborhoodGraph> {
  return getTransport().invoke('get_atom_neighborhood', { atomId, depth, minSimilarity });
}

export async function rebuildSemanticEdges(): Promise<number> {
  return getTransport().invoke('rebuild_semantic_edges');
}

// Clustering types and commands
export interface AtomCluster {
  cluster_id: number;
  atom_ids: string[];
  dominant_tags: string[];
}

export async function computeClusters(
  minSimilarity: number = 0.5,
  minClusterSize: number = 2
): Promise<AtomCluster[]> {
  return getTransport().invoke('compute_clusters', { minSimilarity, minClusterSize });
}

export async function getClusters(): Promise<AtomCluster[]> {
  return getTransport().invoke('get_clusters');
}

export async function getConnectionCounts(
  minSimilarity: number = 0.5
): Promise<Record<string, number>> {
  return getTransport().invoke('get_connection_counts', { minSimilarity });
}

// Model discovery types and commands
export interface AvailableModel {
  id: string;
  name: string;
}

export async function getAvailableLlmModels(): Promise<AvailableModel[]> {
  return getTransport().invoke('get_available_llm_models');
}

// Ollama types and commands
export interface OllamaModel {
  id: string;
  name: string;
  is_embedding: boolean;
  embedding_dimension: number | null;
}

export async function testOllamaConnection(host: string): Promise<boolean> {
  return getTransport().invoke('test_ollama', { host });
}

export async function getOllamaModels(host: string): Promise<OllamaModel[]> {
  return getTransport().invoke('get_ollama_models', { host });
}

export async function getOllamaEmbeddingModels(host: string): Promise<AvailableModel[]> {
  return getTransport().invoke('get_ollama_embedding_models_cmd', { host });
}

export async function getOllamaLlmModels(host: string): Promise<AvailableModel[]> {
  return getTransport().invoke('get_ollama_llm_models_cmd', { host });
}

// Setup verification
export async function verifyProviderConfigured(): Promise<boolean> {
  return getTransport().invoke('verify_provider_configured');
}

// Import types and commands
export interface ImportResult {
  imported: number;
  skipped: number;
  errors: number;
  tags_created: number;
  tags_linked: number;
}

export async function importObsidianVault(
  vaultPath: string,
  maxNotes?: number
): Promise<ImportResult> {
  return getTransport().invoke('import_obsidian_vault', { vaultPath, maxNotes });
}

// API Token types and commands
export interface ApiTokenInfo {
  id: string;
  name: string;
  token_prefix: string;
  created_at: string;
  last_used_at: string | null;
  is_revoked: boolean;
}

export interface CreateTokenResponse {
  id: string;
  name: string;
  token: string;
  prefix: string;
  created_at: string;
}

export async function createApiToken(name: string): Promise<CreateTokenResponse> {
  return getTransport().invoke('create_api_token', { name });
}

export async function listApiTokens(): Promise<ApiTokenInfo[]> {
  return getTransport().invoke('list_api_tokens');
}

export async function revokeApiToken(id: string): Promise<void> {
  return getTransport().invoke('revoke_api_token', { id });
}

// MCP Bridge commands
export interface McpConfig {
  mcpServers: {
    atomic: {
      command: string;
    };
  };
}

export async function getMcpBridgePath(): Promise<string> {
  return getTransport().invoke('get_mcp_bridge_path');
}

export async function getMcpConfig(): Promise<McpConfig> {
  return getTransport().invoke('get_mcp_config');
}
