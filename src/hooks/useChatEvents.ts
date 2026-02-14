import { useEffect } from 'react';
import { getTransport } from '../lib/transport';
import { useChatStore, ChatMessageWithContext, RetrievalStep } from '../stores/chat';

interface ChatStreamDelta {
  conversation_id: string;
  content: string;
}

interface ChatToolStart {
  conversation_id: string;
  tool_call_id: string;
  tool_name: string;
  tool_input: unknown;
}

interface ChatToolComplete {
  conversation_id: string;
  tool_call_id: string;
  results_count: number;
}

interface ChatComplete {
  conversation_id: string;
  message: ChatMessageWithContext;
}

interface ChatError {
  conversation_id: string;
  error: string;
}

export function useChatEvents(conversationId: string | null) {
  const appendStreamContent = useChatStore(s => s.appendStreamContent);
  const addRetrievalStep = useChatStore(s => s.addRetrievalStep);
  const completeMessage = useChatStore(s => s.completeMessage);
  const setStreamingError = useChatStore(s => s.setStreamingError);

  useEffect(() => {
    if (!conversationId) return;

    const transport = getTransport();
    const unsubs: Array<() => void> = [];

    // Listen for streaming content
    unsubs.push(transport.subscribe<ChatStreamDelta>('chat-stream-delta', (payload) => {
      if (payload.conversation_id === conversationId) {
        appendStreamContent(payload.content);
      }
    }));

    // Listen for tool start
    unsubs.push(transport.subscribe<ChatToolStart>('chat-tool-start', (payload) => {
      if (payload.conversation_id === conversationId) {
        const step: RetrievalStep = {
          step_number: Date.now(), // Temporary, will be replaced
          tool_name: payload.tool_name,
          query: JSON.stringify(payload.tool_input),
          results_count: 0,
          timestamp: new Date().toISOString(),
        };
        addRetrievalStep(step);
      }
    }));

    // Listen for tool complete
    unsubs.push(transport.subscribe<ChatToolComplete>('chat-tool-complete', (payload) => {
      if (payload.conversation_id === conversationId) {
        // Update the last retrieval step with results count
        // For now, this is handled by the store
      }
    }));

    // Listen for completion
    unsubs.push(transport.subscribe<ChatComplete>('chat-complete', (payload) => {
      if (payload.conversation_id === conversationId) {
        completeMessage(payload.message);
      }
    }));

    // Listen for errors
    unsubs.push(transport.subscribe<ChatError>('chat-error', (payload) => {
      if (payload.conversation_id === conversationId) {
        setStreamingError(payload.error);
      }
    }));

    return () => {
      unsubs.forEach((unsub) => unsub());
    };
  }, [conversationId, appendStreamContent, addRetrievalStep, completeMessage, setStreamingError]);
}
