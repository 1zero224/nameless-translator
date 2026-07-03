'use client'

import {
  BandageIcon,
  CheckCircle2Icon,
  CircleDashedIcon,
  ImagePlusIcon,
  Languages,
  LoaderCircleIcon,
  SparklesIcon,
  Trash2Icon,
  TriangleAlertIcon,
  TypeIcon,
} from 'lucide-react'
import { motion } from 'motion/react'
import type React from 'react'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
import { Button } from '@/components/ui/button'
import { DraftTextarea } from '@/components/ui/draft-textarea'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useCurrentPage, useTextNodes, type TextNodeEntry } from '@/hooks/useCurrentPage'
import { getConfig, startPipeline, useGetCurrentLlm } from '@/lib/api/default/default'
import type { TextDataPatch } from '@/lib/api/schemas'
import { openImageLayerFile } from '@/lib/io/openFiles'
import {
  applyOp,
  queueAutoRender,
  reorderPageTextNodes,
  uploadRepairImageLayer,
} from '@/lib/io/scene'
import { ops } from '@/lib/ops'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useJobsStore } from '@/lib/stores/jobsStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'
import {
  canChooseResult,
  normalizeWorkflow,
  setResultMode,
  toggleMode,
  type TextResultMode,
  type TextWorkflow,
  type WorkflowStatus,
} from '@/lib/workflow'

export function TextBlocksPanel() {
  const { t } = useTranslation()
  const page = useCurrentPage()
  const textNodes = useTextNodes()
  useEffect(() => {
    if (process.env.NODE_ENV !== 'production') {
      console.debug(
        '[reorder] Text nodes order:',
        textNodes.map((n) => n.id),
      )
    }
  }, [textNodes])
  const selectedIds = useSelectionStore((s) => s.nodeIds)
  const select = useSelectionStore((s) => s.select)
  const clearSelection = useSelectionStore((s) => s.clear)
  const { data: llm } = useGetCurrentLlm()
  const llmReady = llm?.status === 'ready'
  const isProcessing = useJobsStore((s) =>
    Object.values(s.jobs).some((j) => j.status === 'running'),
  )
  const readingOrder = useEditorUiStore((s) => s.readingOrder)
  const setReadingOrder = useEditorUiStore((s) => s.setReadingOrder)

  if (!page) {
    return (
      <div className='flex flex-1 items-center justify-center text-xs text-muted-foreground'>
        {t('textBlocks.emptyPrompt')}
      </div>
    )
  }

  const selectedIndex = textNodes.findIndex((n) => selectedIds.has(n.id))
  const accordionValue = selectedIndex >= 0 ? selectedIndex.toString() : ''

  const patchText = async (nodeId: string, patch: TextDataPatch) => {
    await applyOp(
      ops.updateNode(page.id, nodeId, {
        data: { text: patch },
      }),
    )
    queueAutoRender(page.id)
  }

  const patchWorkflow = async (nodeId: string, workflow: TextWorkflow) => {
    await applyOp(
      ops.updateNode(page.id, nodeId, {
        data: { text: { workflow } },
      }),
    )
    queueAutoRender(page.id)
  }

  const removeNode = async (nodeId: string) => {
    const node = page.nodes[nodeId]
    if (!node) return
    const idx = Object.keys(page.nodes).indexOf(nodeId)
    await applyOp(ops.removeNode(page.id, nodeId, node, idx < 0 ? 0 : idx))
    clearSelection()
    queueAutoRender(page.id)
  }

  const generate = async (nodeId: string) => {
    if (!page) return
    const cfg = await getConfig()
    const translator = cfg.pipeline?.translator || 'llm'
    const renderer = cfg.pipeline?.renderer || 'koharu-renderer'
    const editor = useEditorUiStore.getState()
    const prefs = usePreferencesStore.getState()
    // Keep rendering page-scoped, but constrain translation to the clicked block.
    await startPipeline({
      steps: [translator, renderer],
      pages: [page.id],
      textNodeIds: [nodeId],
      targetLanguage: editor.selectedLanguage,
      systemPrompt: prefs.customSystemPrompt,
      defaultFont: prefs.defaultFont,
      readingOrder: editor.readingOrder === 'custom' ? undefined : editor.readingOrder,
    })
  }

  const bindRepairLayer = async (nodeId: string) => {
    const file = await openImageLayerFile()
    if (!file) return
    await uploadRepairImageLayer(page.id, nodeId, file)
  }

  return (
    <div className='flex min-h-0 flex-1 flex-col' data-testid='panels-textblocks'>
      <div className='flex items-center justify-between border-b border-border px-2 py-1.5 text-xs font-semibold tracking-wide text-muted-foreground uppercase'>
        <span data-testid='textblocks-count' data-count={textNodes.length}>
          {t('textBlocks.title', { count: textNodes.length })}
        </span>
        <div className='flex items-center gap-1.5'>
          <span className='font-normal uppercase opacity-50'>{t('textBlocks.readingOrder')}:</span>
          <Select
            value={readingOrder}
            onValueChange={async (val: 'rtl' | 'ltr' | 'custom') => {
              if (process.env.NODE_ENV !== 'production') {
                console.debug('[reorder] Changing reading order to:', val)
              }

              if (val === 'custom') {
                setReadingOrder(val)
                return
              }

              try {
                await reorderPageTextNodes(page.id, val)
                setReadingOrder(val)
              } catch (err) {
                console.error('[reorder] Failed to reorder text nodes:', err)
                useEditorUiStore.getState().showError(String(err))
              }
            }}
          >
            <SelectTrigger
              className='h-5 w-32 gap-1 border-none bg-transparent px-1.5 text-[10px] font-semibold uppercase hover:bg-accent focus:ring-0'
              aria-label={t('textBlocks.readingOrder')}
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value='rtl' className='text-[10px] font-semibold'>
                {t('textBlocks.readingOrderRtl')}
              </SelectItem>
              <SelectItem value='ltr' className='text-[10px] font-semibold'>
                {t('textBlocks.readingOrderLtr')}
              </SelectItem>
              <SelectItem value='custom' className='text-[10px] font-semibold'>
                {t('textBlocks.readingOrderCustom')}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      <ScrollArea
        key={page.id}
        className='min-h-0 flex-1'
        viewportClassName='pb-1'
        data-testid='textblocks-scroll'
      >
        <div className='p-2'>
          {textNodes.length === 0 ? (
            <p className='rounded-md border border-dashed border-border p-2 text-xs text-muted-foreground'>
              {t('textBlocks.none')}
            </p>
          ) : (
            <Accordion
              data-testid='textblocks-accordion'
              type='single'
              collapsible
              value={accordionValue}
              onValueChange={(value) => {
                if (!value) {
                  clearSelection()
                  return
                }
                const idx = Number(value)
                const node = textNodes[idx]
                if (node) select(node.id, false)
              }}
              className='flex flex-col gap-1'
            >
              {textNodes.map((node, index) => (
                <BlockCard
                  key={node.id}
                  node={node}
                  index={index}
                  selected={selectedIds.has(node.id)}
                  onToggleSelect={() => select(node.id, true)}
                  onPatch={(patch) => void patchText(node.id, patch)}
                  onWorkflow={(workflow) => void patchWorkflow(node.id, workflow)}
                  onDelete={() => void removeNode(node.id)}
                  onGenerate={() => void generate(node.id)}
                  onBindRepairLayer={() => void bindRepairLayer(node.id)}
                  processing={isProcessing}
                  llmReady={llmReady}
                />
              ))}
            </Accordion>
          )}
        </div>
      </ScrollArea>
    </div>
  )
}

type BlockCardProps = {
  node: TextNodeEntry
  index: number
  selected: boolean
  onToggleSelect: () => void
  onPatch: (patch: TextDataPatch) => void
  onWorkflow: (workflow: TextWorkflow) => void
  onDelete: () => void
  onGenerate: () => void
  onBindRepairLayer: () => void
  processing: boolean
  llmReady: boolean
}

function BlockCard({
  node,
  index,
  selected,
  onToggleSelect,
  onPatch,
  onWorkflow,
  onDelete,
  onGenerate,
  onBindRepairLayer,
  processing,
  llmReady,
}: BlockCardProps) {
  const { t } = useTranslation()
  const data = node.data
  const workflow = normalizeWorkflow(data)
  const hasOcr = !!data.text?.trim()
  const hasTranslation = !!data.translation?.trim()
  const preview = data.translation?.trim() || data.text?.trim()

  return (
    <motion.div
      data-testid={`textblock-card-${index}`}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2, delay: index * 0.03 }}
    >
      <AccordionItem
        value={index.toString()}
        data-selected={selected}
        className='overflow-hidden rounded-md bg-card/90 text-xs ring-1 ring-border data-[selected=true]:ring-primary'
      >
        <AccordionTrigger
          onClick={(e) => {
            if (e.shiftKey || e.ctrlKey || e.metaKey) {
              e.preventDefault()
              e.stopPropagation()
              onToggleSelect()
            }
          }}
          className='flex w-full cursor-pointer items-center gap-1.5 px-2 py-1.5 text-left transition outline-none hover:no-underline data-[state=open]:bg-accent [&>svg]:hidden'
        >
          <span
            className={`shrink-0 rounded-md px-1.5 py-0.5 text-center text-[10px] font-medium text-white tabular-nums ${
              selected ? 'bg-primary' : 'bg-muted-foreground/60'
            }`}
            style={{ minWidth: '1.5rem' }}
          >
            {index + 1}
          </span>
          <div className='flex min-w-0 flex-1 items-center gap-1'>
            <span
              className={`shrink-0 rounded-sm px-1 py-0.5 text-[9px] font-medium uppercase ${
                hasOcr ? 'bg-rose-400/70 text-white' : 'bg-muted text-muted-foreground/50'
              }`}
            >
              {t('textBlocks.ocrBadge')}
            </span>
            <span
              className={`shrink-0 rounded-sm px-1 py-0.5 text-[9px] font-medium uppercase ${
                hasTranslation ? 'bg-rose-400/70 text-white' : 'bg-muted text-muted-foreground/50'
              }`}
            >
              {t('textBlocks.translationBadge')}
            </span>
            {preview && (
              <p className='line-clamp-1 min-w-0 flex-1 text-xs text-muted-foreground'>{preview}</p>
            )}
          </div>
        </AccordionTrigger>
        <AccordionContent className='px-2 pt-1.5 pb-2 shadow-[inset_0_1px_0_0_var(--color-border)]'>
          <div className='space-y-1.5'>
            <WorkflowControls workflow={workflow} onWorkflow={onWorkflow} />
            {workflow.modes?.includes('repair') && (
              <div className='flex justify-end'>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      aria-label={workflow.repairLayer ? '替换修图图层' : '绑定修图图层'}
                      variant='ghost'
                      size='icon-xs'
                      disabled={processing}
                      onClick={onBindRepairLayer}
                      className='size-5'
                    >
                      <ImagePlusIcon className='size-3' />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent side='left' sideOffset={4}>
                    {workflow.repairLayer ? '替换修图图层' : '绑定修图图层'}
                  </TooltipContent>
                </Tooltip>
              </div>
            )}
            <div className='flex flex-col gap-0.5'>
              <span className='text-[10px] text-muted-foreground uppercase'>
                {t('textBlocks.ocrLabel')}
              </span>
              <DraftTextarea
                data-testid={`textblock-ocr-${index}`}
                value={data.text ?? ''}
                placeholder={t('textBlocks.addOcrPlaceholder')}
                rows={2}
                onValueChange={(value) => onPatch({ text: value })}
                className='min-h-0 resize-none px-1.5 py-1 text-xs'
              />
            </div>
            <div className='flex flex-col gap-0.5'>
              <div className='flex items-center justify-between'>
                <span className='text-[10px] text-muted-foreground uppercase'>
                  {t('textBlocks.translationLabel')}
                </span>
                <div className='flex items-center gap-0.5'>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        data-testid={`textblock-delete-${index}`}
                        aria-label={t('workspace.deleteBlock')}
                        variant='ghost'
                        size='icon-xs'
                        disabled={processing}
                        onClick={onDelete}
                        className='size-5 text-rose-600 hover:text-rose-600'
                      >
                        <Trash2Icon className='size-3' />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side='left' sideOffset={4}>
                      {t('workspace.deleteBlock')}
                    </TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        data-testid={`textblock-generate-${index}`}
                        aria-label={t('llm.generateTooltip')}
                        variant='ghost'
                        size='icon-xs'
                        disabled={!llmReady || processing}
                        onClick={onGenerate}
                        className='size-5'
                      >
                        {processing ? (
                          <LoaderCircleIcon className='size-3 animate-spin' />
                        ) : (
                          <Languages className='size-3' />
                        )}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side='left' sideOffset={4}>
                      {t('llm.generateTooltip')}
                    </TooltipContent>
                  </Tooltip>
                </div>
              </div>
              <DraftTextarea
                data-testid={`textblock-translation-${index}`}
                value={data.translation ?? ''}
                placeholder={t('textBlocks.addTranslationPlaceholder')}
                rows={2}
                onValueChange={(value) => onPatch({ translation: value })}
                className='min-h-0 resize-none px-1.5 py-1 text-xs'
              />
            </div>
            <WorkflowTrace workflow={workflow} />
          </div>
        </AccordionContent>
      </AccordionItem>
    </motion.div>
  )
}

function WorkflowControls({
  workflow,
  onWorkflow,
}: {
  workflow: TextWorkflow
  onWorkflow: (workflow: TextWorkflow) => void
}) {
  const modes = workflow.modes ?? []
  return (
    <div className='rounded-md border border-border/70 bg-muted/30 p-1.5'>
      <div className='mb-1.5 flex items-center justify-between gap-2'>
        <span className='text-[10px] font-medium text-muted-foreground uppercase'>Workflow</span>
        <div className='flex items-center gap-1'>
          <StatusPill status={workflow.letteringStatus ?? 'pending'} label='嵌字' />
          <StatusPill status={workflow.repairStatus ?? 'pending'} label='修图' />
        </div>
      </div>
      <div className='grid grid-cols-2 gap-1'>
        <ModeButton
          active={modes.includes('lettering')}
          icon={TypeIcon}
          label='嵌字模式'
          onClick={() => onWorkflow(toggleMode(workflow, 'lettering'))}
        />
        <ModeButton
          active={modes.includes('repair')}
          icon={BandageIcon}
          label='修图模式'
          onClick={() => onWorkflow(toggleMode(workflow, 'repair'))}
        />
      </div>
      {canChooseResult(workflow) && (
        <div className='mt-1.5 grid grid-cols-2 gap-1 rounded-md bg-background/70 p-1'>
          <ResultButton
            active={workflow.resultMode === 'lettering'}
            mode='lettering'
            label='导出嵌字'
            onClick={(mode) => onWorkflow(setResultMode(workflow, mode))}
          />
          <ResultButton
            active={workflow.resultMode === 'repair'}
            mode='repair'
            label='导出修图'
            onClick={(mode) => onWorkflow(setResultMode(workflow, mode))}
          />
        </div>
      )}
    </div>
  )
}

function ModeButton({
  active,
  icon: Icon,
  label,
  onClick,
}: {
  active: boolean
  icon: React.ComponentType<{ className?: string }>
  label: string
  onClick: () => void
}) {
  return (
    <button
      type='button'
      data-active={active ? 'true' : 'false'}
      onClick={onClick}
      className='flex h-7 cursor-pointer items-center justify-center gap-1 rounded-md border border-border bg-background px-2 text-[11px] font-medium text-muted-foreground transition-colors data-[active=true]:border-primary/50 data-[active=true]:bg-primary/10 data-[active=true]:text-primary hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none'
    >
      <Icon className='size-3.5' />
      <span>{label}</span>
    </button>
  )
}

function ResultButton({
  active,
  mode,
  label,
  onClick,
}: {
  active: boolean
  mode: TextResultMode
  label: string
  onClick: (mode: TextResultMode) => void
}) {
  return (
    <button
      type='button'
      data-active={active ? 'true' : 'false'}
      onClick={() => onClick(mode)}
      className='h-6 cursor-pointer rounded px-2 text-[10px] font-semibold text-muted-foreground uppercase transition-colors data-[active=true]:bg-primary data-[active=true]:text-primary-foreground hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none'
    >
      {label}
    </button>
  )
}

function StatusPill({ status, label }: { status: WorkflowStatus; label: string }) {
  const Icon =
    status === 'running'
      ? LoaderCircleIcon
      : status === 'succeeded'
        ? CheckCircle2Icon
        : status === 'failed'
          ? TriangleAlertIcon
          : CircleDashedIcon
  return (
    <span
      data-status={status}
      className='inline-flex items-center gap-0.5 rounded-full border border-border bg-background px-1.5 py-0.5 text-[9px] text-muted-foreground data-[status=failed]:border-destructive/40 data-[status=failed]:text-destructive data-[status=succeeded]:border-emerald-500/40 data-[status=succeeded]:text-emerald-600'
    >
      <Icon className={`size-2.5 ${status === 'running' ? 'animate-spin' : ''}`} />
      {label}
    </span>
  )
}

function WorkflowTrace({ workflow }: { workflow: TextWorkflow }) {
  const fontTrace = workflow.fontTrace
  const selectedFont = fontTrace?.selectedFont
  const primaryCategory = formatFontCategory(fontTrace?.primaryCategory)
  const secondaryCategory = formatFontCategory(fontTrace?.secondaryCategory)
  const candidateFonts = fontTrace?.candidateFonts?.filter(Boolean).slice(0, 6) ?? []
  const fontNotes = fontTrace?.notes?.filter(Boolean).slice(0, 3) ?? []
  const repairModel = workflow.repairTrace?.model
  const repairError = workflow.repairTrace?.error
  if (
    !selectedFont &&
    !primaryCategory &&
    !secondaryCategory &&
    candidateFonts.length === 0 &&
    fontNotes.length === 0 &&
    !repairModel &&
    !repairError
  ) {
    return null
  }
  return (
    <div className='space-y-1 rounded-md border border-border/60 bg-background/70 p-1.5 text-[10px] text-muted-foreground'>
      {(selectedFont || primaryCategory || secondaryCategory || candidateFonts.length > 0) && (
        <div className='flex items-center gap-1'>
          <SparklesIcon className='size-3 text-primary' />
          <span className='truncate'>
            字体:
            {primaryCategory && <span> {primaryCategory}</span>}
            {secondaryCategory && <span> / {secondaryCategory}</span>}
            {selectedFont && <span>{` -> ${selectedFont}`}</span>}
          </span>
        </div>
      )}
      {candidateFonts.length > 0 && (
        <div className='truncate pl-4'>候选: {candidateFonts.join(' / ')}</div>
      )}
      {fontNotes.map((note) => (
        <div key={note} className='truncate pl-4'>
          {note}
        </div>
      ))}
      {repairModel && (
        <div className='flex items-center gap-1'>
          <BandageIcon className='size-3 text-primary' />
          <span className='truncate'>修图: {repairModel}</span>
        </div>
      )}
      {repairError && (
        <div className='flex items-center gap-1 text-destructive'>
          <TriangleAlertIcon className='size-3' />
          <span className='truncate'>{repairError}</span>
        </div>
      )}
    </div>
  )
}

function formatFontCategory(value?: string | null): string | null {
  switch (value) {
    case 'serif':
      return '有衬线'
    case 'sans_serif':
      return '无衬线'
    case 'gothic':
      return '黑体'
    case 'round':
      return '圆体'
    case 'mincho':
      return '宋体'
    case 'kai':
      return '楷体'
    default:
      return value?.trim() || null
  }
}
