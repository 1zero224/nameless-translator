'use client'

import { useQueryClient } from '@tanstack/react-query'
import {
  BotIcon,
  EyeIcon,
  LanguagesIcon,
  ListChecksIcon,
  LoaderCircleIcon,
  PaintbrushIcon,
  ScanIcon,
  ScanTextIcon,
  TriangleAlertIcon,
  TypeIcon,
  Wand2Icon,
} from 'lucide-react'
import { motion } from 'motion/react'
import { useEffect, useMemo, useState, type ComponentType } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { LlmModelSelect, type LlmModelOption } from '@/components/ui/llm-model-select'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Textarea } from '@/components/ui/textarea'
import { useScene } from '@/hooks/useScene'
import {
  deleteCurrentLlm,
  getConfig,
  getGetConfigQueryKey,
  patchConfig,
  putCurrentLlm,
  startPipeline,
  useGetCatalog,
  useGetConfig,
  useGetCurrentLlm,
  useGetEngineCatalog,
} from '@/lib/api/default/default'
import type {
  ConfigPatch,
  EngineCatalogEntry,
  LlmCatalog,
  LlmCatalogModel,
  LlmProviderCatalog,
  LlmTarget,
  PipelineConfig,
} from '@/lib/api/schemas'
import {
  buildAutomationPlan,
  buildAutomationSteps,
  type AutomationPlan,
} from '@/lib/automationPipeline'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useJobsStore } from '@/lib/stores/jobsStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

// ---------------------------------------------------------------------------
// Helpers (inlined from former llmTargets util)
// ---------------------------------------------------------------------------

function llmTargetKey(t: LlmTarget): string {
  return `${t.kind}:${t.providerId ?? ''}:${t.modelId}`
}

function sameLlmTarget(a?: LlmTarget | null, b?: LlmTarget | null): boolean {
  if (!a || !b) return false
  return (
    a.kind === b.kind &&
    a.modelId === b.modelId &&
    (a.providerId ?? null) === (b.providerId ?? null)
  )
}

type SelectableLlmModel = { model: LlmCatalogModel; provider?: LlmProviderCatalog }

const flattenCatalogModels = (catalog?: LlmCatalog): SelectableLlmModel[] => [
  ...(catalog?.localModels ?? []).map((model) => ({ model })),
  ...(catalog?.providers ?? [])
    .filter((p) => p.status === 'ready')
    .flatMap((p) => p.models.map((model) => ({ model, provider: p }))),
]

type PipelineEngineKey =
  | 'detector'
  | 'font_detector'
  | 'ocr'
  | 'translator'
  | 'inpainter'
  | 'repairer'
  | 'renderer'

const PIPELINE_PATCH_KEYS: Record<PipelineEngineKey, keyof NonNullable<ConfigPatch['pipeline']>> = {
  detector: 'detector',
  font_detector: 'fontDetector',
  ocr: 'ocr',
  translator: 'translator',
  inpainter: 'inpainter',
  repairer: 'repairer',
  renderer: 'renderer',
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CanvasToolbar() {
  return (
    <div className='flex items-center gap-2 border-b border-border/60 bg-card px-3 py-2 text-xs text-foreground'>
      <WorkflowButtons />
      <div className='flex-1' />
      <AutomationButtons />
      <ModelConfigPopover />
    </div>
  )
}

/** Currently-busy step (derived from jobsStore). */
function useCurrentStep(): string | null {
  const jobs = useJobsStore((s) => s.jobs)
  for (const j of Object.values(jobs)) {
    if (j.status === 'running' && j.progress?.step) return String(j.progress.step)
  }
  return null
}

function useIsProcessing(): boolean {
  const jobs = useJobsStore((s) => s.jobs)
  return Object.values(jobs).some((j) => j.status === 'running')
}

function WorkflowButtons() {
  const { t } = useTranslation()
  const { data: llmState } = useGetCurrentLlm()
  const llmReady = llmState?.status === 'ready'
  const pageId = useSelectionStore((s) => s.pageId)
  const hasPage = pageId !== null
  const isProcessing = useIsProcessing()
  const currentStep = useCurrentStep()

  /**
   * Run a pipeline step (or a small chain). `GET /config` is the single
   * source of truth for engine ids — every field has a serde default in
   * the Rust `PipelineConfig`, so we trust what the server returns and
   * never hard-code fallbacks here.
   *
   * Detect is the only multi-engine button. It runs native text/bubble
   * detection only; font detection belongs to project automation where it
   * can see workflow modes and translations.
   */
  const runStep = async (
    pick: (p: NonNullable<Awaited<ReturnType<typeof getConfig>>['pipeline']>) => string[],
  ) => {
    if (!pageId) return
    const cfg = await getConfig()
    if (!cfg.pipeline) return
    const steps = pick(cfg.pipeline).filter((s): s is string => !!s)
    if (steps.length === 0) return
    const editor = useEditorUiStore.getState()
    const prefs = usePreferencesStore.getState()
    await startPipeline({
      steps,
      pages: [pageId],
      targetLanguage: editor.selectedLanguage,
      systemPrompt: prefs.customSystemPrompt,
      defaultFont: prefs.defaultFont,
      readingOrder: editor.readingOrder === 'custom' ? undefined : editor.readingOrder,
    })
  }

  type PipelinePick = (
    p: NonNullable<Awaited<ReturnType<typeof getConfig>>['pipeline']>,
  ) => string[]
  const detectChain: PipelinePick = (p) => [
    p.detector!,
    p.segmenter!,
    p.bubble_segmenter!,
  ]
  const ocrChain: PipelinePick = (p) => [p.ocr!]
  const translateChain: PipelinePick = (p) => [p.translator!]
  const inpaintChain: PipelinePick = (p) => [p.inpainter!]
  const renderChain: PipelinePick = (p) => [p.renderer!]

  const isDetecting = currentStep === 'detect'
  const isOcr = currentStep === 'ocr'
  const isInpainting = currentStep === 'inpaint'
  const isTranslating = currentStep === 'llmGenerate'
  const isRendering = currentStep === 'render'

  return (
    <div className='flex items-center gap-0.5'>
      <Button
        variant='ghost'
        size='xs'
        onClick={() => void runStep(detectChain)}
        data-testid='toolbar-detect'
        data-step-running={isDetecting ? 'true' : 'false'}
        disabled={!hasPage || isProcessing}
      >
        {isDetecting ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <ScanIcon className='size-4' />
        )}
        {t('processing.detect')}
      </Button>
      <Separator orientation='vertical' className='mx-0.5 h-4' />
      <Button
        variant='ghost'
        size='xs'
        onClick={() => void runStep(ocrChain)}
        data-testid='toolbar-ocr'
        data-step-running={isOcr ? 'true' : 'false'}
        disabled={!hasPage || isProcessing}
      >
        {isOcr ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <ScanTextIcon className='size-4' />
        )}
        {t('processing.ocr')}
      </Button>
      <Separator orientation='vertical' className='mx-0.5 h-4' />
      <Button
        variant='ghost'
        size='xs'
        onClick={() => void runStep(translateChain)}
        disabled={!hasPage || !llmReady || isProcessing}
        data-testid='toolbar-translate'
        data-step-running={isTranslating ? 'true' : 'false'}
      >
        {isTranslating ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <LanguagesIcon className='size-4' />
        )}
        {t('llm.generate')}
      </Button>
      <Separator orientation='vertical' className='mx-0.5 h-4' />
      <Button
        variant='ghost'
        size='xs'
        onClick={() => void runStep(inpaintChain)}
        data-testid='toolbar-inpaint'
        data-step-running={isInpainting ? 'true' : 'false'}
        disabled={!hasPage || isProcessing}
      >
        {isInpainting ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <Wand2Icon className='size-4' />
        )}
        {t('mask.inpaint')}
      </Button>
      <Separator orientation='vertical' className='mx-0.5 h-4' />
      <Button
        variant='ghost'
        size='xs'
        onClick={() => void runStep(renderChain)}
        data-testid='toolbar-render'
        data-step-running={isRendering ? 'true' : 'false'}
        disabled={!hasPage || isProcessing}
      >
        {isRendering ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <TypeIcon className='size-4' />
        )}
        {t('llm.render')}
      </Button>
    </div>
  )
}

function AutomationButtons() {
  const { data: config } = useGetConfig()
  const { scene } = useScene()
  const hasProject = scene !== null
  const isProcessing = useIsProcessing()
  const jobs = useJobsStore((s) => s.jobs)
  const [automationOperationId, setAutomationOperationId] = useState<string | null>(null)
  const automationPlan = useMemo(
    () => buildAutomationPlan(config?.pipeline ?? {}, scene),
    [config?.pipeline, scene],
  )
  const automationJob = automationOperationId ? jobs[automationOperationId] : undefined
  const isAutomationRunning = automationJob?.status === 'running'

  useEffect(() => {
    if (!automationOperationId) return
    const job = jobs[automationOperationId]
    if (job && job.status !== 'running') setAutomationOperationId(null)
  }, [automationOperationId, jobs])

  type PipelinePick = (
    p: NonNullable<Awaited<ReturnType<typeof getConfig>>['pipeline']>,
  ) => string[]
  const automationChain: PipelinePick = (p) => [...buildAutomationSteps(p, scene)]

  return (
    <div className='flex items-center gap-0.5'>
      <Button
        variant='default'
        size='xs'
        onClick={() => void runStepForProject(automationChain)}
        data-testid='toolbar-automation'
        data-automation-running={isAutomationRunning ? 'true' : 'false'}
        disabled={!hasProject || isProcessing || !automationPlan.canRun}
      >
        {isAutomationRunning ? (
          <LoaderCircleIcon className='size-4 animate-spin' />
        ) : (
          <BotIcon className='size-4' />
        )}
        启动自动化工作流
      </Button>
      <AutomationPlanPopover plan={automationPlan} />
    </div>
  )

  async function runStepForProject(pick: PipelinePick) {
    const cfg = await getConfig()
    if (!cfg.pipeline) return
    const steps = pick(cfg.pipeline).filter((s): s is string => !!s)
    if (steps.length === 0) return
    const editor = useEditorUiStore.getState()
    const prefs = usePreferencesStore.getState()
    const response = await startPipeline({
      steps,
      targetLanguage: editor.selectedLanguage,
      systemPrompt: prefs.customSystemPrompt,
      defaultFont: prefs.defaultFont,
      readingOrder: editor.readingOrder === 'custom' ? undefined : editor.readingOrder,
    })
    setAutomationOperationId(response.operationId)
  }
}

function AutomationPlanPopover({ plan }: { plan: AutomationPlan }) {
  const missing = plan.missingEngines.length > 0
  const missingTranslations = plan.counts.missingTranslationBlocks > 0
  const blocked = missing || missingTranslations
  const empty = plan.counts.textBlocks === 0
  const triggerLabel = missingTranslations
    ? `缺${plan.counts.missingTranslationBlocks}译文`
    : missing
      ? '缺配置'
      : `${plan.counts.textBlocks}块`
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type='button'
          variant={blocked ? 'outline' : 'ghost'}
          size='xs'
          data-testid='toolbar-automation-plan'
          className={blocked ? 'border-amber-300 text-amber-700 dark:text-amber-300' : undefined}
          title='自动化计划'
        >
          {blocked ? (
            <TriangleAlertIcon className='size-3.5' />
          ) : (
            <ListChecksIcon className='size-3.5' />
          )}
          {triggerLabel}
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align='start'
        className='w-64 p-0 text-xs'
        data-testid='automation-plan-popover'
      >
        <div className='flex flex-col gap-2 p-3'>
          <div className='flex items-center justify-between gap-2'>
            <span className='font-semibold text-foreground'>自动化计划</span>
            <span className='rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground'>
              {plan.canRun ? '可执行' : '需配置'}
            </span>
          </div>
          <div className='grid grid-cols-2 gap-1.5'>
            <PlanStat label={`${plan.counts.textBlocks} 个文本块`} />
            <PlanStat label={`嵌字 ${plan.counts.letteringBlocks}`} />
            <PlanStat label={`修图 ${plan.counts.repairBlocks}`} />
            <PlanStat label={`双模式 ${plan.counts.dualModeBlocks}`} />
          </div>
          {blocked ? (
            <div className='flex flex-col gap-1.5'>
              {missing && (
                <div className='rounded-md border border-amber-300/70 bg-amber-50 px-2 py-1.5 text-amber-800 dark:bg-amber-950/30 dark:text-amber-200'>
                  缺少 {plan.missingEngines.join(' / ')}
                </div>
              )}
              {missingTranslations && (
                <div className='rounded-md border border-amber-300/70 bg-amber-50 px-2 py-1.5 text-amber-800 dark:bg-amber-950/30 dark:text-amber-200'>
                  缺少译文 {plan.counts.missingTranslationBlocks}
                </div>
              )}
            </div>
          ) : empty ? (
            <div className='rounded-md border border-border bg-muted/50 px-2 py-1.5 text-muted-foreground'>
              当前项目还没有可处理的文本块
            </div>
          ) : (
            <div className='rounded-md border border-border bg-muted/50 px-2 py-1.5 text-muted-foreground'>
              步骤 {plan.steps.join(' -> ')}
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function PlanStat({ label }: { label: string }) {
  return (
    <span className='rounded-md border border-border bg-muted/40 px-2 py-1 text-muted-foreground'>
      {label}
    </span>
  )
}

function ModelConfigPopover() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const { data: llmCatalog } = useGetCatalog()
  const { data: llmState } = useGetCurrentLlm()
  const { data: config } = useGetConfig()
  const { data: engineCatalog } = useGetEngineCatalog()
  const llmReady = llmState?.status === 'ready'
  const llmLoading = llmState?.status === 'loading'
  const [popoverOpen, setPopoverOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [engineBusyKey, setEngineBusyKey] = useState<PipelineEngineKey | null>(null)
  const llmModels: LlmModelOption[] = useMemo(() => flattenCatalogModels(llmCatalog), [llmCatalog])
  const selectedTarget = useEditorUiStore((s) => s.selectedTarget)
  const customSystemPrompt = usePreferencesStore((s) => s.customSystemPrompt)
  const setCustomSystemPrompt = usePreferencesStore((s) => s.setCustomSystemPrompt)
  const llmSelectedLanguage = useEditorUiStore((s) => s.selectedLanguage)
  const pipeline = (config?.pipeline ?? {}) as PipelineConfig

  const selectedModel = useMemo(
    () => llmModels.find(({ model }) => sameLlmTarget(model.target, selectedTarget)),
    [llmModels, selectedTarget],
  )
  const selectedTargetKey = selectedTarget ? llmTargetKey(selectedTarget) : undefined
  const selectedModelLanguages = selectedModel?.model.languages ?? []
  const selectedIsLoaded = llmReady && sameLlmTarget(llmState?.target, selectedTarget)

  const handleSetSelectedModel = (key: string) => {
    const next = llmModels.find(({ model }) => llmTargetKey(model.target) === key)
    if (!next) return
    const nextLanguages = next.model.languages
    const nextLanguage =
      llmSelectedLanguage && nextLanguages.includes(llmSelectedLanguage)
        ? llmSelectedLanguage
        : nextLanguages[0]
    useEditorUiStore.setState({ selectedTarget: next.model.target, selectedLanguage: nextLanguage })
  }

  const handleSetSelectedLanguage = (language: string) => {
    if (!selectedModelLanguages.includes(language)) return
    useEditorUiStore.setState({ selectedLanguage: language })
  }

  const handleToggleLoadUnload = async () => {
    const target = useEditorUiStore.getState().selectedTarget
    if (!target) return
    setBusy(true)
    try {
      if (selectedIsLoaded) {
        await deleteCurrentLlm()
      } else {
        await putCurrentLlm({ target })
      }
    } catch (e) {
      useEditorUiStore.getState().showError(String(e))
    } finally {
      setBusy(false)
    }
  }

  const handlePipelineChange = async (key: PipelineEngineKey, value: string) => {
    const patchKey = PIPELINE_PATCH_KEYS[key]
    const pipelinePatch = { [patchKey]: value } as NonNullable<ConfigPatch['pipeline']>
    setEngineBusyKey(key)
    try {
      const next = await patchConfig({ pipeline: pipelinePatch })
      queryClient.setQueryData(getGetConfigQueryKey(), next)
      await queryClient.invalidateQueries({ queryKey: getGetConfigQueryKey() })
    } catch (e) {
      useEditorUiStore.getState().showError(String(e))
    } finally {
      setEngineBusyKey(null)
    }
  }

  useEffect(() => {
    if (llmModels.length === 0) return
    const hasCurrent = llmModels.some(({ model }) => sameLlmTarget(model.target, selectedTarget))
    const nextModel = hasCurrent ? selectedModel?.model : llmModels[0]?.model
    if (!nextModel) return
    const nextLanguages = nextModel.languages
    const nextLanguage =
      llmSelectedLanguage && nextLanguages.includes(llmSelectedLanguage)
        ? llmSelectedLanguage
        : nextLanguages[0]
    const cur = useEditorUiStore.getState()
    if (
      sameLlmTarget(cur.selectedTarget, nextModel.target) &&
      cur.selectedLanguage === nextLanguage
    ) {
      return
    }
    useEditorUiStore.setState({
      selectedTarget: nextModel.target,
      selectedLanguage: nextLanguage,
    })
  }, [llmModels, llmSelectedLanguage, selectedModel?.model, selectedTarget])

  const indicatorBusy = busy || llmLoading

  return (
    <Popover open={popoverOpen} onOpenChange={setPopoverOpen}>
      <PopoverTrigger asChild>
        <button
          data-testid='model-config-trigger'
          data-llm-ready={llmReady ? 'true' : 'false'}
          data-llm-loading={indicatorBusy ? 'true' : 'false'}
          className={`flex h-6 cursor-pointer items-center gap-1.5 rounded-full px-2.5 text-[11px] font-medium shadow-sm transition hover:opacity-80 ${
            llmReady
              ? 'bg-rose-400 text-white ring-1 ring-rose-400/30'
              : indicatorBusy
                ? 'bg-amber-400 text-white ring-1 ring-amber-400/30'
                : 'bg-muted text-muted-foreground ring-1 ring-border/50'
          }`}
        >
          <motion.span
            className={`size-1.5 rounded-full ${
              llmReady ? 'bg-white' : indicatorBusy ? 'bg-white' : 'bg-muted-foreground/40'
            }`}
            animate={
              llmReady
                ? { opacity: [1, 0.5, 1] }
                : indicatorBusy
                  ? { opacity: [1, 0.4, 1] }
                  : { opacity: 1 }
            }
            transition={
              llmReady || indicatorBusy
                ? { duration: indicatorBusy ? 1 : 2, repeat: Infinity, ease: 'easeInOut' }
                : {}
            }
          />
          <span data-testid='llm-trigger' className='contents'>
            模型
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        align='end'
        className='max-h-[80vh] w-[360px] overflow-y-auto p-0'
        data-testid='model-config-popover'
      >
        <div data-testid='llm-popover' className='flex flex-col'>
          <section className='flex flex-col gap-1.5 px-3 pt-3 pb-2.5'>
            <ModelSectionTitle icon={LanguagesIcon} title='翻译模型' />
            <div className='flex items-center gap-1.5'>
              <LlmModelSelect
                data-testid='llm-model-select'
                value={selectedTargetKey}
                options={llmModels}
                getKey={({ model }) => llmTargetKey(model.target)}
                placeholder={t('llm.selectPlaceholder')}
                onChange={handleSetSelectedModel}
                triggerClassName='min-w-0 flex-1'
              />
              <Button
                data-testid='llm-load-toggle'
                data-llm-ready={selectedIsLoaded ? 'true' : 'false'}
                data-llm-loading={indicatorBusy ? 'true' : 'false'}
                variant={selectedIsLoaded ? 'ghost' : 'default'}
                size='sm'
                onClick={() => void handleToggleLoadUnload()}
                disabled={!selectedTarget || indicatorBusy}
                className='h-6 shrink-0 gap-1 px-2 text-[11px]'
              >
                {indicatorBusy ? <LoaderCircleIcon className='size-3 animate-spin' /> : null}
                {selectedIsLoaded ? t('llm.unload') : t('llm.load')}
              </Button>
            </div>
            <PipelineEngineSelect
              label='翻译引擎'
              value={pipeline.translator}
              engines={engineCatalog?.translators ?? []}
              busy={engineBusyKey === 'translator'}
              onChange={(value) => void handlePipelineChange('translator', value)}
            />
            {selectedModelLanguages.length > 0 ? (
              <Select
                value={llmSelectedLanguage ?? selectedModelLanguages[0]}
                onValueChange={handleSetSelectedLanguage}
              >
                <SelectTrigger data-testid='llm-language-select' className='w-full'>
                  <SelectValue placeholder={t('llm.languagePlaceholder')} />
                </SelectTrigger>
                <SelectContent position='popper'>
                  {selectedModelLanguages.map((language, index) => (
                    <SelectItem
                      key={language}
                      value={language}
                      data-testid={`llm-language-option-${index}`}
                    >
                      {t(`llm.languages.${language}`, { defaultValue: language })}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : null}
            <Textarea
              data-testid='llm-system-prompt'
              value={customSystemPrompt ?? ''}
              onChange={(e) => setCustomSystemPrompt(e.target.value || undefined)}
              placeholder={t('llm.systemPromptPlaceholder')}
              rows={5}
              className='min-h-0 resize-y px-2 py-1.5 text-xs leading-snug md:text-xs'
            />
          </section>
          <Separator />
          <section className='flex flex-col gap-1.5 px-3 py-2.5'>
            <ModelSectionTitle icon={EyeIcon} title='视觉模型' />
            <PipelineEngineSelect
              label='检测'
              value={pipeline.detector}
              engines={engineCatalog?.detectors ?? []}
              busy={engineBusyKey === 'detector'}
              onChange={(value) => void handlePipelineChange('detector', value)}
            />
            <PipelineEngineSelect
              label='字体识别'
              value={pipeline.font_detector}
              engines={engineCatalog?.fontDetectors ?? []}
              busy={engineBusyKey === 'font_detector'}
              onChange={(value) => void handlePipelineChange('font_detector', value)}
            />
            <PipelineEngineSelect
              label='OCR'
              value={pipeline.ocr}
              engines={engineCatalog?.ocr ?? []}
              busy={engineBusyKey === 'ocr'}
              onChange={(value) => void handlePipelineChange('ocr', value)}
            />
          </section>
          <Separator />
          <section className='flex flex-col gap-1.5 px-3 py-2.5'>
            <ModelSectionTitle icon={PaintbrushIcon} title='修图模型' />
            <PipelineEngineSelect
              label='涂白'
              value={pipeline.inpainter}
              engines={engineCatalog?.inpainters ?? []}
              busy={engineBusyKey === 'inpainter'}
              onChange={(value) => void handlePipelineChange('inpainter', value)}
            />
            <PipelineEngineSelect
              label='修图生成'
              value={pipeline.repairer}
              engines={engineCatalog?.repairers ?? []}
              busy={engineBusyKey === 'repairer'}
              onChange={(value) => void handlePipelineChange('repairer', value)}
            />
            <PipelineEngineSelect
              label='嵌字渲染'
              value={pipeline.renderer}
              engines={engineCatalog?.renderers ?? []}
              busy={engineBusyKey === 'renderer'}
              onChange={(value) => void handlePipelineChange('renderer', value)}
            />
          </section>
        </div>
      </PopoverContent>
    </Popover>
  )
}

function ModelSectionTitle({
  icon: Icon,
  title,
}: {
  icon: ComponentType<{ className?: string }>
  title: string
}) {
  return (
    <div className='flex items-center gap-1.5 text-[10px] font-semibold text-muted-foreground uppercase'>
      <Icon className='size-3.5' />
      <span>{title}</span>
    </div>
  )
}

function PipelineEngineSelect({
  label,
  value,
  engines,
  busy,
  onChange,
}: {
  label: string
  value?: string | null
  engines: EngineCatalogEntry[]
  busy?: boolean
  onChange: (value: string) => void
}) {
  const current = value?.trim() || ''
  const hasCurrent = current && engines.some((engine) => engine.id === current)
  const options =
    current && !hasCurrent ? [{ id: current, name: current, produces: [] }, ...engines] : engines

  if (options.length === 0) {
    return (
      <div className='flex items-center justify-between gap-2 rounded-md border border-border bg-muted/30 px-2 py-1.5'>
        <span className='shrink-0 text-[10px] text-muted-foreground'>{label}</span>
        <span className='truncate text-[11px] text-muted-foreground'>无可用引擎</span>
      </div>
    )
  }

  return (
    <div className='grid grid-cols-[4.25rem_minmax(0,1fr)] items-center gap-2'>
      <span className='text-[10px] text-muted-foreground'>{label}</span>
      <Select value={current || options[0]?.id} onValueChange={onChange} disabled={busy}>
        <SelectTrigger className='h-7 w-full min-w-0 text-[11px]'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent position='popper'>
          {options.map((engine) => (
            <SelectItem key={engine.id} value={engine.id} className='text-xs'>
              {engine.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
