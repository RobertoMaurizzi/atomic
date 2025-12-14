import { useState } from 'react';
import { TagTree } from '../tags/TagTree';
import { SettingsButton, SettingsModal } from '../settings';

export function LeftPanel() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  return (
    <aside
      className="w-[250px] h-full bg-[var(--color-bg-panel)]/80 border-r border-[var(--color-border)] flex flex-col transition-colors duration-300 backdrop-blur-xl"
    >
      {/* Titlebar row with settings button - aligned with traffic lights */}
      <div className="h-[52px] flex items-center justify-end px-3 flex-shrink-0">
        {/* Drag region - fills available space */}
        <div data-tauri-drag-region className="flex-1 h-full drag-region" />
        <SettingsButton onClick={() => setIsSettingsOpen(true)} />
      </div>

      {/* Tag Tree with integrated search */}
      <div className="flex-1 overflow-hidden">
        <TagTree />
      </div>

      <SettingsModal isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
    </aside>
  );
}

