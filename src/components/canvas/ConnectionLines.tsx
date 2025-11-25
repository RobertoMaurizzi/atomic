import { memo } from 'react';

export interface Connection {
  sourceId: string;
  targetId: string;
  sharedTagCount: number;
}

interface ConnectionLinesProps {
  connections: Connection[];
  nodePositions: Map<string, { x: number; y: number }>;
  fadedAtomIds: Set<string>;
}

export const ConnectionLines = memo(function ConnectionLines({
  connections,
  nodePositions,
  fadedAtomIds,
}: ConnectionLinesProps) {
  return (
    <svg
      className="absolute top-0 left-0 w-full h-full pointer-events-none"
      style={{ zIndex: 0 }}
    >
      {connections.map((conn) => {
        const source = nodePositions.get(conn.sourceId);
        const target = nodePositions.get(conn.targetId);

        if (!source || !target) return null;

        // Check if either endpoint is faded
        const isFaded =
          fadedAtomIds.has(conn.sourceId) || fadedAtomIds.has(conn.targetId);

        return (
          <line
            key={`${conn.sourceId}-${conn.targetId}`}
            x1={source.x}
            y1={source.y}
            x2={target.x}
            y2={target.y}
            stroke="#666"
            strokeWidth={1}
            strokeOpacity={isFaded ? 0.05 : 0.15}
          />
        );
      })}
    </svg>
  );
});

