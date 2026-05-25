import React from "react";

interface LogoProps {
    isCollapsed: boolean;
    toggleCollapse: () => void;
}

const WaveformMark = () => (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" aria-hidden="true" className="text-blue-500">
    <rect x="0"  y="9"  width="3" height="6"  rx="1.5" fill="currentColor"/>
    <rect x="5"  y="5"  width="3" height="14" rx="1.5" fill="currentColor"/>
    <rect x="10" y="1"  width="3" height="22" rx="1.5" fill="currentColor"/>
    <rect x="15" y="5"  width="3" height="14" rx="1.5" fill="currentColor"/>
    <rect x="20" y="9"  width="3" height="6"  rx="1.5" fill="currentColor"/>
  </svg>
);

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(({ isCollapsed, toggleCollapse }, ref) => {
  return isCollapsed ? (
    <button ref={ref} onClick={toggleCollapse} className="flex items-center justify-center mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-75 transition-opacity">
      <WaveformMark />
    </button>
  ) : (
    <button ref={ref} onClick={toggleCollapse} className="flex items-center gap-2 mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-75 transition-opacity w-full">
      <WaveformMark />
      <span className="text-[15px] font-semibold tracking-[-0.2px] text-foreground">meetily</span>
    </button>
  );
});

Logo.displayName = "Logo";

export default Logo;
