"use client";
import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

type AnalysisStatus = 'idle' | 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled';

interface AnalysisResult {
  status: string;
  markdown: string | null;
  error: string | null;
}

export function useAnalysisGeneration(meetingId: string) {
  const [status, setStatus] = useState<AnalysisStatus>('idle');
  const [markdown, setMarkdown] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pollIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollIntervalRef.current) {
      clearInterval(pollIntervalRef.current);
      pollIntervalRef.current = null;
    }
  }, []);

  const applyResult = useCallback((result: AnalysisResult) => {
    const s = result.status as AnalysisStatus;
    setStatus(s);
    if (result.markdown) setMarkdown(result.markdown);
    if (result.error) setError(result.error);
    if (s === 'completed' || s === 'failed' || s === 'cancelled' || s === 'idle') {
      stopPolling();
    }
  }, [stopPolling]);

  const startPolling = useCallback(() => {
    if (pollIntervalRef.current) return;
    pollIntervalRef.current = setInterval(async () => {
      try {
        const result = await invoke<AnalysisResult>('api_get_analysis', { meetingId });
        applyResult(result);
      } catch (e) {
        console.error('Failed to poll analysis status:', e);
      }
    }, 1500);
  }, [meetingId, applyResult]);

  // Load initial state on mount
  useEffect(() => {
    let cancelled = false;
    invoke<AnalysisResult>('api_get_analysis', { meetingId })
      .then((result) => {
        if (cancelled) return;
        applyResult(result);
        if (result.status === 'pending' || result.status === 'processing') {
          startPolling();
        }
      })
      .catch((e) => console.error('Failed to fetch analysis:', e));
    return () => {
      cancelled = true;
      stopPolling();
    };
  }, [meetingId]); // eslint-disable-line react-hooks/exhaustive-deps

  const triggerAnalysis = useCallback(async (transcriptText: string) => {
    setStatus('pending');
    setError(null);
    try {
      await invoke('api_process_analysis', { meetingId, text: transcriptText });
      startPolling();
    } catch (e: any) {
      setStatus('failed');
      setError(String(e));
    }
  }, [meetingId, startPolling]);

  const cancelAnalysis = useCallback(async () => {
    stopPolling();
    try {
      await invoke('api_cancel_analysis', { meetingId });
      setStatus('cancelled');
    } catch (e) {
      console.error('Failed to cancel analysis:', e);
    }
  }, [meetingId, stopPolling]);

  return { status, markdown, error, triggerAnalysis, cancelAnalysis };
}
