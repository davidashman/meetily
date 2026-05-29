'use client';

import { Suspense, useEffect } from 'react';
import { useSearchParams } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { RecordingControls } from '@/components/RecordingControls';
import { RecordingStateProvider, useRecordingState } from '@/contexts/RecordingStateContext';

function OverlayContent() {
  const params = useSearchParams();
  const meetingName = params.get('meeting') ?? '';
  const { isRecording } = useRecordingState();

  // Close when recording stops from any source (tray, main window, or this overlay)
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen('recording-stop-complete', () => {
      getCurrentWindow().close();
    }).then(fn => { unlisten = fn; });
    return () => unlisten?.();
  }, []);

  const handleRecordingStart = async () => {
    await invoke('overlay_record', { meetingName }).catch(console.error);
  };

  const handleDismiss = async () => {
    await invoke('overlay_decline').catch(console.error);
  };

  const handleRecordingStop = async () => {
    // Notify the main window so it can run post-processing (summary, etc.)
    const { emit } = await import('@tauri-apps/api/event');
    await emit('recording-stop-complete', true);
  };

  return (
    <RecordingControls
      isRecording={isRecording}
      onRecordingStart={handleRecordingStart}
      onRecordingStop={handleRecordingStop}
      onTranscriptReceived={() => {}}
      isRecordingDisabled={false}
      isParentProcessing={false}
      onDismiss={handleDismiss}
      draggable
    />
  );
}

export default function OverlayPage() {
  return (
    <Suspense>
      <RecordingStateProvider>
        <OverlayContent />
      </RecordingStateProvider>
    </Suspense>
  );
}
