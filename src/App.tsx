import { useCallback, useEffect } from 'react';
import { Layout } from './components/layout';
import { LocalGraphView } from './components/canvas';
import { useEmbeddingEvents } from './hooks';
import { useUIStore } from './stores/ui';

function App() {
  // Initialize embedding event listener
  useEmbeddingEvents();

  // Listen for auth expiry (token revoked or invalid)
  useEffect(() => {
    const handler = () => {
      // Reload triggers initTransport() which sees no saved config → setup mode
      window.location.reload();
    };
    window.addEventListener('atomic:auth-expired', handler);
    return () => window.removeEventListener('atomic:auth-expired', handler);
  }, []);

  const openDrawer = useUIStore(s => s.openDrawer);

  const handleAtomClick = useCallback((atomId: string) => {
    openDrawer('viewer', atomId);
  }, [openDrawer]);

  return (
    <>
      <Layout />
      <LocalGraphView onAtomClick={handleAtomClick} />
    </>
  );
}

export default App;

