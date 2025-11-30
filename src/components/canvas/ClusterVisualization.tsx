import { memo, useMemo } from 'react';
import { AtomCluster } from '../../lib/tauri';

interface ClusterVisualizationProps {
  clusters: AtomCluster[];
  nodePositions: Map<string, { x: number; y: number }>;
  fadedAtomIds: Set<string>;
  onClusterClick?: (clusterId: number) => void;
}

// Generate a consistent color from cluster ID
function clusterToColor(clusterId: number): { h: number; s: number; l: number; hsl: string } {
  const hues = [210, 280, 160, 30, 340, 120, 60, 190, 250, 90];
  const h = hues[clusterId % hues.length];
  const s = 40;
  const l = 45;
  return { h, s, l, hsl: `hsl(${h}, ${s}%, ${l}%)` };
}

function colorWithAlpha(color: { h: number; s: number; l: number }, alpha: number): string {
  return `hsla(${color.h}, ${color.s}%, ${color.l}%, ${alpha})`;
}

// Calculate the centroid of a set of points
function calculateCentroid(points: { x: number; y: number }[]): { x: number; y: number } {
  if (points.length === 0) return { x: 0, y: 0 };
  const sum = points.reduce(
    (acc, p) => ({ x: acc.x + p.x, y: acc.y + p.y }),
    { x: 0, y: 0 }
  );
  return { x: sum.x / points.length, y: sum.y / points.length };
}

// Calculate bounding circle radius
function calculateRadius(points: { x: number; y: number }[], centroid: { x: number; y: number }): number {
  if (points.length === 0) return 0;
  const maxDist = Math.max(
    ...points.map((p) =>
      Math.sqrt(Math.pow(p.x - centroid.x, 2) + Math.pow(p.y - centroid.y, 2))
    )
  );
  return maxDist + 100; // Add padding
}

export const ClusterVisualization = memo(function ClusterVisualization({
  clusters,
  nodePositions,
  fadedAtomIds,
  onClusterClick,
}: ClusterVisualizationProps) {
  // Calculate cluster positions and shapes
  const clusterData = useMemo(() => {
    return clusters
      .map((cluster) => {
        // Get positions of atoms in this cluster
        const points: { x: number; y: number }[] = [];
        let allFaded = true;

        for (const atomId of cluster.atom_ids) {
          const pos = nodePositions.get(atomId);
          if (pos) {
            points.push(pos);
            if (!fadedAtomIds.has(atomId)) {
              allFaded = false;
            }
          }
        }

        if (points.length < 2) return null;

        const centroid = calculateCentroid(points);
        const radius = calculateRadius(points, centroid);
        const color = clusterToColor(cluster.cluster_id);

        return {
          ...cluster,
          centroid,
          radius,
          color,
          allFaded,
          pointCount: points.length,
        };
      })
      .filter((c): c is NonNullable<typeof c> => c !== null);
  }, [clusters, nodePositions, fadedAtomIds]);

  return (
    <>
      {/* Cluster backgrounds */}
      <svg
        className="absolute top-0 left-0 w-full h-full pointer-events-none"
        style={{ zIndex: -1 }}
      >
        {clusterData.map((cluster) => (
          <circle
            key={`bg-${cluster.cluster_id}`}
            cx={cluster.centroid.x}
            cy={cluster.centroid.y}
            r={cluster.radius}
            fill={colorWithAlpha(cluster.color, cluster.allFaded ? 0.02 : 0.06)}
            stroke={colorWithAlpha(cluster.color, cluster.allFaded ? 0.05 : 0.15)}
            strokeWidth={1}
            strokeDasharray="8,4"
          />
        ))}
      </svg>

      {/* Cluster labels */}
      {clusterData.map((cluster) => {
        // Filter out empty/whitespace tags
        const visibleTags = cluster.dominant_tags.filter(t => t && t.trim());
        if (visibleTags.length === 0) return null;

        return (
          <div
            key={`label-${cluster.cluster_id}`}
            className={`absolute pointer-events-auto cursor-pointer transition-opacity ${
              cluster.allFaded ? 'opacity-20' : 'opacity-70 hover:opacity-100'
            }`}
            style={{
              left: cluster.centroid.x,
              top: cluster.centroid.y - cluster.radius + 20,
              transform: 'translate(-50%, 0)',
            }}
            onClick={() => onClusterClick?.(cluster.cluster_id)}
            title={`Cluster: ${cluster.dominant_tags.join(', ')} (${cluster.pointCount} atoms)`}
          >
            <div
              className="px-2 py-1 rounded-md text-xs font-medium whitespace-nowrap"
              style={{
                backgroundColor: colorWithAlpha(cluster.color, 0.2),
                color: cluster.color.hsl,
                border: `1px solid ${colorWithAlpha(cluster.color, 0.3)}`,
              }}
            >
              {visibleTags.slice(0, 2).join(' · ')}
              {cluster.pointCount > 3 && (
                <span className="ml-1 opacity-60">({cluster.pointCount})</span>
              )}
            </div>
          </div>
        );
      })}
    </>
  );
});
