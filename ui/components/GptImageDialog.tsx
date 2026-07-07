'use client'

import { LoaderCircleIcon, RotateCcwIcon, SparklesIcon } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import type { TextNodeEntry } from '@/hooks/useCurrentPage'
import {
  startCodexImageGeneration,
  useGetCodexAuthStatus,
  useGetConfig,
} from '@/lib/api/default/default'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useJobsStore } from '@/lib/stores/jobsStore'

const DEFAULT_MODEL = 'gpt-image-2'
const DEFAULT_QUALITY = 'high'
const DEFAULT_INSTRUCTIONS = 'Generate a manga repair layer from the selected text block only.'

type GptImageDialogProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  pageId: string | null
  textNode: TextNodeEntry | null
}

export function buildGptImagePrompt(textNode: TextNodeEntry | null): string {
  const original = textNode?.data.text?.trim() || '(empty original text)'
  const translation = textNode?.data.translation?.trim() || '(empty translation)'
  const transform = textNode?.transform
  const geometry = transform
    ? `Text block: x=${formatNumber(transform.x)}, y=${formatNumber(transform.y)}, width=${formatNumber(transform.width)}, height=${formatNumber(transform.height)}, rotation=${formatNumber(transform.rotationDeg ?? 0)} degrees.`
    : 'Text block geometry is unavailable.'

  return [
    'Create a transparent repair layer for the selected manga text block.',
    `Original text: ${original}`,
    `Translation: ${translation}`,
    geometry,
    'Replace only the selected original lettering with the translation.',
    'Preserve the local artwork, screentones, speech bubble edge, perspective, font weight, font size, and text angle.',
    'Do not change content outside the selected text block or mask.',
  ].join('\n')
}

export function GptImageDialog({
  open,
  onOpenChange,
  pageId,
  textNode,
}: GptImageDialogProps) {
  const { t } = useTranslation()
  const [prompt, setPrompt] = useState('')
  const [model, setModel] = useState(DEFAULT_MODEL)
  const [quality, setQuality] = useState(DEFAULT_QUALITY)
  const [instructions, setInstructions] = useState(DEFAULT_INSTRUCTIONS)
  const [busy, setBusy] = useState(false)
  const { data: auth } = useGetCodexAuthStatus({ query: { enabled: open } })
  const { data: config } = useGetConfig({ query: { enabled: open } })
  const setShowRepairResultLayers = useEditorUiStore((s) => s.setShowRepairResultLayers)
  const showError = useEditorUiStore((s) => s.showError)
  const isProcessing = useJobsStore((s) =>
    Object.values(s.jobs).some((job) => job.status === 'running'),
  )

  const builtInPrompt = useMemo(() => buildGptImagePrompt(textNode), [textNode])
  const signedIn = auth?.signedIn === true
  const trimmedPrompt = prompt.trim()
  const canGenerate =
    signedIn && !!pageId && !!textNode && !!trimmedPrompt && !isProcessing && !busy

  useEffect(() => {
    if (open) setPrompt(builtInPrompt)
  }, [builtInPrompt, open])

  useEffect(() => {
    if (open) setModel(config?.ai_models?.gpt_image?.trim() || DEFAULT_MODEL)
  }, [config?.ai_models?.gpt_image, open])

  const handleSubmit = async () => {
    if (!canGenerate || !pageId || !textNode) return
    setBusy(true)
    try {
      setShowRepairResultLayers(true)
      await startCodexImageGeneration({
        pageId,
        textNodeId: textNode.id,
        prompt: trimmedPrompt,
        model: model.trim() || DEFAULT_MODEL,
        instructions: instructions.trim() || undefined,
        quality: quality.trim() || DEFAULT_QUALITY,
      })
      onOpenChange(false)
    } catch (err) {
      showError(String(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent data-testid='gpt-image-dialog' className='w-[560px] max-w-[92vw] gap-4 p-4'>
        <DialogHeader className='gap-1'>
          <DialogTitle className='text-base'>{t('ai.gptImageTitle')}</DialogTitle>
          <DialogDescription>{t('ai.gptImageDescription')}</DialogDescription>
        </DialogHeader>

        <div className='grid gap-3 text-xs'>
          <div className='grid gap-2 sm:grid-cols-[1fr_120px]'>
            <div className='space-y-1.5'>
              <Label
                htmlFor='gpt-image-model'
                className='text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'
              >
                {t('ai.model')}
              </Label>
              <Input
                id='gpt-image-model'
                data-testid='gpt-image-model'
                value={model}
                onChange={(event) => setModel(event.target.value)}
                className='h-8 text-xs'
              />
            </div>
            <div className='space-y-1.5'>
              <Label
                htmlFor='gpt-image-quality'
                className='text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'
              >
                {t('ai.quality')}
              </Label>
              <Input
                id='gpt-image-quality'
                data-testid='gpt-image-quality'
                value={quality}
                onChange={(event) => setQuality(event.target.value)}
                className='h-8 text-xs'
              />
            </div>
          </div>

          <div className='space-y-1.5'>
            <Label
              htmlFor='gpt-image-instructions'
              className='text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'
            >
              {t('ai.instructions')}
            </Label>
            <Input
              id='gpt-image-instructions'
              data-testid='gpt-image-instructions'
              value={instructions}
              onChange={(event) => setInstructions(event.target.value)}
              className='h-8 text-xs'
            />
          </div>

          <div className='space-y-1.5'>
            <div className='flex items-center justify-between gap-2'>
              <Label
                htmlFor='gpt-image-prompt'
                className='text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'
              >
                {t('ai.prompt')}
              </Label>
              <Button
                type='button'
                size='xs'
                variant='ghost'
                className='h-6 gap-1 px-1.5 text-[11px]'
                onClick={() => setPrompt(builtInPrompt)}
              >
                <RotateCcwIcon className='size-3' />
                {t('ai.useBuiltInPrompt')}
              </Button>
            </div>
            <Textarea
              id='gpt-image-prompt'
              data-testid='gpt-image-prompt'
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              rows={9}
              className='min-h-48 resize-y text-xs leading-snug md:text-xs'
            />
          </div>

          {!textNode && <p className='text-xs text-muted-foreground'>{t('ai.textNodeRequired')}</p>}
          {!signedIn && <p className='text-xs text-muted-foreground'>{t('ai.signInRequired')}</p>}
        </div>

        <DialogFooter>
          <Button
            type='button'
            variant='outline'
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t('common.cancel')}
          </Button>
          <Button
            type='button'
            data-testid='gpt-image-submit'
            disabled={!canGenerate}
            onClick={() => void handleSubmit()}
          >
            {busy || isProcessing ? (
              <LoaderCircleIcon className='size-4 animate-spin' />
            ) : (
              <SparklesIcon className='size-4' />
            )}
            {t('ai.generate')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function formatNumber(value: number): string {
  return Number.isFinite(value) ? value.toFixed(1) : '0.0'
}
