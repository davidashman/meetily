"use client";
import { Transcript } from '@/types';
import { useAnalysisGeneration } from '@/hooks/meeting-details/useAnalysisGeneration';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface AnalysisPanelProps {
  meetingId: string;
  transcripts: Transcript[];
}

export function AnalysisPanel({ meetingId, transcripts }: AnalysisPanelProps) {
  const { status, markdown, error, triggerAnalysis, cancelAnalysis } =
    useAnalysisGeneration(meetingId);

  const isLoading = status === 'pending' || status === 'processing';

  const handleRunAnalysis = () => {
    const text = transcripts.map((t) => t.text).join('\n');
    triggerAnalysis(text);
  };

  return (
    <div className="flex-1 min-w-0 flex flex-col bg-background overflow-hidden">
      {isLoading ? (
        <div className="flex flex-col items-center justify-center flex-1 gap-4">
          <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500" />
          <p className="text-muted-foreground text-sm">Analyzing transcript…</p>
          <button
            onClick={cancelAnalysis}
            className="text-xs text-muted-foreground underline hover:text-foreground"
          >
            Cancel
          </button>
        </div>
      ) : status === 'failed' ? (
        <div className="flex flex-col items-center justify-center flex-1 gap-4 p-6">
          <p className="text-destructive text-sm text-center">
            Analysis failed: {error ?? 'Unknown error'}
          </p>
          <button
            onClick={handleRunAnalysis}
            disabled={transcripts.length === 0}
            className="px-4 py-2 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-50"
          >
            Retry Analysis
          </button>
        </div>
      ) : markdown ? (
        <div className="flex flex-col h-full overflow-hidden">
          <div className="flex items-center justify-end px-4 py-2 border-b border-border">
            <button
              onClick={handleRunAnalysis}
              disabled={transcripts.length === 0}
              className="text-xs text-muted-foreground hover:text-foreground underline disabled:opacity-50"
            >
              Re-run Analysis
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-6">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              className="prose prose-sm dark:prose-invert max-w-none"
            >
              {markdown}
            </ReactMarkdown>
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center flex-1 gap-4 p-6">
          <div className="text-center max-w-sm">
            <h3 className="font-medium text-foreground mb-2">Deep Analysis</h3>
            <p className="text-muted-foreground text-sm mb-6">
              Generate a deep analysis of the transcript including discussion themes, key findings,
              interpretations, risks, and recommendations.
            </p>
            <button
              onClick={handleRunAnalysis}
              disabled={transcripts.length === 0}
              className="px-4 py-2 rounded bg-primary text-primary-foreground text-sm hover:bg-primary/90 disabled:opacity-50"
            >
              {transcripts.length === 0 ? 'No transcript available' : 'Run Analysis'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
