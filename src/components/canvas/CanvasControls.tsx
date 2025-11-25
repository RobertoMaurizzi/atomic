import { useControls } from 'react-zoom-pan-pinch';

export function CanvasControls() {
  const { zoomIn, zoomOut, resetTransform } = useControls();

  return (
    <div className="absolute bottom-4 right-4 flex flex-col gap-1 z-10">
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
  );
}

