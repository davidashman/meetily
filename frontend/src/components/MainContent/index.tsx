'use client';

import React from 'react';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

interface MainContentProps {
  children: React.ReactNode;
}

const MainContent: React.FC<MainContentProps> = ({ children }) => {
  const { isCollapsed, sidebarWidth } = useSidebar();

  return (
    <main
      className="flex-1 flex flex-col"
      style={{ marginLeft: isCollapsed ? 64 : sidebarWidth }}
    >
      <div className="flex-shrink-0 h-7" data-tauri-drag-region />
      <div className="pl-8 bg-background flex-1">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
