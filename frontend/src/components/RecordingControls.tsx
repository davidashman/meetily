'use client';

import { invoke } from '@tauri-apps/api/core';
import { appDataDir } from '@tauri-apps/api/path';
import { useCallback, useEffect, useState } from 'react';
import { Play, Pause, Square, Mic, AlertCircle, X } from 'lucide-react';
import { SummaryResponse } from '@/types/summary';
import { listen } from '@tauri-apps/api/event';
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import Analytics from '@/lib/analytics';
import { useRecordingState } from '@/contexts/RecordingStateContext';

interface RecordingControlsProps {
  isRecording: boolean;
  onRecordingStop: (callApi?: boolean) => void;
  onRecordingStart: () => void;
  onTranscriptReceived: (summary: SummaryResponse) => void;
  onTranscriptionError?: (message: string) => void;
  onStopInitiated?: () => void;
  isRecordingDisabled: boolean;
  isParentProcessing: boolean;
  // When provided, shows X button (dismiss) alongside the mic in idle state.
  // Also signals overlay context — enables draggable + glassmorphism styling.
  onDismiss?: () => void;
  draggable?: boolean;
  selectedDevices?: {
    micDevice: string | null;
    systemDevice: string | null;
  };
  meetingName?: string;
}

const glassStyle: React.CSSProperties = {
  background: 'rgba(18, 18, 28, 0.65)',
  backdropFilter: 'blur(20px)',
  WebkitBackdropFilter: 'blur(20px)',
  border: '1px solid rgba(255,255,255,0.07)',
};

const COUNTDOWN_SECS = 30;
const R = 17; // SVG arc radius (fits in 40×40 container with strokeWidth 2.5)
const CIRC = 2 * Math.PI * R;

export const RecordingControls: React.FC<RecordingControlsProps> = ({
  isRecording,
  onRecordingStop,
  onRecordingStart,
  onTranscriptReceived,
  onTranscriptionError,
  onStopInitiated,
  isRecordingDisabled,
  isParentProcessing,
  onDismiss,
  draggable = false,
  selectedDevices,
  meetingName,
}) => {
  const recordingState = useRecordingState();
  const isPaused = recordingState.isPaused;

  const [isProcessing, setIsProcessing] = useState(false);
  const [isStarting, setIsStarting] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [isPausing, setIsPausing] = useState(false);
  const [isResuming, setIsResuming] = useState(false);
  const [transcriptionErrors, setTranscriptionErrors] = useState(0);
  const [isValidatingModel, setIsValidatingModel] = useState(false);
  const [speechDetected, setSpeechDetected] = useState(false);
  const [deviceError, setDeviceError] = useState<{ title: string; message: string } | null>(null);
  const [countdown, setCountdown] = useState(COUNTDOWN_SECS);

  const formatTime = (time: number) => {
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  };

  // Auto-dismiss countdown: only runs in overlay prompt mode (onDismiss present, not yet recording)
  useEffect(() => {
    if (!onDismiss || isRecording) {
      setCountdown(COUNTDOWN_SECS);
      return;
    }
    const t = setInterval(() => {
      setCountdown(n => {
        if (n <= 1) {
          clearInterval(t);
          onDismiss();
          return 0;
        }
        return n - 1;
      });
    }, 1000);
    return () => clearInterval(t);
  }, [onDismiss, isRecording]);

  useEffect(() => {
    const checkTauri = async () => {
      try {
        await invoke('is_recording');
      } catch (error) {
        console.error('Tauri initialization error:', error);
      }
    };
    checkTauri();
  }, []);

  const handleStartRecording = useCallback(async () => {
    if (isStarting || isValidatingModel) return;
    setSpeechDetected(false);

    try {
      await onRecordingStart();
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      if (errorMsg.includes('microphone') || errorMsg.includes('mic') || errorMsg.includes('input')) {
        setDeviceError({
          title: 'Microphone Not Available',
          message: 'Unable to access your microphone. Please check that:\n• Your microphone is connected\n• The app has microphone permissions\n• No other app is using the microphone'
        });
      } else if (errorMsg.includes('system audio') || errorMsg.includes('speaker') || errorMsg.includes('output')) {
        setDeviceError({
          title: 'System Audio Not Available',
          message: 'Unable to capture system audio. Please check that:\n• A virtual audio device (like BlackHole) is installed\n• The app has screen recording permissions (macOS)\n• System audio is properly configured'
        });
      } else if (errorMsg.includes('permission')) {
        setDeviceError({
          title: 'Permission Required',
          message: 'Recording permissions are required. Please:\n• Grant microphone access in System Settings\n• Grant screen recording access for system audio (macOS)\n• Restart the app after granting permissions'
        });
      } else {
        setDeviceError({
          title: 'Recording Failed',
          message: 'Unable to start recording. Please check your audio device settings and try again.'
        });
      }
    }
  }, [onRecordingStart, isStarting, isValidatingModel]);

  const stopRecordingAction = useCallback(async () => {
    try {
      setIsProcessing(true);
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const savePath = `${dataDir}/recording-${timestamp}.wav`;
      await invoke('stop_recording', { args: { save_path: savePath } });
      setIsProcessing(false);
      Analytics.trackTranscriptionSuccess();
      onRecordingStop(true);
    } catch (error) {
      console.error('Failed to stop recording:', error);
      const isNoRecording = (e: unknown) => {
        const msg = e instanceof Error ? e.message : String(e);
        return msg.includes('No recording in progress');
      };
      if (!isNoRecording(error)) {
        setIsProcessing(false);
        onRecordingStop(false);
      }
    } finally {
      setIsStopping(false);
    }
  }, [onRecordingStop]);

  const handleStopRecording = useCallback(async () => {
    if (!isRecording || isStarting || isStopping) return;
    onStopInitiated?.();
    setIsStopping(true);
    await stopRecordingAction();
  }, [isRecording, isStarting, isStopping, stopRecordingAction, onStopInitiated]);

  const handlePauseRecording = useCallback(async () => {
    if (!isRecording || isPaused || isPausing) return;
    setIsPausing(true);
    try {
      await invoke('pause_recording');
    } catch (error) {
      console.error('Failed to pause recording:', error);
    } finally {
      setIsPausing(false);
    }
  }, [isRecording, isPaused, isPausing]);

  const handleResumeRecording = useCallback(async () => {
    if (!isRecording || !isPaused || isResuming) return;
    setIsResuming(true);
    try {
      await invoke('resume_recording');
    } catch (error) {
      console.error('Failed to resume recording:', error);
    } finally {
      setIsResuming(false);
    }
  }, [isRecording, isPaused, isResuming]);

  useEffect(() => {
    let unsubscribes: (() => void)[] = [];

    const setupListeners = async () => {
      try {
        const transcriptErrorUnsubscribe = await listen('transcript-error', (event) => {
          const errorMessage = event.payload as string;
          Analytics.trackTranscriptionError(errorMessage);
          setTranscriptionErrors(prev => prev + 1);
          setIsProcessing(false);
          onRecordingStop(false);
          if (onTranscriptionError) onTranscriptionError(errorMessage);
        });

        const transcriptionErrorUnsubscribe = await listen('transcription-error', (event) => {
          let errorMessage: string;
          if (typeof event.payload === 'object' && event.payload !== null) {
            const payload = event.payload as { error: string; userMessage: string; actionable: boolean };
            errorMessage = payload.userMessage || payload.error;
          } else {
            errorMessage = String(event.payload);
          }
          Analytics.trackTranscriptionError(errorMessage);
          setTranscriptionErrors(prev => prev + 1);
          setIsProcessing(false);
          onRecordingStop(false);
        });

        const speechDetectedUnsubscribe = await listen('speech-detected', () => {
          setSpeechDetected(true);
        });

        unsubscribes = [transcriptErrorUnsubscribe, transcriptionErrorUnsubscribe, speechDetectedUnsubscribe];
      } catch (error) {
        console.error('Failed to set up recording event listeners:', error);
      }
    };

    setupListeners();
    return () => unsubscribes.forEach(u => u?.());
  }, [onRecordingStop, onTranscriptionError]);

  // SVG arc: full circle = COUNTDOWN_SECS, shrinks to 0
  const arcProgress = countdown / COUNTDOWN_SECS; // 1 → 0
  const dashOffset = CIRC * (1 - arcProgress);    // 0 → CIRC

  return (
    <TooltipProvider>
      <div
        className={`flex items-center rounded-full shadow-lg gap-2 bg-background px-2 py-2 `}
        style={draggable ? glassStyle : {}}
        {...(draggable ? { 'data-tauri-drag-region': '' } : {})}
      >
        <div 
          className="w-14 h-10 flex items-center justify-center rounded-full relative"
          style={(onDismiss || isRecording) ? {} : { 'display': 'none' }}
        >
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="relative w-10 h-10">
                <button
                  onClick={() => {
                    if (isRecording) {
                      if (isPaused) {
                        Analytics.trackButtonClick('resume_recording', 'recording_controls');
                        handleResumeRecording();
                      } else {
                        Analytics.trackButtonClick('pause_recording', 'recording_controls');
                        handlePauseRecording();
                      }
                    } else if (onDismiss) {
                      onDismiss();
                    }
                  }}
                  disabled={isPausing || isResuming || isStopping}
                  className={`absolute top-0 left-0 w-10 h-10 flex items-center justify-center rounded-full transition-colors bg-gray-700 border border-gray-600`}
                >
                  {isRecording ? isPaused ? <Play size={16} /> : <Pause size={16} /> : onDismiss ? <X size={16} /> : <Pause size={16} />}
                  {!draggable && (isPausing || isResuming) && (
                    <div className="absolute -top-8 text-gray-600 font-medium text-xs">
                      {isPausing ? 'Pausing...' : 'Resuming...'}
                    </div>
                  )}
                </button>
                {onDismiss && !isRecording && (
                  <svg
                    width={40} height={40}
                    className="absolute top-0 left-0 -rotate-90"
                    style={{ pointerEvents: 'none' }}
                  >
                    <circle
                      cx={20} cy={20} r={R}
                      fill="none"
                      stroke="#3b82f6"
                      strokeWidth={2.5}
                      strokeDasharray={CIRC}
                      strokeDashoffset={dashOffset}
                      strokeLinecap="round"
                      style={{ transition: 'stroke-dashoffset 0.9s linear' }}
                    />
                  </svg>
                )}
              </div>
            </TooltipTrigger>
            <TooltipContent><p>{isPaused ? 'Resume recording' : 'Pause recording'}</p></TooltipContent>
          </Tooltip>
        </div>

        <Tooltip>
          <TooltipTrigger asChild>
            <button
              onClick={() => {
                if (isRecording) {
                  Analytics.trackButtonClick('stop_recording', 'recording_controls');
                  handleStopRecording();
                } else {
                  Analytics.trackButtonClick('start_recording', 'recording_controls');
                  handleStartRecording();
                }
              }}
              disabled={isStopping || isPausing || isResuming}
              className={`w-12 h-12 flex items-center justify-center rounded-full text-white transition-colors relative flex-shrink-0 ${
                isStopping || isPausing || isResuming ? 'bg-gray-700' : 'bg-red-500 hover:bg-red-600'
              }`}
          >
               {isValidatingModel ? (
                 <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-white" />
               ) : isRecording ? (
                <Square size={16} fill="currentColor" />
               ) : (
                 <Mic size={20} />
               )}
              {!draggable && isStopping && (
                <div className="absolute -top-8 text-gray-600 font-medium text-xs">
                  Stopping...
                </div>
              )}
            </button>
          </TooltipTrigger>
          <TooltipContent><p>Stop recording</p></TooltipContent>
        </Tooltip>

        <div 
          className="flex items-center justify-center flex-shrink-0 w-14"
          style={(onDismiss || isRecording) ? {} : { 'display': 'none' }}
        >
          <span className={`text-xs font-mono font-semibold tabular-nums ${isPaused || !isRecording ? 'text-gray-400' : 'text-white'}`}>
            {formatTime(recordingState.elapsedSeconds)}
          </span>
        </div>
      </div>

      {!draggable && isValidatingModel && (
        <div className="text-xs text-gray-600 text-center mt-2">
          Validating speech recognition...
        </div>
      )}
    </TooltipProvider>
  );
};
