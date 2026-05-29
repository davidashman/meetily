"use client";

import { useState, useCallback, useRef } from 'react';
import { Transcript } from '@/types';
import { useAnalysisGeneration } from '@/hooks/meeting-details/useAnalysisGeneration';
import { useConfig } from '@/contexts/ConfigContext';
import { BlockNoteSummaryView, BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { ModelConfig, ModelSettingsModal } from '@/components/ModelSettingsModal';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  Dialog,
  DialogContent,
  DialogTrigger,
  DialogTitle,
} from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';
import { Sparkles, Settings, Copy, Square, Save, Loader2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface AnalysisPanelProps {
  meetingId: string;
  transcripts: Transcript[];
}

export function AnalysisPanel({ meetingId, transcripts }: AnalysisPanelProps) {
  const { modelConfig, setModelConfig } = useConfig();
  const { status, markdown, error, triggerAnalysis, cancelAnalysis } =
    useAnalysisGeneration(meetingId);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const [isDirty, setIsDirty] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  const analysisRef = useRef<BlockNoteSummaryViewRef>(null);

  const isLoading = status === 'pending' || status === 'processing';
  const hasAnalysis = !!markdown;

  const handleGenerate = useCallback(() => {
    const text = transcripts.map((t) => t.text).join('\n');
    triggerAnalysis(text);
  }, [transcripts, triggerAnalysis]);

  const handleSave = useCallback(async (data: { markdown?: string }) => {
    if (!data.markdown) return;
    setIsSaving(true);
    try {
      await invoke('api_save_analysis', { meetingId, markdown: data.markdown });
      setIsDirty(false);
      toast.success('Analysis saved');
    } catch (e) {
      toast.error('Failed to save analysis');
      console.error(e);
    } finally {
      setIsSaving(false);
    }
  }, [meetingId]);

  const handleSaveClick = useCallback(async () => {
    if (analysisRef.current) {
      await analysisRef.current.saveSummary();
    }
  }, []);

  const handleCopy = useCallback(async () => {
    if (!analysisRef.current) return;
    try {
      const text = await analysisRef.current.getMarkdown();
      await navigator.clipboard.writeText(text);
      toast.success('Analysis copied to clipboard');
    } catch {
      toast.error('Failed to copy analysis');
    }
  }, []);

  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey ?? null,
        ollamaEndpoint: config.ollamaEndpoint ?? null,
      });
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);
      toast.success('Model settings saved');
      setSettingsOpen(false);
    } catch {
      toast.error('Failed to save model settings');
    }
  };

  const generatorButtons = (
    <ButtonGroup>
      {isLoading ? (
        <Button
          variant="outline"
          size="sm"
          className="bg-gradient-to-r from-red-50 to-orange-50 hover:from-red-100 hover:to-orange-100 border-red-200 dark:from-blue-950/60 dark:to-purple-950/60 dark:hover:from-blue-900/60 dark:hover:to-purple-900/60 dark:border-blue-800/50 rounded-r-none xl:px-4"
          onClick={cancelAnalysis}
          title="Stop analysis generation"
        >
          <Square className="xl:mr-2" size={18} fill="currentColor" />
          <span className="hidden lg:inline xl:inline">Stop</span>
        </Button>
      ) : (
        <Button
          variant="outline"
          size="sm"
          className="bg-gradient-to-r from-blue-50 to-purple-50 hover:from-blue-100 hover:to-purple-100 border-blue-200 dark:from-blue-950/60 dark:to-purple-950/60 dark:hover:from-blue-900/60 dark:hover:to-purple-900/60 dark:border-blue-800/50 rounded-r-none xl:px-4"
          onClick={handleGenerate}
          disabled={transcripts.length === 0}
          title="Generate Deep Analysis"
        >
          <Sparkles className="xl:mr-2" size={18} />
          <span className="hidden lg:inline xl:inline">Generate Analysis</span>
        </Button>
      )}

      <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
        <DialogTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="rounded-l-none border-l-0"
            title="AI Model Settings"
          >
            <Settings />
            <span className="hidden lg:inline">AI Model</span>
          </Button>
        </DialogTrigger>
        <DialogContent aria-describedby={undefined}>
          <VisuallyHidden>
            <DialogTitle>Model Settings</DialogTitle>
          </VisuallyHidden>
          <ModelSettingsModal
            onSave={handleSaveModelConfig}
            modelConfig={modelConfig}
            setModelConfig={setModelConfig}
            skipInitialFetch={true}
          />
        </DialogContent>
      </Dialog>
    </ButtonGroup>
  );

  const updaterButtons = (
    <ButtonGroup>
      <Button
        variant="outline"
        size="sm"
        className={isDirty ? 'bg-green-200' : ''}
        onClick={handleSaveClick}
        disabled={isSaving}
        title={isSaving ? 'Saving…' : 'Save Changes'}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span className="hidden lg:inline">Saving…</span>
          </>
        ) : (
          <>
            <Save />
            <span className="hidden lg:inline">Save</span>
          </>
        )}
      </Button>

      <Button
        variant="outline"
        size="sm"
        onClick={handleCopy}
        disabled={!hasAnalysis}
        title="Copy Analysis"
        className="cursor-pointer"
      >
        <Copy />
        <span className="hidden lg:inline">Copy</span>
      </Button>
    </ButtonGroup>
  );

  return (
    <div className="flex-1 min-w-0 mb-2 flex flex-col bg-background overflow-hidden">
      {/* Top bar — only shown when analysis exists and not loading */}
      <div className="p-4 border-b border-border">
        {hasAnalysis && !isLoading && (
          <div className="flex items-center justify-center w-full pt-0 gap-2">
            <div className="flex-shrink-0">{generatorButtons}</div>
            <div className="flex-shrink-0">{updaterButtons}</div>
          </div>
        )}
      </div>

      {isLoading ? (
        <div className="flex flex-col h-full">
          <div className="flex items-center justify-center pt-8 pb-4">
            {generatorButtons}
          </div>
          <div className="flex items-center justify-center flex-1">
            <div className="text-center">
              <div className="inline-block animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-blue-500 mb-4" />
              <p className="text-muted-foreground">Generating analysis…</p>
            </div>
          </div>
        </div>
      ) : !hasAnalysis ? (
        <div className="flex flex-col h-full">
          <div className="flex items-center justify-center pt-8 pb-4">
            {generatorButtons}
          </div>
          <div className="flex items-center justify-center flex-1 p-6">
            <div className="text-center max-w-sm">
              <h3 className="font-medium text-foreground mb-2">Deep Analysis</h3>
              <p className="text-muted-foreground text-sm">
                Generate a deep analysis of the transcript including discussion themes,
                key findings, interpretations, risks, and recommendations.
              </p>
              {status === 'failed' && (
                <p className="text-destructive text-sm mt-4">{error ?? 'Analysis failed'}</p>
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto min-h-0">
          <BlockNoteSummaryView
            ref={analysisRef}
            summaryData={{ markdown }}
            onSave={handleSave}
            onDirtyChange={setIsDirty}
          />
        </div>
      )}
    </div>
  );
}
