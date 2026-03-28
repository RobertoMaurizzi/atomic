import { useState, useEffect } from 'react';
import { Button } from '../../ui/Button';
import { QRCode } from '../QRCode';
import {
  getMcpConfig,
  createApiToken,
  createFeed,
  ingestUrl as apiIngestUrl,
  importObsidianVault,
  type McpConfig,
  type ImportResult,
  type IngestionResult,
} from '../../../lib/api';
import { isDesktopApp, getLocalServerConfig, getTransport } from '../../../lib/transport';
import type { HttpTransport } from '../../../lib/transport/http';
import { pickDirectory } from '../../../lib/platform';
import type { OnboardingState, OnboardingAction } from '../useOnboardingState';

function copyToClipboard(text: string) {
  if (navigator.clipboard && window.isSecureContext) {
    return navigator.clipboard.writeText(text);
  }
  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand('copy');
  document.body.removeChild(textarea);
  return Promise.resolve();
}

function getServerInfo() {
  if (isDesktopApp()) {
    const localConfig = getLocalServerConfig();
    return {
      url: localConfig?.baseUrl || 'http://127.0.0.1:44380',
      token: localConfig?.authToken || '',
    };
  }
  const transport = getTransport() as HttpTransport;
  const config = transport.getConfig();
  return { url: config.baseUrl, token: config.authToken };
}

// --- Collapsible section wrapper ---

function Section({
  title,
  description,
  isOpen,
  onToggle,
  children,
}: {
  title: string;
  description: string;
  isOpen: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border border-[var(--color-border)] rounded-lg overflow-hidden">
      <button
        onClick={onToggle}
        className="w-full flex items-center justify-between p-4 bg-[var(--color-bg-card)] hover:bg-[var(--color-bg-hover)] transition-colors text-left"
      >
        <div>
          <h3 className="text-sm font-medium text-[var(--color-text-primary)]">{title}</h3>
          <p className="text-xs text-[var(--color-text-secondary)]">{description}</p>
        </div>
        <svg
          className={`w-4 h-4 text-[var(--color-text-secondary)] transition-transform duration-200 shrink-0 ml-3 ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {isOpen && <div className="p-4 border-t border-[var(--color-border)] space-y-3">{children}</div>}
    </div>
  );
}

// --- MCP content ---

function McpContent() {
  const [mcpConfig, setMcpConfig] = useState<McpConfig | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const { url } = getServerInfo();
    setMcpConfig(getMcpConfig(url));
  }, []);

  const handleCopy = async () => {
    if (!mcpConfig) return;
    await copyToClipboard(JSON.stringify(mcpConfig, null, 2));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const configJson = mcpConfig ? JSON.stringify(mcpConfig, null, 2) : '';

  return (
    <>
      <ol className="space-y-1.5 text-sm text-[var(--color-text-secondary)] list-decimal list-inside">
        <li>Open Claude Desktop settings</li>
        <li>Navigate to <span className="text-[var(--color-text-primary)]">Developer &gt; Edit Config</span></li>
        <li>Add the following to your configuration file:</li>
      </ol>
      <div className="relative">
        <pre className="p-3 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-lg text-xs text-[var(--color-text-primary)] overflow-x-auto font-mono">
          {configJson}
        </pre>
        <Button variant="secondary" size="sm" onClick={handleCopy} className="absolute top-2 right-2">
          {copied ? 'Copied!' : 'Copy'}
        </Button>
      </div>
      <p className="text-xs text-[var(--color-text-secondary)]">
        After saving, restart Claude Desktop. Atomic will appear as an available MCP tool.
      </p>
    </>
  );
}

// --- Mobile content ---

function MobileContent({
  state,
  dispatch,
}: {
  state: OnboardingState;
  dispatch: React.Dispatch<OnboardingAction>;
}) {
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const { url } = getServerInfo();

  const handleGenerateQR = async () => {
    setIsGenerating(true);
    setError(null);
    try {
      const result = await createApiToken('mobile-setup');
      dispatch({ type: 'SET_MOBILE_TOKEN', token: result.token });
    } catch (e) {
      setError(String(e));
    } finally {
      setIsGenerating(false);
    }
  };

  const qrPayload = state.mobileToken
    ? JSON.stringify({ url, token: state.mobileToken })
    : null;

  const handleCopy = async () => {
    if (!qrPayload) return;
    await copyToClipboard(qrPayload);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (!state.mobileToken) {
    return (
      <>
        <p className="text-sm text-[var(--color-text-secondary)]">
          Generate a QR code to connect the Atomic iOS app instantly.
        </p>
        <Button variant="secondary" onClick={handleGenerateQR} disabled={isGenerating}>
          {isGenerating ? 'Generating...' : 'Generate QR Code'}
        </Button>
        {error && <p className="text-sm text-red-500">{error}</p>}
      </>
    );
  }

  return (
    <>
      <div className="flex flex-col items-center space-y-3">
        <div className="p-4 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg">
          <QRCode value={qrPayload!} size={180} />
        </div>
        <p className="text-xs text-[var(--color-text-secondary)] text-center">
          Open the Atomic iOS app and tap <strong className="text-[var(--color-text-primary)]">Scan QR Code</strong>
        </p>
      </div>
      <div className="flex gap-2">
        <code className="flex-1 px-3 py-2 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-xs text-[var(--color-text-primary)] truncate">
          {url}
        </code>
        <Button variant="secondary" size="sm" onClick={handleCopy}>
          {copied ? 'Copied!' : 'Copy'}
        </Button>
      </div>
    </>
  );
}

// --- Extension content ---

function ExtensionContent() {
  const [copiedUrl, setCopiedUrl] = useState(false);
  const [copiedToken, setCopiedToken] = useState(false);
  const serverInfo = getServerInfo();

  return (
    <>
      <ol className="space-y-1.5 text-sm text-[var(--color-text-secondary)] list-decimal list-inside">
        <li>Install the Atomic browser extension</li>
        <li>Click the extension icon and open settings</li>
        <li>Enter the server URL and auth token below</li>
      </ol>
      <div className="space-y-2">
        <div className="space-y-1">
          <label className="block text-xs font-medium text-[var(--color-text-secondary)]">Server URL</label>
          <div className="flex gap-2">
            <code className="flex-1 px-3 py-2 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-xs text-[var(--color-text-primary)] truncate">
              {serverInfo.url}
            </code>
            <Button
              variant="secondary"
              size="sm"
              onClick={async () => {
                await copyToClipboard(serverInfo.url);
                setCopiedUrl(true);
                setTimeout(() => setCopiedUrl(false), 2000);
              }}
            >
              {copiedUrl ? 'Copied!' : 'Copy'}
            </Button>
          </div>
        </div>
        <div className="space-y-1">
          <label className="block text-xs font-medium text-[var(--color-text-secondary)]">Auth Token</label>
          <div className="flex gap-2">
            <code className="flex-1 px-3 py-2 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-xs text-[var(--color-text-primary)] truncate">
              {serverInfo.token ? `${serverInfo.token.substring(0, 12)}...` : 'N/A'}
            </code>
            <Button
              variant="secondary"
              size="sm"
              disabled={!serverInfo.token}
              onClick={async () => {
                await copyToClipboard(serverInfo.token);
                setCopiedToken(true);
                setTimeout(() => setCopiedToken(false), 2000);
              }}
            >
              {copiedToken ? 'Copied!' : 'Copy'}
            </Button>
          </div>
        </div>
      </div>
    </>
  );
}

// --- Data loading content ---

function DataLoadingContent({
  state,
  dispatch,
}: {
  state: OnboardingState;
  dispatch: React.Dispatch<OnboardingAction>;
}) {
  const isDesktop = isDesktopApp();

  const [addingFeed, setAddingFeed] = useState(false);
  const [feedAdded, setFeedAdded] = useState(false);
  const [feedError, setFeedError] = useState<string | null>(null);

  const [ingesting, setIngesting] = useState(false);
  const [ingestResult, setIngestResult] = useState<IngestionResult | null>(null);
  const [ingestError, setIngestError] = useState<string | null>(null);

  const [isImporting, setIsImporting] = useState(false);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const [importError, setImportError] = useState<string | null>(null);

  const handleAddFeed = async () => {
    if (!state.feedUrl.trim() || addingFeed) return;
    setAddingFeed(true);
    setFeedError(null);
    try {
      await createFeed(state.feedUrl.trim());
      setFeedAdded(true);
      dispatch({ type: 'SET_FEED_URL', value: '' });
    } catch (e) {
      setFeedError(String(e));
    } finally {
      setAddingFeed(false);
    }
  };

  const handleIngestUrl = async () => {
    if (!state.ingestUrl.trim() || ingesting) return;
    setIngesting(true);
    setIngestResult(null);
    setIngestError(null);
    try {
      const result = await apiIngestUrl(state.ingestUrl.trim());
      setIngestResult(result);
      dispatch({ type: 'SET_INGEST_URL', value: '' });
    } catch (e) {
      setIngestError(String(e));
    } finally {
      setIngesting(false);
    }
  };

  const handleObsidianImport = async () => {
    setImportResult(null);
    setImportError(null);
    try {
      const selected = await pickDirectory('Select Obsidian Vault');
      if (!selected) return;
      setIsImporting(true);
      const result = await importObsidianVault(selected);
      setImportResult(result);
    } catch (e) {
      setImportError(String(e));
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <>
      {/* RSS Feed */}
      <div>
        <label className="block text-xs font-medium text-[var(--color-text-secondary)] mb-1.5">RSS Feed</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={state.feedUrl}
            onChange={(e) => dispatch({ type: 'SET_FEED_URL', value: e.target.value })}
            placeholder="https://example.com/feed.xml"
            className="flex-1 px-3 py-2 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent text-sm"
          />
          <Button variant="secondary" onClick={handleAddFeed} disabled={!state.feedUrl.trim() || addingFeed}>
            {addingFeed ? 'Adding...' : 'Add'}
          </Button>
        </div>
        {feedAdded && <p className="text-xs text-green-500 mt-1">Feed added successfully</p>}
        {feedError && <p className="text-xs text-red-500 mt-1">{feedError}</p>}
      </div>

      {/* URL Ingest */}
      <div>
        <label className="block text-xs font-medium text-[var(--color-text-secondary)] mb-1.5">Ingest URL</label>
        <div className="flex gap-2">
          <input
            type="text"
            value={state.ingestUrl}
            onChange={(e) => dispatch({ type: 'SET_INGEST_URL', value: e.target.value })}
            placeholder="https://example.com/article"
            className="flex-1 px-3 py-2 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent text-sm"
          />
          <Button variant="secondary" onClick={handleIngestUrl} disabled={!state.ingestUrl.trim() || ingesting}>
            {ingesting ? 'Ingesting...' : 'Ingest'}
          </Button>
        </div>
        {ingestResult && <p className="text-xs text-green-500 mt-1">Ingested: {ingestResult.title}</p>}
        {ingestError && <p className="text-xs text-red-500 mt-1">{ingestError}</p>}
      </div>

      {/* Obsidian Import (desktop only) */}
      {isDesktop && (
        <div>
          <label className="block text-xs font-medium text-[var(--color-text-secondary)] mb-1.5">Import from Obsidian</label>
          <Button variant="secondary" onClick={handleObsidianImport} disabled={isImporting}>
            {isImporting ? 'Importing...' : 'Select Vault Folder'}
          </Button>
          {importResult && (
            <p className="text-xs text-green-500 mt-1">
              Imported {importResult.imported} notes ({importResult.skipped} skipped)
            </p>
          )}
          {importError && <p className="text-xs text-red-500 mt-1">{importError}</p>}
        </div>
      )}
    </>
  );
}

// --- Main component ---

interface IntegrationsStepProps {
  state: OnboardingState;
  dispatch: React.Dispatch<OnboardingAction>;
}

export function IntegrationsStep({ state, dispatch }: IntegrationsStepProps) {
  const [openSection, setOpenSection] = useState<string | null>(null);

  const toggle = (id: string) => setOpenSection(prev => (prev === id ? null : id));

  return (
    <div className="space-y-3 px-2">
      <div className="text-center mb-4">
        <h2 className="text-xl font-bold text-[var(--color-text-primary)] mb-1">Integrations & Data</h2>
        <p className="text-sm text-[var(--color-text-secondary)]">
          Set up optional integrations and import data. You can always configure these later in Settings.
        </p>
      </div>

      <Section
        title="Claude Desktop (MCP)"
        description="Let Claude search your knowledge base"
        isOpen={openSection === 'mcp'}
        onToggle={() => toggle('mcp')}
      >
        <McpContent />
      </Section>

      <Section
        title="Mobile App"
        description="Connect the Atomic iOS app via QR code"
        isOpen={openSection === 'mobile'}
        onToggle={() => toggle('mobile')}
      >
        <MobileContent state={state} dispatch={dispatch} />
      </Section>

      <Section
        title="Browser Extension"
        description="Save web pages to your knowledge base"
        isOpen={openSection === 'extension'}
        onToggle={() => toggle('extension')}
      >
        <ExtensionContent />
      </Section>

      <Section
        title="Import Data"
        description="RSS feeds, URLs, or Obsidian vault"
        isOpen={openSection === 'data'}
        onToggle={() => toggle('data')}
      >
        <DataLoadingContent state={state} dispatch={dispatch} />
      </Section>
    </div>
  );
}
