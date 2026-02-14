interface NormalizedEvent {
  event: string;
  payload: unknown;
}

export function normalizeServerEvent(data: Record<string, unknown>): NormalizedEvent | null {
  const type = data.type as string;
  switch (type) {
    case 'EmbeddingStarted':
      return { event: 'embedding-started', payload: { atom_id: data.atom_id } };
    case 'EmbeddingComplete':
      return { event: 'embedding-complete', payload: { atom_id: data.atom_id, status: 'complete' } };
    case 'EmbeddingFailed':
      return { event: 'embedding-complete', payload: { atom_id: data.atom_id, status: 'failed', error: data.error } };
    case 'TaggingComplete':
      return { event: 'tagging-complete', payload: { atom_id: data.atom_id, status: 'complete', tags_extracted: data.tags_extracted, new_tags_created: data.new_tags_created } };
    case 'TaggingFailed':
      return { event: 'tagging-complete', payload: { atom_id: data.atom_id, status: 'failed', error: data.error, tags_extracted: [], new_tags_created: [] } };
    case 'TaggingSkipped':
      return { event: 'tagging-complete', payload: { atom_id: data.atom_id, status: 'skipped', tags_extracted: [], new_tags_created: [] } };
    case 'ChatStreamDelta':
      return { event: 'chat-stream-delta', payload: data };
    case 'ChatToolStart':
      return { event: 'chat-tool-start', payload: data };
    case 'ChatToolComplete':
      return { event: 'chat-tool-complete', payload: data };
    case 'ChatComplete':
      return { event: 'chat-complete', payload: data };
    case 'ChatError':
      return { event: 'chat-error', payload: data };
    case 'AtomCreated':
      return { event: 'atom-created', payload: data.atom };
    case 'EmbeddingsReset':
      return { event: 'embeddings-reset', payload: data };
    default:
      console.warn('Unknown server event type:', type);
      return null;
  }
}
