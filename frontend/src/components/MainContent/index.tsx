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
      className="flex-1"
      style={{ marginLeft: isCollapsed ? 64 : sidebarWidth }}
    >
      <div className="pl-8 bg-background">
        {children}
      </div>
    </main>
  );
};

export default MainContent;
