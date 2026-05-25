import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Mic, Volume2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { OnboardingContainer } from '../OnboardingContainer';
import { PermissionRow } from '../shared';
import { useOnboarding } from '@/contexts/OnboardingContext';

export function PermissionsStep() {
  const { setPermissionStatus, setPermissionsSkipped, permissions, completeOnboarding } = useOnboarding();
  const [isPending, setIsPending] = useState(false);

  const handleMicrophoneAction = async () => {
    if (permissions.microphone === 'denied') {
      try { await invoke('open_system_settings'); } catch {
        alert('Please enable microphone access in System Preferences > Security & Privacy > Microphone');
      }
      return;
    }
    setIsPending(true);
    try {
      const granted = await invoke<boolean>('trigger_microphone_permission');
      setPermissionStatus('microphone', granted ? 'authorized' : 'denied');
    } catch {
      setPermissionStatus('microphone', 'denied');
    } finally {
      setIsPending(false);
    }
  };

  const handleSystemAudioAction = async () => {
    if (permissions.systemAudio === 'denied') {
      try { await invoke('open_system_settings'); } catch {
        alert('Please enable Audio Capture in System Settings → Privacy & Security → Audio Capture');
      }
      return;
    }
    setIsPending(true);
    try {
      const granted = await invoke<boolean>('trigger_system_audio_permission_command');
      setPermissionStatus('systemAudio', granted ? 'authorized' : 'denied');
    } catch {
      setPermissionStatus('systemAudio', 'denied');
    } finally {
      setIsPending(false);
    }
  };

  const handleFinish = async () => {
    try {
      await completeOnboarding();
      window.location.reload();
    } catch (error) {
      console.error('Failed to complete onboarding:', error);
    }
  };

  const handleSkip = async () => {
    setPermissionsSkipped(true);
    await handleFinish();
  };

  const allPermissionsGranted =
    permissions.microphone === 'authorized' &&
    permissions.systemAudio === 'authorized';


  return (
    <OnboardingContainer
      title="Grant Permissions"
      description="Meetily needs access to your microphone and system audio to record meetings"
      step={4}
      hideProgress={true}
      showNavigation={allPermissionsGranted}
      canGoNext={allPermissionsGranted}
    >
      <div className="max-w-lg mx-auto space-y-6">
        <div className="space-y-4">
          <PermissionRow
            icon={<Mic className="w-5 h-5" />}
            title="Microphone"
            description="Required to capture your voice during meetings"
            status={permissions.microphone}
            isPending={isPending}
            onAction={handleMicrophoneAction}
          />

          <PermissionRow
            icon={<Volume2 className="w-5 h-5" />}
            title="System Audio"
            description="Click Enable to grant Audio Capture permission"
            status={permissions.systemAudio}
            isPending={isPending}
            onAction={handleSystemAudioAction}
          />
        </div>

        <div className="flex flex-col gap-3 pt-4">
          <Button onClick={handleFinish} disabled={!allPermissionsGranted} className="w-full h-11">
            Finish Setup
          </Button>

          <button
            onClick={handleSkip}
            className="text-sm text-neutral-500 hover:text-neutral-700 transition-colors"
          >
            I'll do this later
          </button>

          {!allPermissionsGranted && (
            <p className="text-xs text-center text-muted-foreground">
              Recording won't work without permissions. You can grant them later in settings.
            </p>
          )}
        </div>
      </div>
    </OnboardingContainer>
  );
}
