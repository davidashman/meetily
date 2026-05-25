'use client';

import { Suspense, useEffect, useState } from 'react';
import { useSearchParams } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Mic, X, Square } from 'lucide-react';

type Mode = 'prompt' | 'recording';

const COUNTDOWN_SECS = 30;
const R = 20; // SVG arc radius
const CIRC = 2 * Math.PI * R;

function fmt(s: number) {
  const m = Math.floor(s / 60);
  const sec = s % 60;
  return `${m}:${sec.toString().padStart(2, '0')}`;
}

function OverlayContent() {
  const params = useSearchParams();
  const initialMode = (params.get('mode') ?? 'prompt') as Mode;
  const appName = params.get('app') ?? 'Unknown App';
  const initialMeeting = params.get('meeting') ?? 'Meeting';

  const [mode, setMode] = useState<Mode>(initialMode);
  const [meetingName, setMeetingName] = useState(initialMeeting);
  const [countdown, setCountdown] = useState(COUNTDOWN_SECS);
  const [elapsed, setElapsed] = useState(0);
  const [busy, setBusy] = useState(false);

  // Auto-decline countdown (prompt mode only)
  useEffect(() => {
    if (mode !== 'prompt') return;
    const t = setInterval(() => {
      setCountdown(n => {
        if (n <= 1) {
          clearInterval(t);
          invoke('overlay_decline').catch(() => {});
          return 0;
        }
        return n - 1;
      });
    }, 1000);
    return () => clearInterval(t);
  }, [mode]);

  // Elapsed timer (recording mode)
  useEffect(() => {
    if (mode !== 'recording') return;
    const t = setInterval(() => setElapsed(e => e + 1), 1000);
    return () => clearInterval(t);
  }, [mode]);

  // overlay:state — transition prompt → recording in-place
  useEffect(() => {
    const un = listen<{ mode: Mode; meetingName: string }>('overlay:state', e => {
      setMode(e.payload.mode);
      setMeetingName(e.payload.meetingName);
      setElapsed(0);
      setBusy(false);
    });
    return () => { un.then(f => f()); };
  }, []);

  const handleRecord = async () => {
    if (busy) return;
    setBusy(true);
    await invoke('overlay_record', { meetingName }).catch(() => setBusy(false));
  };

  const handleDecline = async () => {
    if (busy) return;
    setBusy(true);
    await invoke('overlay_decline').catch(() => setBusy(false));
  };

  const handleStop = async () => {
    if (busy) return;
    setBusy(true);
    await invoke('overlay_stop').catch(() => setBusy(false));
  };

  const handleDismiss = () => getCurrentWindow().close();

  // SVG arc: full circle = COUNTDOWN_SECS, shrinks to 0
  const arcProgress = countdown / COUNTDOWN_SECS; // 1 → 0
  const dashOffset = CIRC * (1 - arcProgress);    // 0 → CIRC

  const card: React.CSSProperties = {
    background: 'rgba(18, 18, 28, 0.55)',
    backdropFilter: 'blur(20px)',
    WebkitBackdropFilter: 'blur(20px)',
    border: '1px solid rgba(255,255,255,0.07)',
    borderRadius: 14,
    display: 'flex',
    flexDirection: 'column',
    justifyContent: 'space-between',
    height: '100vh',
    padding: '12px 14px',
    boxSizing: 'border-box',
    userSelect: 'none',
    position: 'relative',
  };

  const dismissBtn: React.CSSProperties = {
    position: 'absolute',
    top: 6,
    right: 8,
    background: 'none',
    border: 'none',
    cursor: 'pointer',
    padding: 2,
    lineHeight: 1,
    color: 'rgba(255,255,255,0.35)',
    fontSize: 14,
  };

  if (mode === 'recording') {
    return (
      <div style={card} data-tauri-drag-region>
        <button style={dismissBtn} onClick={handleDismiss} title="Dismiss">×</button>
        <div data-tauri-drag-region>
          <p style={{ fontSize: 10, color: 'rgba(255,255,255,0.35)', letterSpacing: '0.08em', textTransform: 'uppercase', margin: 0 }} data-tauri-drag-region>
            Meetily Recording...
          </p>
          <p style={{ fontSize: 13, fontWeight: 600, color: '#fff', margin: '3px 0 0', lineHeight: 1.3 }} data-tauri-drag-region>
            <span style={{ color: '#60a5fa' }}>{appName}</span>
          </p>
        </div>

        <div style={{ display: 'flex', justifyContent: 'center', gap: 20, paddingBottom: 2 }}>
          {/* Stop — red circle with square stop icon */}
          <button
            onClick={handleStop}
            disabled={busy}
            title="Stop Recording"
            style={{
              width: 44, height: 44, borderRadius: '50%',
              background: busy ? '#7f1d1d' : '#dc2626',
              border: 'none', cursor: busy ? 'default' : 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              transition: 'background 0.15s',
              opacity: busy ? 0.6 : 1,
            }}
          >
            <Square size={18} color="#fff" fill="#fff" />
          </button>

          {/* Elapsed time — same slot as the X button */}
          <div style={{
            width: 44, height: 44,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
          }}>
            <span style={{ fontSize: 13, fontFamily: 'monospace', fontWeight: 600, color: '#fff' }}>
              {fmt(elapsed)}
            </span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div style={card} data-tauri-drag-region>
      <button style={dismissBtn} onClick={handleDismiss} title="Dismiss">×</button>
      <div data-tauri-drag-region>
        <p style={{ fontSize: 10, color: 'rgba(255,255,255,0.35)', letterSpacing: '0.08em', textTransform: 'uppercase', margin: 0 }} data-tauri-drag-region>
          Meetily
        </p>
        <p style={{ fontSize: 13, fontWeight: 600, color: '#fff', margin: '3px 0 0', lineHeight: 1.3 }} data-tauri-drag-region>
          <span style={{ color: '#60a5fa' }}>{appName}</span>
        </p>
      </div>

      <div style={{ display: 'flex', justifyContent: 'center', gap: 20, paddingBottom: 2 }}>
        {/* Record — red circle with mic icon */}
        <button
          onClick={handleRecord}
          disabled={busy}
          title="Start Recording"
          style={{
            width: 44, height: 44, borderRadius: '50%',
            background: busy ? '#7f1d1d' : '#dc2626',
            border: 'none', cursor: busy ? 'default' : 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            transition: 'background 0.15s',
            opacity: busy ? 0.6 : 1,
          }}
        >
          <Mic size={20} color="#fff" />
        </button>

        {/* Decline — grey circle with X, blue arc countdown border */}
        <div style={{ position: 'relative', width: 44, height: 44 }}>
          <svg
            width={44} height={44}
            style={{ position: 'absolute', top: 0, left: 0, transform: 'rotate(-90deg)' }}
          >
            {/* Track */}
            <circle cx={22} cy={22} r={R} fill="none" stroke="rgba(255,255,255,0.08)" strokeWidth={2.5} />
            {/* Progress arc */}
            <circle
              cx={22} cy={22} r={R}
              fill="none"
              stroke="#3b82f6"
              strokeWidth={2.5}
              strokeDasharray={CIRC}
              strokeDashoffset={dashOffset}
              strokeLinecap="round"
              style={{ transition: 'stroke-dashoffset 0.9s linear' }}
            />
          </svg>
          <button
            onClick={handleDecline}
            disabled={busy}
            title={`Decline (${countdown}s)`}
            style={{
              position: 'absolute', top: 3, left: 3,
              width: 38, height: 38, borderRadius: '50%',
              background: 'rgba(255,255,255,0.12)',
              border: 'none', cursor: busy ? 'default' : 'pointer',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              transition: 'background 0.15s',
              opacity: busy ? 0.5 : 1,
            }}
          >
            <X size={16} color="#d1d5db" />
          </button>
        </div>
      </div>
    </div>
  );
}

export default function OverlayPage() {
  return (
    <Suspense>
      <OverlayContent />
    </Suspense>
  );
}
