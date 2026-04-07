import { useEffect } from 'react';
import { useWikiStore } from '../../stores/wiki';
import { useUIStore } from '../../stores/ui';
import { WikiHeader } from './WikiHeader';
import { WikiEmptyState } from './WikiEmptyState';
import { WikiGenerating } from './WikiGenerating';
import { WikiArticleContent } from './WikiArticleContent';
import { WikiProposalDiff } from './WikiProposalDiff';

interface WikiViewerProps {
  tagId: string;
  tagName: string;
}

export function WikiViewer({ tagId, tagName }: WikiViewerProps) {
  const currentArticle = useWikiStore(s => s.currentArticle);
  const articleStatus = useWikiStore(s => s.articleStatus);
  const relatedTags = useWikiStore(s => s.relatedTags);
  const wikiLinks = useWikiStore(s => s.wikiLinks);
  const articles = useWikiStore(s => s.articles);
  const versions = useWikiStore(s => s.versions);
  const selectedVersion = useWikiStore(s => s.selectedVersion);
  const isLoading = useWikiStore(s => s.isLoading);
  const isGenerating = useWikiStore(s => s.isGenerating);
  const isUpdating = useWikiStore(s => s.isUpdating);
  const error = useWikiStore(s => s.error);
  const fetchArticle = useWikiStore(s => s.fetchArticle);
  const fetchArticleStatus = useWikiStore(s => s.fetchArticleStatus);
  const fetchRelatedTags = useWikiStore(s => s.fetchRelatedTags);
  const fetchWikiLinks = useWikiStore(s => s.fetchWikiLinks);
  const fetchVersions = useWikiStore(s => s.fetchVersions);
  const selectVersion = useWikiStore(s => s.selectVersion);
  const clearSelectedVersion = useWikiStore(s => s.clearSelectedVersion);
  const generateArticle = useWikiStore(s => s.generateArticle);
  const openArticle = useWikiStore(s => s.openArticle);
  const clearArticle = useWikiStore(s => s.clearArticle);
  const fetchAllArticles = useWikiStore(s => s.fetchAllArticles);
  const clearError = useWikiStore(s => s.clearError);
  // Proposal state + actions
  const proposal = useWikiStore(s => s.proposal);
  const isProposing = useWikiStore(s => s.isProposing);
  const isAccepting = useWikiStore(s => s.isAccepting);
  const isDismissing = useWikiStore(s => s.isDismissing);
  const reviewingProposal = useWikiStore(s => s.reviewingProposal);
  const fetchProposal = useWikiStore(s => s.fetchProposal);
  const proposeArticle = useWikiStore(s => s.proposeArticle);
  const acceptProposal = useWikiStore(s => s.acceptProposal);
  const dismissProposal = useWikiStore(s => s.dismissProposal);
  const startReviewingProposal = useWikiStore(s => s.startReviewingProposal);
  const stopReviewingProposal = useWikiStore(s => s.stopReviewingProposal);

  const closeDrawer = useUIStore(s => s.closeDrawer);
  const openDrawer = useUIStore(s => s.openDrawer);

  // Fetch article and status when component mounts or tagId changes
  useEffect(() => {
    fetchArticle(tagId);
    fetchArticleStatus(tagId);
    fetchVersions(tagId);
    fetchProposal(tagId);
    // Ensure articles list is available for implicit back-linking
    if (articles.length === 0) {
      fetchAllArticles();
    }

    // Cleanup when unmounting
    return () => {
      clearArticle();
    };
  }, [tagId, fetchArticle, fetchArticleStatus, fetchVersions, fetchProposal, clearArticle, articles.length, fetchAllArticles]);

  // Only fetch related tags and wiki links when an article exists
  useEffect(() => {
    if (currentArticle) {
      fetchRelatedTags(tagId);
      fetchWikiLinks(tagId);
    }
  }, [tagId, currentArticle, fetchRelatedTags, fetchWikiLinks]);

  const handleGenerate = () => {
    generateArticle(tagId, tagName);
  };

  const handleUpdate = () => {
    // The button that used to call update_wiki_article now generates a proposal
    // that the user reviews before it becomes live.
    proposeArticle(tagId, tagName);
  };

  const handleReviewProposal = () => {
    startReviewingProposal();
  };

  const handleAcceptProposal = () => {
    acceptProposal(tagId);
  };

  const handleDismissProposal = () => {
    dismissProposal(tagId);
  };

  const handleCancelReview = () => {
    stopReviewingProposal();
  };

  const handleRegenerate = () => {
    generateArticle(tagId, tagName);
  };

  const handleViewAtom = (atomId: string) => {
    openDrawer('viewer', atomId);
  };

  const handleNavigateToArticle = (targetTagId: string, targetTagName: string) => {
    openArticle(targetTagId, targetTagName);
  };

  // Loading state
  if (isLoading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <div className="w-8 h-8 animate-spin">
            <svg className="w-full h-full text-[var(--color-accent)]" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </div>
        </div>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center px-6 text-center">
          <div className="w-12 h-12 mb-4 rounded-full bg-red-500/10 flex items-center justify-center">
            <svg className="w-6 h-6 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <p className="text-[var(--color-text-primary)] mb-2">Failed to generate article</p>
          <p className="text-sm text-[var(--color-text-secondary)] mb-4">{error}</p>
          <button
            onClick={() => {
              clearError();
              handleGenerate();
            }}
            className="px-4 py-2 bg-[var(--color-accent)] text-white rounded-lg hover:bg-[var(--color-accent-hover)] transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // Generating state
  if (isGenerating) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <WikiGenerating tagName={tagName} atomCount={articleStatus?.current_atom_count || 0} />
      </div>
    );
  }

  // Empty state (no article exists)
  if (!currentArticle) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <WikiEmptyState
          tagName={tagName}
          atomCount={articleStatus?.current_atom_count || 0}
          onGenerate={handleGenerate}
          isGenerating={isGenerating}
        />
      </div>
    );
  }

  // Determine what content to display
  const displayArticle = selectedVersion
    ? { content: selectedVersion.content, id: selectedVersion.id, tag_id: selectedVersion.tag_id, created_at: selectedVersion.created_at, updated_at: selectedVersion.created_at, atom_count: selectedVersion.atom_count }
    : currentArticle.article;
  const displayCitations = selectedVersion
    ? selectedVersion.citations
    : currentArticle.citations;

  // Article exists - show content
  return (
    <div className="flex flex-col h-full">
      <WikiHeader
        newAtomsAvailable={selectedVersion ? 0 : (articleStatus?.new_atoms_available || 0)}
        onUpdate={handleUpdate}
        onRegenerate={handleRegenerate}
        onClose={closeDrawer}
        isUpdating={isUpdating}
        versions={versions}
        onSelectVersion={selectVersion}
        isViewingVersion={!!selectedVersion}
        onReturnToCurrent={clearSelectedVersion}
        hasProposal={!!proposal && !selectedVersion}
        isProposing={isProposing}
        proposalAtomCount={proposal?.new_atom_count || 0}
        onReviewProposal={handleReviewProposal}
      />
      {reviewingProposal && proposal && !selectedVersion ? (
        <WikiProposalDiff
          liveContent={currentArticle.article.content}
          proposalContent={proposal.content}
          newAtomCount={proposal.new_atom_count}
          createdAt={proposal.created_at}
          onAccept={handleAcceptProposal}
          onDismiss={handleDismissProposal}
          onCancel={handleCancelReview}
          isAccepting={isAccepting}
          isDismissing={isDismissing}
        />
      ) : (
        <div className="flex-1 overflow-y-auto">
          <WikiArticleContent
            article={displayArticle}
            citations={displayCitations}
            wikiLinks={selectedVersion ? [] : wikiLinks}
            relatedTags={selectedVersion ? [] : relatedTags}
            tagName={tagName}
            updatedAt={selectedVersion ? selectedVersion.created_at : currentArticle.article.updated_at}
            sourceCount={displayCitations.length}
            onViewAtom={handleViewAtom}
            onNavigateToArticle={handleNavigateToArticle}
          />
        </div>
      )}
    </div>
  );
}
