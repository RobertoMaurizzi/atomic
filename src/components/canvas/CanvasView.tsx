import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import { TransformWrapper, TransformComponent } from 'react-zoom-pan-pinch';
import { AtomWithTags } from '../../stores/atoms';
import { CanvasContent } from './CanvasContent';
import { CanvasControls } from './CanvasControls';
import { useForceSimulation, buildConnections, SimulationNode } from './useForceSimulation';
import {
  getAtomPositions,
  saveAtomPositions,
  getAtomsWithEmbeddings,
  AtomPosition,
} from '../../lib/tauri';

const CANVAS_CENTER = 2500;

interface CanvasViewProps {
  atoms: AtomWithTags[];
  selectedTagId: string | null;
  searchResultIds: string[] | null; // atom IDs matching search, null = not searching
  onAtomClick: (atomId: string) => void;
}

export function CanvasView({
  atoms,
  selectedTagId,
  searchResultIds,
  onAtomClick,
}: CanvasViewProps) {
  const [positions, setPositions] = useState<Map<string, { x: number; y: number }>>(
    new Map()
  );
  const [embeddings, setEmbeddings] = useState<Map<string, number[]>>(new Map());
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const hasLoadedRef = useRef(false);

  // Build connections from shared tags
  const connections = useMemo(() => buildConnections(atoms), [atoms]);

  // Load positions and embeddings on mount
  useEffect(() => {
    if (hasLoadedRef.current) return;
    hasLoadedRef.current = true;

    async function loadData() {
      try {
        setIsLoading(true);
        setError(null);

        // Load positions and embeddings in parallel
        const [positionsData, embeddingsData] = await Promise.all([
          getAtomPositions(),
          getAtomsWithEmbeddings(),
        ]);

        // Convert positions to map
        const posMap = new Map<string, { x: number; y: number }>();
        for (const pos of positionsData) {
          posMap.set(pos.atom_id, { x: pos.x, y: pos.y });
        }
        setPositions(posMap);

        // Convert embeddings to map
        const embMap = new Map<string, number[]>();
        for (const atom of embeddingsData) {
          if (atom.embedding) {
            embMap.set(atom.id, atom.embedding);
          }
        }
        setEmbeddings(embMap);

        setIsLoading(false);
      } catch (err) {
        console.error('Failed to load canvas data:', err);
        setError(String(err));
        setIsLoading(false);
      }
    }

    loadData();
  }, []);

  // Handle simulation end - save positions
  const handleSimulationEnd = useCallback(async (nodes: SimulationNode[]) => {
    try {
      const positionsToSave: AtomPosition[] = nodes.map((node) => ({
        atom_id: node.id,
        x: node.x,
        y: node.y,
      }));
      await saveAtomPositions(positionsToSave);

      // Update local positions map
      const newPositions = new Map<string, { x: number; y: number }>();
      for (const node of nodes) {
        newPositions.set(node.id, { x: node.x, y: node.y });
      }
      setPositions(newPositions);
    } catch (err) {
      console.error('Failed to save positions:', err);
    }
  }, []);

  // Run force simulation
  const { nodes, isSimulating } = useForceSimulation({
    atoms,
    embeddings,
    existingPositions: positions,
    connections,
    enabled: !isLoading && atoms.length > 0,
    onSimulationEnd: handleSimulationEnd,
  });

  // Calculate faded atom IDs based on tag filter and search
  const fadedAtomIds = useMemo(() => {
    const faded = new Set<string>();

    // If searching, fade non-matching atoms
    if (searchResultIds !== null) {
      const matchingIds = new Set(searchResultIds);
      for (const atom of atoms) {
        if (!matchingIds.has(atom.id)) {
          faded.add(atom.id);
        }
      }
      return faded;
    }

    // If tag is selected, fade non-matching atoms
    if (selectedTagId) {
      for (const atom of atoms) {
        const hasTag = atom.tags.some((tag) => tag.id === selectedTagId);
        if (!hasTag) {
          faded.add(atom.id);
        }
      }
    }

    return faded;
  }, [atoms, selectedTagId, searchResultIds]);

  // Calculate initial transform to center on content
  const initialTransform = useMemo(() => {
    if (nodes.length === 0) {
      return { x: -CANVAS_CENTER + 400, y: -CANVAS_CENTER + 300, scale: 1 };
    }

    // Find bounding box of all nodes
    let minX = Infinity,
      maxX = -Infinity,
      minY = Infinity,
      maxY = -Infinity;
    for (const node of nodes) {
      minX = Math.min(minX, node.x);
      maxX = Math.max(maxX, node.x);
      minY = Math.min(minY, node.y);
      maxY = Math.max(maxY, node.y);
    }

    // Add padding
    const padding = 100;
    minX -= padding;
    maxX += padding;
    minY -= padding;
    maxY += padding;

    // Calculate center of content
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;

    // Calculate scale to fit content (assuming viewport is ~800x600)
    const contentWidth = maxX - minX;
    const contentHeight = maxY - minY;
    const scaleX = 800 / contentWidth;
    const scaleY = 600 / contentHeight;
    const scale = Math.min(scaleX, scaleY, 1); // Don't zoom in past 1x

    return {
      x: -centerX * scale + 400,
      y: -centerY * scale + 300,
      scale,
    };
  }, [nodes]);

  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[#1e1e1e]">
        <div className="flex items-center gap-3 text-[#888888]">
          <svg className="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
          Loading canvas...
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[#1e1e1e]">
        <div className="text-red-500">Error: {error}</div>
      </div>
    );
  }

  if (atoms.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[#1e1e1e]">
        <div className="text-[#888888]">No atoms to display</div>
      </div>
    );
  }

  return (
    <div className="flex-1 relative overflow-hidden bg-[#1e1e1e]">
      {/* Simulation loading overlay */}
      {isSimulating && (
        <div className="absolute top-4 left-1/2 -translate-x-1/2 z-20 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md px-4 py-2 flex items-center gap-2">
          <svg className="w-4 h-4 animate-spin text-[#888888]" fill="none" viewBox="0 0 24 24">
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
          <span className="text-sm text-[#888888]">Calculating positions...</span>
        </div>
      )}

      <TransformWrapper
        initialScale={initialTransform.scale}
        initialPositionX={initialTransform.x}
        initialPositionY={initialTransform.y}
        minScale={0.1}
        maxScale={2}
        limitToBounds={false}
        panning={{ velocityDisabled: true }}
      >
        <CanvasControls />
        <TransformComponent
          wrapperStyle={{
            width: '100%',
            height: '100%',
          }}
        >
          <CanvasContent
            nodes={nodes}
            connections={connections}
            fadedAtomIds={fadedAtomIds}
            onAtomClick={onAtomClick}
          />
        </TransformComponent>
      </TransformWrapper>
    </div>
  );
}

