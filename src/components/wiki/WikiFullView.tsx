import { useEffect, useRef, useState } from 'react';
import { useWikiStore } from '../../stores/wiki';
import { useUIStore } from '../../stores/ui';
import { WikiArticlesList } from './WikiArticlesList';
import { WikiEmptyState } from './WikiEmptyState';
import { WikiGenerating } from './WikiGenerating';
import { WikiArticleContent } from './WikiArticleContent';
import { WikiProposalDiff } from './WikiProposalDiff';
import { Button } from '../ui/Button';
import { Modal } from '../ui/Modal';
import { formatRelativeTime } from '../../lib/date';

export function WikiFullView() {
  const view = useWikiStore(s => s.view);
  const currentTagId = useWikiStore(s => s.currentTagId);
  const currentTagName = useWikiStore(s => s.currentTagName);
  const currentArticle = useWikiStore(s => s.currentArticle);
  const articleStatus = useWikiStore(s => s.articleStatus);
  const relatedTags = useWikiStore(s => s.relatedTags);
  const wikiLinks = useWikiStore(s => s.wikiLinks);
  const isLoading = useWikiStore(s => s.isLoading);
  const isGenerating = useWikiStore(s => s.isGenerating);
  const isUpdating = useWikiStore(s => s.isUpdating);
  const error = useWikiStore(s => s.error);
  const fetchAllArticles = useWikiStore(s => s.fetchAllArticles);
  const generateArticle = useWikiStore(s => s.generateArticle);
  const openArticle = useWikiStore(s => s.openArticle);
  const goBack = useWikiStore(s => s.goBack);
  const clearError = useWikiStore(s => s.clearError);

  // Version history
  const versions = useWikiStore(s => s.versions);
  const selectedVersion = useWikiStore(s => s.selectedVersion);
  const selectVersion = useWikiStore(s => s.selectVersion);
  const clearSelectedVersion = useWikiStore(s => s.clearSelectedVersion);

  // Proposal state
  const proposal = useWikiStore(s => s.proposal);
  const isProposing = useWikiStore(s => s.isProposing);
  const isAccepting = useWikiStore(s => s.isAccepting);
  const isDismissing = useWikiStore(s => s.isDismissing);
  const reviewingProposal = useWikiStore(s => s.reviewingProposal);
  const proposeArticle = useWikiStore(s => s.proposeArticle);
  const acceptProposal = useWikiStore(s => s.acceptProposal);
  const dismissProposal = useWikiStore(s => s.dismissProposal);
  const startReviewingProposal = useWikiStore(s => s.startReviewingProposal);
  const stopReviewingProposal = useWikiStore(s => s.stopReviewingProposal);

  const reset = useWikiStore(s => s.reset);

  const openDrawer = useUIStore(s => s.openDrawer);

  const [showRegenerateModal, setShowRegenerateModal] = useState(false);
  const [showVersions, setShowVersions] = useState(false);
  const versionsRef = useRef<HTMLDivElement>(null);
  const initializedRef = useRef(false);

  useEffect(() => {
    if (initializedRef.current) return;
    initializedRef.current = true;
    fetchAllArticles();
  }, [fetchAllArticles]);

  // Clean up wiki store state on unmount
  useEffect(() => {
    return () => { reset(); };
  }, [reset]);

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

  const handleGenerate = () => {
    if (currentTagId && currentTagName) {
      generateArticle(currentTagId, currentTagName);
    }
  };

  const handleUpdate = () => {
    if (currentTagId && currentTagName) {
      proposeArticle(currentTagId, currentTagName);
    }
  };

  const handleViewAtom = (atomId: string) => {
    openDrawer('viewer', atomId);
  };

  const renderArticleContent = () => {
    if (view === 'list' || !currentTagId) {
      return (
        <div className="flex flex-col items-center justify-center h-full text-[var(--color-text-secondary)] gap-3 p-8">
          <svg className="w-12 h-12 opacity-40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
          </svg>
          <p className="text-sm">Select an article to read</p>
        </div>
      );
    }

    if (isLoading) {
      return (
        <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
          Loading...
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex flex-col items-center justify-center h-full gap-4 p-4">
          <p className="text-red-400 text-sm">{error}</p>
          <button onClick={clearError} className="text-xs text-[var(--color-accent)] hover:underline">
            Dismiss
          </button>
        </div>
      );
    }

    if (isGenerating) {
      return <WikiGenerating tagName={currentTagName || ''} atomCount={articleStatus?.current_atom_count || 0} />;
    }

    if (!currentArticle) {
      return (
        <WikiEmptyState
          tagName={currentTagName || ''}
          atomCount={articleStatus?.current_atom_count || 0}
          onGenerate={handleGenerate}
          isGenerating={false}
        />
      );
    }

    const displayArticle = selectedVersion
      ? { content: selectedVersion.content, id: selectedVersion.id, tag_id: selectedVersion.tag_id, created_at: selectedVersion.created_at, updated_at: selectedVersion.created_at, atom_count: selectedVersion.atom_count }
      : currentArticle.article;
    const displayCitations = selectedVersion
      ? selectedVersion.citations
      : currentArticle.citations;

    return (
      <div className="h-full flex flex-col overflow-hidden">
        {/* Version viewing banner */}
        {selectedVersion && (
          <div className="flex items-center justify-between px-6 py-2 bg-amber-500/10 border-b border-amber-500/20 flex-shrink-0">
            <span className="text-sm text-amber-400">Viewing previous version</span>
            <button onClick={clearSelectedVersion} className="text-sm text-amber-400 hover:text-amber-300 underline transition-colors">
              Return to current
            </button>
          </div>
        )}

        {/* Proposal ready banner */}
        {!!proposal && !selectedVersion && (
          <div className="flex items-center justify-between px-6 py-2 bg-[var(--color-accent)]/15 border-b border-[var(--color-accent)]/30 flex-shrink-0">
            <span className="text-sm text-[var(--color-accent-light)]">
              Suggested update ready
              {proposal.new_atom_count > 0 && (
                <> — based on {proposal.new_atom_count} new atom{proposal.new_atom_count !== 1 ? 's' : ''}</>
              )}
            </span>
            <Button variant="primary" size="sm" onClick={startReviewingProposal}>Review</Button>
          </div>
        )}

        {/* New atoms available banner */}
        {!proposal && !selectedVersion && (articleStatus?.new_atoms_available || 0) > 0 && (
          <div className="flex items-center justify-between px-6 py-2 bg-[var(--color-accent)]/10 border-b border-[var(--color-accent)]/20 flex-shrink-0">
            <span className="text-sm text-[var(--color-accent-light)]">
              {articleStatus!.new_atoms_available} new atom{articleStatus!.new_atoms_available !== 1 ? 's' : ''} available
            </span>
            <Button variant="primary" size="sm" onClick={handleUpdate} disabled={isProposing || isUpdating}>
              {isProposing ? 'Generating...' : 'Generate update'}
            </Button>
          </div>
        )}

        {/* Proposal diff view */}
        {reviewingProposal && proposal && !selectedVersion ? (
          <WikiProposalDiff
            liveContent={currentArticle.article.content}
            proposalContent={proposal.content}
            newAtomCount={proposal.new_atom_count}
            createdAt={proposal.created_at}
            onAccept={() => currentTagId && acceptProposal(currentTagId)}
            onDismiss={() => currentTagId && dismissProposal(currentTagId)}
            onCancel={stopReviewingProposal}
            isAccepting={isAccepting}
            isDismissing={isDismissing}
          />
        ) : (
          <div className="flex-1 overflow-y-auto scrollbar-auto-hide">
            <WikiArticleContent
              article={displayArticle}
              citations={displayCitations}
              wikiLinks={selectedVersion ? [] : wikiLinks}
              relatedTags={selectedVersion ? [] : relatedTags}
              tagName={currentTagName || ''}
              updatedAt={selectedVersion ? selectedVersion.created_at : currentArticle.article.updated_at}
              sourceCount={displayCitations.length}
              titleActions={
                <>
                  {/* Version history */}
                  {versions.length > 0 && (
                    <div className="relative" ref={versionsRef}>
                      <Button variant="ghost" size="sm" onClick={() => setShowVersions(!showVersions)}>
                        <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                        </svg>
                        {versions.length}
                      </Button>
                      {showVersions && (
                        <div className="absolute right-0 top-full mt-1 w-64 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg shadow-lg z-50 py-1 max-h-64 overflow-y-auto">
                          {selectedVersion && (
                            <button
                              onClick={() => { clearSelectedVersion(); setShowVersions(false); }}
                              className="w-full text-left px-3 py-2 text-sm hover:bg-[var(--color-bg-hover)] transition-colors text-[var(--color-accent-light)] font-medium"
                            >
                              Current version
                            </button>
                          )}
                          {versions.map((v) => (
                            <button
                              key={v.id}
                              onClick={() => { selectVersion(v.id); setShowVersions(false); }}
                              className="w-full text-left px-3 py-2 text-sm hover:bg-[var(--color-bg-hover)] transition-colors"
                            >
                              <div className="text-[var(--color-text-primary)]">Version {v.version_number}</div>
                              <div className="text-xs text-[var(--color-text-secondary)]">
                                {formatRelativeTime(v.created_at)} • {v.atom_count} source{v.atom_count !== 1 ? 's' : ''}
                              </div>
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                  {/* Regenerate */}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => setShowRegenerateModal(true)}
                    disabled={isUpdating || !!selectedVersion}
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                    </svg>
                  </Button>
                  {/* Mobile back */}
                  <button
                    onClick={goBack}
                    className="md:hidden text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors p-1"
                  >
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </>
              }
              onViewAtom={handleViewAtom}
              onNavigateToArticle={(tagId, tagName) => openArticle(tagId, tagName)}
            />
          </div>
        )}

        {/* Regenerate confirmation modal */}
        <Modal
          isOpen={showRegenerateModal}
          onClose={() => setShowRegenerateModal(false)}
          title="Regenerate Article"
          confirmLabel="Regenerate"
          confirmVariant="primary"
          onConfirm={() => {
            setShowRegenerateModal(false);
            handleGenerate();
          }}
        >
          <p className="text-[var(--color-text-primary)]">
            This will regenerate the article from scratch, replacing the current content.
            The current version will be saved in the version history.
            Are you sure you want to continue?
          </p>
        </Modal>
      </div>
    );
  };

  // On mobile (no sidebar), show the article list when nothing is selected
  const showMobileList = !currentTagId || view === 'list';

  return (
    <div className="h-full overflow-hidden">
      {/* On mobile, swap between list and article */}
      <div className="md:hidden h-full">
        {showMobileList ? (
          <WikiArticlesList />
        ) : (
          renderArticleContent()
        )}
      </div>
      <div className="hidden md:block h-full">
        {renderArticleContent()}
      </div>
    </div>
  );
}
