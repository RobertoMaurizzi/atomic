import { useMemo, useCallback } from 'react';
import { AtomNode } from './AtomNode';
import { ConnectionLines, Connection } from './ConnectionLines';
import { SimulationNode } from './useForceSimulation';

const CANVAS_SIZE = 5000;
const HUB_THRESHOLD = 8; // Atoms with this many or more connections are hubs

interface CanvasContentProps {
  nodes: SimulationNode[];
  connections: Connection[];
  fadedAtomIds: Set<string>;
  connectionCounts: Record<string, number>;
  highlightedAtomId: string | null;
  onAtomClick: (atomId: string) => void;
}

export function CanvasContent({
  nodes,
  connections,
  fadedAtomIds,
  connectionCounts,
  highlightedAtomId,
  onAtomClick,
}: CanvasContentProps) {
  // Stable onClick handler to prevent AtomNode re-renders
  const handleAtomClick = useCallback((atomId: string) => {
    onAtomClick(atomId);
  }, [onAtomClick]);

  // Build position map for connection lines and clusters
  const nodePositions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const node of nodes) {
      map.set(node.id, { x: node.x, y: node.y });
    }
    return map;
  }, [nodes]);

  // Identify hub atoms
  const hubAtomIds = useMemo(() => {
    const hubs = new Set<string>();
    for (const [atomId, count] of Object.entries(connectionCounts)) {
      if (count >= HUB_THRESHOLD) {
        hubs.add(atomId);
      }
    }
    return hubs;
  }, [connectionCounts]);

  return (
    <div
      className="relative bg-[#1e1e1e]"
      style={{
        width: CANVAS_SIZE,
        height: CANVAS_SIZE,
      }}
    >
      {/* Connection lines (behind atoms) */}
      <ConnectionLines
        connections={connections}
        nodePositions={nodePositions}
        fadedAtomIds={fadedAtomIds}
      />

      {/* Atom nodes */}
      {nodes.map((node) => (
        <AtomNode
          key={node.id}
          atom={node.atom}
          x={node.x}
          y={node.y}
          isFaded={fadedAtomIds.has(node.id)}
          isHub={hubAtomIds.has(node.id)}
          isHighlighted={node.id === highlightedAtomId}
          connectionCount={connectionCounts[node.id] || 0}
          onClick={handleAtomClick}
          atomId={node.id}
        />
      ))}
    </div>
  );
}
