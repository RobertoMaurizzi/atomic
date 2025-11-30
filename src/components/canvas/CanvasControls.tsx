import { useState } from 'react';
import { useControls } from 'react-zoom-pan-pinch';
import { ConnectionOptions } from './CanvasView';

interface CanvasControlsProps {
  connectionOptions: ConnectionOptions;
  onConnectionOptionsChange: (options: ConnectionOptions) => void;
}

export function CanvasControls({ connectionOptions, onConnectionOptionsChange }: CanvasControlsProps) {
  const { zoomIn, zoomOut, resetTransform } = useControls();
  const [showPanel, setShowPanel] = useState(false);

  const handleToggleTag = () => {
    onConnectionOptionsChange({
      ...connectionOptions,
      showTagConnections: !connectionOptions.showTagConnections,
    });
  };

  const handleToggleSemantic = () => {
    onConnectionOptionsChange({
      ...connectionOptions,
      showSemanticConnections: !connectionOptions.showSemanticConnections,
    });
  };

  const handleSimilarityChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    onConnectionOptionsChange({
      ...connectionOptions,
      minSimilarity: parseFloat(e.target.value),
    });
  };

  return (
    <>
      {/* Connection options panel */}
      {showPanel && (
        <div className="absolute bottom-16 right-4 z-10 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md p-3 w-56 shadow-lg">
          <div className="text-xs text-[#888888] mb-3 font-medium">Connections</div>

          {/* Tag connections toggle */}
          <label className="flex items-center gap-2 mb-2 cursor-pointer">
            <input
              type="checkbox"
              checked={connectionOptions.showTagConnections}
              onChange={handleToggleTag}
              className="w-4 h-4 rounded bg-[#1e1e1e] border-[#3d3d3d] text-[#7c3aed] focus:ring-[#7c3aed] focus:ring-offset-0"
            />
            <span className="text-sm text-[#dcddde] flex items-center gap-2">
              <span className="w-4 h-0.5 bg-[#666666] inline-block" />
              Tag connections
            </span>
          </label>

          {/* Semantic connections toggle */}
          <label className="flex items-center gap-2 mb-3 cursor-pointer">
            <input
              type="checkbox"
              checked={connectionOptions.showSemanticConnections}
              onChange={handleToggleSemantic}
              className="w-4 h-4 rounded bg-[#1e1e1e] border-[#3d3d3d] text-[#7c3aed] focus:ring-[#7c3aed] focus:ring-offset-0"
            />
            <span className="text-sm text-[#dcddde] flex items-center gap-2">
              <span className="w-4 h-0.5 bg-[#7c3aed] inline-block" style={{ backgroundImage: 'repeating-linear-gradient(90deg, #7c3aed 0, #7c3aed 4px, transparent 4px, transparent 6px)' }} />
              Semantic connections
            </span>
          </label>

          {/* Similarity threshold slider */}
          {connectionOptions.showSemanticConnections && (
            <div className="pt-2 border-t border-[#3d3d3d]">
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs text-[#888888]">Min similarity</span>
                <span className="text-xs text-[#dcddde] font-mono">{connectionOptions.minSimilarity.toFixed(2)}</span>
              </div>
              <input
                type="range"
                min="0.3"
                max="0.9"
                step="0.05"
                value={connectionOptions.minSimilarity}
                onChange={handleSimilarityChange}
                className="w-full h-1.5 bg-[#3d3d3d] rounded-lg appearance-none cursor-pointer accent-[#7c3aed]"
              />
              <div className="flex justify-between text-xs text-[#666666] mt-1">
                <span>More</span>
                <span>Fewer</span>
              </div>
            </div>
          )}

          {/* Legend */}
          <div className="mt-3 pt-2 border-t border-[#3d3d3d]">
            <div className="text-xs text-[#888888] mb-2">Legend</div>
            <div className="space-y-1.5 text-xs">
              <div className="flex items-center gap-2">
                <span className="w-5 h-0.5 bg-[#666666] inline-block" />
                <span className="text-[#888888]">Shared tags</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="w-5 h-0.5 inline-block" style={{ backgroundImage: 'repeating-linear-gradient(90deg, #7c3aed 0, #7c3aed 4px, transparent 4px, transparent 6px)' }} />
                <span className="text-[#888888]">Semantic similarity</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="w-5 h-0.5 bg-[#a78bfa] inline-block" />
                <span className="text-[#888888]">Both</span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Control buttons */}
      <div className="absolute bottom-4 right-4 flex flex-col gap-1 z-10">
        {/* Connection options toggle */}
        <button
          onClick={() => setShowPanel(!showPanel)}
          className={`w-8 h-8 border rounded transition-colors flex items-center justify-center ${
            showPanel
              ? 'bg-[#7c3aed] border-[#7c3aed] text-white'
              : 'bg-[#2d2d2d] border-[#3d3d3d] text-[#dcddde] hover:bg-[#3d3d3d]'
          }`}
          title="Connection options"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <circle cx="6" cy="12" r="2" strokeWidth={2} />
            <circle cx="18" cy="12" r="2" strokeWidth={2} />
            <path strokeLinecap="round" strokeWidth={2} d="M8 12h8" />
          </svg>
        </button>

        <div className="h-px bg-[#3d3d3d] my-1" />

        <button
          onClick={() => zoomIn()}
          className="w-8 h-8 bg-[#2d2d2d] border border-[#3d3d3d] rounded text-[#dcddde] hover:bg-[#3d3d3d] transition-colors flex items-center justify-center"
          title="Zoom in"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
        </button>
        <button
          onClick={() => zoomOut()}
          className="w-8 h-8 bg-[#2d2d2d] border border-[#3d3d3d] rounded text-[#dcddde] hover:bg-[#3d3d3d] transition-colors flex items-center justify-center"
          title="Zoom out"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" />
          </svg>
        </button>
        <button
          onClick={() => resetTransform()}
          className="w-8 h-8 bg-[#2d2d2d] border border-[#3d3d3d] rounded text-[#dcddde] hover:bg-[#3d3d3d] transition-colors flex items-center justify-center"
          title="Reset view"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5l-5-5m5 5v-4m0 4h-4" />
          </svg>
        </button>
      </div>
    </>
  );
}
