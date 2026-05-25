"use client"

import { useEffect, useState } from "react"
import { FolderOpen, AlertTriangle, CheckCircle, Bell } from "lucide-react"
import { invoke } from "@tauri-apps/api/core"
import Analytics from "@/lib/analytics"
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch"
import { useConfig } from "@/contexts/ConfigContext"

export function PreferenceSettings() {
  const { storageLocations, isLoadingPreferences, loadPreferences } = useConfig();
  const [macosPermissionStatus, setMacosPermissionStatus] = useState<string | null>(null);
  const [requestingPermission, setRequestingPermission] = useState(false);

  const refreshStatus = () => {
    invoke<string>('get_macos_notification_status')
      .then(setMacosPermissionStatus)
      .catch(() => setMacosPermissionStatus('authorized'));
  };

  useEffect(() => {
    loadPreferences();
    refreshStatus();
    Analytics.track('preferences_viewed', {}).catch(() => {});
  }, [loadPreferences]);

  const handleRequestPermission = async () => {
    setRequestingPermission(true);
    try {
      console.log('[PreferenceSettings] invoking request_macos_notification_permission');
      const granted = await invoke<boolean>('request_macos_notification_permission');
      console.log('[PreferenceSettings] permission result:', granted);
      refreshStatus();
    } catch (err) {
      console.error('[PreferenceSettings] permission request failed:', err);
      refreshStatus();
    } finally {
      setRequestingPermission(false);
    }
  };

  const handleOpenFolder = async (folderType: 'recordings') => {
    try {
      await invoke('open_recordings_folder');
      Analytics.track('storage_folder_opened', { folder_type: folderType }).catch(() => {});
    } catch (error) {
      console.error('Failed to open recordings folder:', error);
    }
  };

  if (isLoadingPreferences && !storageLocations) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  return (
    <div className="space-y-6">
      {/* Notifications Section */}
      <div className="bg-card rounded-lg border border-border p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-foreground mb-2">Notifications</h3>
        <p className="text-sm text-muted-foreground">
          Meetily notifies you when another app grabs the microphone so you can start recording.
        </p>

        <div className="mt-4 space-y-3">
          <div className="text-xs text-muted-foreground">
            Current status: <span className="font-mono">{macosPermissionStatus ?? 'checking…'}</span>
          </div>

          {macosPermissionStatus === 'not_determined' && (
            <button
              onClick={handleRequestPermission}
              disabled={requestingPermission}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors disabled:opacity-50"
            >
              <Bell className="w-4 h-4" />
              {requestingPermission ? 'Requesting…' : 'Request Notification Permission'}
            </button>
          )}

          {macosPermissionStatus === 'denied' && (
            <div className="flex items-start gap-3 p-3 bg-amber-500/10 border border-amber-500/30 rounded-lg">
              <AlertTriangle className="h-4 w-4 text-amber-400 flex-shrink-0 mt-0.5" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-amber-400">Notification permission denied</p>
                <p className="text-xs text-amber-400/80 mt-0.5">
                  Meetily can&apos;t alert you about meetings until you grant permission in System Settings.
                </p>
                <button
                  onClick={() => invoke('open_notification_system_settings')}
                  className="mt-2 text-xs font-medium text-amber-400 underline hover:text-amber-300"
                >
                  Open System Settings → Notifications
                </button>
              </div>
            </div>
          )}
          {(macosPermissionStatus === 'authorized' || macosPermissionStatus === 'provisional') && (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <CheckCircle className="h-3.5 w-3.5 text-green-500" />
              <span>System notification permission granted</span>
            </div>
          )}
        </div>
      </div>

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
            <button
              onClick={() => handleOpenFolder('recordings')}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-border rounded-md hover:bg-muted transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
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
