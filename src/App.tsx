import { Layout } from './components/layout';
import { LocalGraphView } from './components/canvas';
import { useEmbeddingEvents } from './hooks';
import { useUIStore } from './stores/ui';

function App() {
  // Initialize embedding event listener
  useEmbeddingEvents();

  const { openDrawer } = useUIStore();

  const handleAtomClick = (atomId: string) => {
    openDrawer('viewer', atomId);
  };

  return (
    <>
      <Layout />
      <LocalGraphView onAtomClick={handleAtomClick} />
    </>
  );
}

export default App;

