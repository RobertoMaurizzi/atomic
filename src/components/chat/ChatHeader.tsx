import { useState } from 'react';
import { ConversationWithTags, useChatStore } from '../../stores/chat';
import { ScopeEditor } from './ScopeEditor';

interface ChatHeaderProps {
  conversation: ConversationWithTags;
  onBack: () => void;
}

export function ChatHeader({ conversation, onBack }: ChatHeaderProps) {
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [editedTitle, setEditedTitle] = useState(conversation.title || '');
  const { updateConversationTitle } = useChatStore();

  const handleTitleSave = async () => {
    if (editedTitle.trim() !== conversation.title) {
      await updateConversationTitle(conversation.id, editedTitle.trim() || 'Untitled');
    }
    setIsEditingTitle(false);
  };

  const handleTitleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleTitleSave();
    } else if (e.key === 'Escape') {
      setEditedTitle(conversation.title || '');
      setIsEditingTitle(false);
    }
  };

  return (
    <div className="flex-shrink-0 border-b border-[#3d3d3d]">
      {/* Top row: Back button and title */}
      <div className="flex items-center gap-3 px-4 py-3">
        <button
          onClick={onBack}
          className="p-1.5 text-[#888888] hover:text-[#dcddde] hover:bg-[#3d3d3d] rounded transition-colors"
          aria-label="Back to conversations"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        {isEditingTitle ? (
          <input
            type="text"
            value={editedTitle}
            onChange={(e) => setEditedTitle(e.target.value)}
            onBlur={handleTitleSave}
            onKeyDown={handleTitleKeyDown}
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            className="flex-1 bg-[#1e1e1e] border border-[#3d3d3d] rounded px-2 py-1 text-[#dcddde] focus:outline-none focus:border-[#7c3aed]"
            autoFocus
          />
        ) : (
          <h2
            onClick={() => {
              setEditedTitle(conversation.title || '');
              setIsEditingTitle(true);
            }}
            className="flex-1 text-[#dcddde] font-medium cursor-pointer hover:text-[#a78bfa] transition-colors truncate"
            title="Click to edit title"
          >
            {conversation.title || 'New Conversation'}
          </h2>
        )}
      </div>

      {/* Scope editor row */}
      <div className="px-4 pb-3">
        <ScopeEditor conversation={conversation} />
      </div>
    </div>
  );
}
