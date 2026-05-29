"use client"

import { useEffect } from "react"
import Analytics from "@/lib/analytics"
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch"
import { useConfig } from "@/contexts/ConfigContext"

export function PreferenceSettings() {
  const { storageLocations, isLoadingPreferences, loadPreferences } = useConfig();

  useEffect(() => {
    loadPreferences();
    Analytics.track('preferences_viewed', {}).catch(() => {});
  }, [loadPreferences]);

  if (isLoadingPreferences && !storageLocations) {
    return <div className="max-w-2xl mx-auto p-6">Loading Preferences...</div>
  }

  return (
    <div className="space-y-6">
      {/* Data Storage Locations Section */}
      <div className="bg-card rounded-lg border border-border p-6 shadow-sm">
        <h3 className="text-lg font-semibold text-foreground mb-4">Data Storage Locations</h3>
        <p className="text-sm text-muted-foreground mb-6">
          View where Meetily stores your data
        </p>

        <div className="space-y-4">
          <div className="p-4 border border-border rounded-lg bg-muted/50">
            <div className="font-medium mb-2">Database</div>
            <div className="text-sm text-muted-foreground mb-3 break-all font-mono text-xs">
              {storageLocations?.database || 'Loading...'}
            </div>
          </div>

          <div className="p-4 border border-border rounded-lg bg-muted/50">
            <div className="font-medium mb-2">Models</div>
            <div className="text-sm text-muted-foreground mb-3 break-all font-mono text-xs">
              {storageLocations?.models || 'Loading...'}
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
