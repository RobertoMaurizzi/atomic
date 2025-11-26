import { useMemo, useCallback } from 'react';
import { AtomNode } from './AtomNode';
import { ConnectionLines, Connection } from './ConnectionLines';
import { SimulationNode } from './useForceSimulation';

const CANVAS_SIZE = 5000;

interface CanvasContentProps {
  nodes: SimulationNode[];
  connections: Connection[];
  fadedAtomIds: Set<string>;
  onAtomClick: (atomId: string) => void;
}

export function CanvasContent({
  nodes,
  connections,
  fadedAtomIds,
  onAtomClick,
}: CanvasContentProps) {
  // Stable onClick handler to prevent AtomNode re-renders
  const handleAtomClick = useCallback((atomId: string) => {
    onAtomClick(atomId);
  }, [onAtomClick]);

  // Build position map for connection lines
  const nodePositions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const node of nodes) {
      map.set(node.id, { x: node.x, y: node.y });
    }
    return map;
  }, [nodes]);

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
          onClick={handleAtomClick}
          atomId={node.id}
        />
      ))}
    </div>
  );
}

