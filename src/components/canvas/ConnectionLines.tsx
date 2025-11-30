import { memo } from 'react';

export interface Connection {
  sourceId: string;
  targetId: string;
  sharedTagCount: number;
  type?: 'tag' | 'semantic' | 'both';
  strength?: number;
  similarityScore?: number | null;
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

        // Determine stroke style based on connection type
        const connectionType = conn.type || 'tag';
        const strength = conn.strength || 0.3;

        let strokeColor: string;
        let strokeDasharray: string | undefined;
        let strokeWidth: number;
        let baseOpacity: number;

        switch (connectionType) {
          case 'semantic':
            // Purple dashed line for semantic connections
            strokeColor = '#7c3aed';
            strokeDasharray = '6,3';
            strokeWidth = 1 + strength;
            baseOpacity = 0.2 + strength * 0.3;
            break;
          case 'both':
            // Thicker solid purple for combined connections
            strokeColor = '#a78bfa';
            strokeDasharray = undefined;
            strokeWidth = 1.5 + strength;
            baseOpacity = 0.3 + strength * 0.3;
            break;
          case 'tag':
          default:
            // Gray solid line for tag connections
            strokeColor = '#666666';
            strokeDasharray = undefined;
            strokeWidth = 1;
            baseOpacity = 0.15;
            break;
        }

        return (
          <line
            key={`${conn.sourceId}-${conn.targetId}`}
            x1={source.x}
            y1={source.y}
            x2={target.x}
            y2={target.y}
            stroke={strokeColor}
            strokeWidth={strokeWidth}
            strokeOpacity={isFaded ? baseOpacity * 0.2 : baseOpacity}
            strokeDasharray={strokeDasharray}
          />
        );
      })}
    </svg>
  );
});
