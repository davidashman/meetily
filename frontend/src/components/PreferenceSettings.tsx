"use client"

import { useEffect, useState } from "react"
import { FolderOpen, FolderEdit } from "lucide-react"
import { invoke } from "@tauri-apps/api/core"
import Analytics from "@/lib/analytics"
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch"
import { useConfig } from "@/contexts/ConfigContext"

export function PreferenceSettings() {
  const { storageLocations, isLoadingPreferences, loadPreferences, updateRecordingsPath } = useConfig();
  const [isSelectingFolder, setIsSelectingFolder] = useState(false);

  useEffect(() => {
    loadPreferences();
    Analytics.track('preferences_viewed', {}).catch(() => {});
  }, [loadPreferences]);

  const handleOpenFolder = async () => {
    try {
      await invoke('open_recordings_folder');
      Analytics.track('storage_folder_opened', { folder_type: 'recordings' }).catch(() => {});
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
    }
  };

  const handleChangeFolder = async () => {
    setIsSelectingFolder(true);
    try {
      const newPath = await invoke<string | null>('select_recording_folder');
      if (newPath) {
        updateRecordingsPath(newPath);
        Analytics.track('storage_folder_changed', { folder_type: 'recordings' }).catch(() => {});
      }
    } catch (error) {
      console.error('Failed to select recordings folder:', error);
    } finally {
      setIsSelectingFolder(false);
    }
  };

  if (isLoadingPreferences && !storageLocations) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  return (
    <div className="space-y-6">
      {/* Data Storage Locations Section */}
      <div className="bg-card rounded-lg border border-border p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-foreground mb-4">Data Storage Locations</h3>
        <p className="text-sm text-muted-foreground mb-6">
          View and access where Meetily stores your data
        </p>

        <div className="space-y-4">
          <div className="p-4 border border-border rounded-lg bg-muted/50">
            <div className="font-medium mb-2">Meeting Recordings</div>
            <div className="text-sm text-muted-foreground mb-3 break-all font-mono text-xs">
              {storageLocations?.recordings || 'Loading...'}
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={handleOpenFolder}
                className="flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors"
              >
                <FolderOpen className="w-4 h-4" />
                Open Folder
              </button>
              <button
                onClick={handleChangeFolder}
                disabled={isSelectingFolder}
                className="flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <FolderEdit className="w-4 h-4" />
                {isSelectingFolder ? 'Selecting...' : 'Change Folder'}
              </button>
            </div>
          </div>
        </div>

        <div className="mt-4 p-3 bg-blue-500/10 border border-blue-500/30 rounded-md">
          <p className="text-xs text-blue-400">
            <strong>Note:</strong> Database and models are stored together in your application data directory for unified management.
          </p>
        </div>
      </div>

      {/* Analytics Section */}
      <div className="bg-card rounded-lg border border-border p-6 shadow-sm">
        <AnalyticsConsentSwitch />
      </div>
    </div>
  )
}
