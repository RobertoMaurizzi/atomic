import { useState, useRef, useEffect } from 'react';
import { Button } from '../ui/Button';
import { Modal } from '../ui/Modal';
import { formatRelativeTime } from '../../lib/date';
import type { WikiVersionSummary } from '../../stores/wiki';

interface WikiHeaderProps {
  newAtomsAvailable: number;
  onUpdate: () => void;
  onRegenerate: () => void;
  onClose: () => void;
  isUpdating: boolean;
  onBack?: () => void;
  versions: WikiVersionSummary[];
  onSelectVersion: (versionId: string) => void;
  isViewingVersion: boolean;
  onReturnToCurrent: () => void;
  // Proposal review state
  hasProposal: boolean;
  isProposing: boolean;
  proposalAtomCount: number;
  onReviewProposal: () => void;
}

export function WikiHeader({
  newAtomsAvailable,
  onUpdate,
  onRegenerate,
  onClose,
  isUpdating,
  onBack,
  versions,
  onSelectVersion,
  isViewingVersion,
  onReturnToCurrent,
  hasProposal,
  isProposing,
  proposalAtomCount,
  onReviewProposal,
}: WikiHeaderProps) {
  const [showRegenerateModal, setShowRegenerateModal] = useState(false);
  const [showVersions, setShowVersions] = useState(false);
  const versionsRef = useRef<HTMLDivElement>(null);

  const handleRegenerate = () => {
    setShowRegenerateModal(false);
    onRegenerate();
  };

  // Close versions dropdown on outside click
  useEffect(() => {
    if (!showVersions) return;
    const handleClick = (e: MouseEvent) => {
      if (versionsRef.current && !versionsRef.current.contains(e.target as Node)) {
        setShowVersions(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [showVersions]);

  return (
    <div className="border-b border-[var(--color-border)]">
      {/* Version viewing banner */}
      {isViewingVersion && (
        <div className="flex items-center justify-between px-6 py-2 bg-amber-500/10 border-b border-amber-500/20">
          <span className="text-sm text-amber-400">
            Viewing previous version
          </span>
          <button
            onClick={onReturnToCurrent}
            className="text-sm text-amber-400 hover:text-amber-300 underline transition-colors"
          >
            Return to current
          </button>
        </div>
      )}

      {/* Compact toolbar */}
      <div className="flex items-center justify-end px-4 py-1.5">
        <div className="flex items-center gap-1">
          {onBack && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onBack}
            >
              <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
              Back
            </Button>
          )}
          {/* Version history button */}
          {versions.length > 0 && (
            <div className="relative" ref={versionsRef}>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowVersions(!showVersions)}
              >
                <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                History ({versions.length})
              </Button>
              {showVersions && (
                <div className="absolute right-0 top-full mt-1 w-64 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg shadow-lg z-50 py-1 max-h-64 overflow-y-auto">
                  {isViewingVersion && (
                    <button
                      onClick={() => {
                        onReturnToCurrent();
                        setShowVersions(false);
                      }}
                      className="w-full text-left px-3 py-2 text-sm hover:bg-[var(--color-bg-hover)] transition-colors text-[var(--color-accent-light)] font-medium"
                    >
                      Current version
                    </button>
                  )}
                  {versions.map((v) => (
                    <button
                      key={v.id}
                      onClick={() => {
                        onSelectVersion(v.id);
                        setShowVersions(false);
                      }}
                      className="w-full text-left px-3 py-2 text-sm hover:bg-[var(--color-bg-hover)] transition-colors"
                    >
                      <div className="text-[var(--color-text-primary)]">
                        Version {v.version_number}
                      </div>
                      <div className="text-xs text-[var(--color-text-secondary)]">
                        {formatRelativeTime(v.created_at)} • {v.atom_count} source{v.atom_count !== 1 ? 's' : ''}
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowRegenerateModal(true)}
            disabled={isUpdating || isViewingVersion}
          >
            <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
            Regenerate
          </Button>
          <button
            onClick={onClose}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors p-1"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Proposal ready banner — takes precedence over the new-atoms banner */}
      {hasProposal && !isViewingVersion && (
        <div className="flex items-center justify-between px-6 py-2 bg-[var(--color-accent)]/15 border-t border-[var(--color-accent)]/30">
          <span className="text-sm text-[var(--color-accent-light)]">
            Suggested update ready
            {proposalAtomCount > 0 && (
              <> — based on {proposalAtomCount} new atom{proposalAtomCount !== 1 ? 's' : ''}</>
            )}
          </span>
          <Button
            variant="primary"
            size="sm"
            onClick={onReviewProposal}
          >
            Review
          </Button>
        </div>
      )}

      {/* New atoms banner — only when no proposal is pending */}
      {!hasProposal && newAtomsAvailable > 0 && !isViewingVersion && (
        <div className="flex items-center justify-between px-6 py-2 bg-[var(--color-accent)]/10 border-t border-[var(--color-accent)]/20">
          <span className="text-sm text-[var(--color-accent-light)]">
            {newAtomsAvailable} new atom{newAtomsAvailable !== 1 ? 's' : ''} available
          </span>
          <Button
            variant="primary"
            size="sm"
            onClick={onUpdate}
            disabled={isProposing || isUpdating}
          >
            {isProposing ? (
              <>
                <svg className="w-3 h-3 mr-1 animate-spin" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                </svg>
                Generating...
              </>
            ) : (
              'Generate update'
            )}
          </Button>
        </div>
      )}

      {/* Regenerate confirmation modal */}
      <Modal
        isOpen={showRegenerateModal}
        onClose={() => setShowRegenerateModal(false)}
        title="Regenerate Article"
        confirmLabel="Regenerate"
        confirmVariant="primary"
        onConfirm={handleRegenerate}
      >
        <p className="text-[var(--color-text-primary)]">
          This will regenerate the article from scratch, replacing the current content.
          The current version will be saved in the version history.
          Are you sure you want to continue?
        </p>
      </Modal>
    </div>
  );
}
