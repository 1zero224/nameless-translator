'use client'

import {
  AlertTriangleIcon,
  CheckCircle2Icon,
  LoaderCircleIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  TagsIcon,
} from 'lucide-react'
import type React from 'react'
import { useMemo, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { useScene } from '@/hooks/useScene'
import { getConfig, startPipeline } from '@/lib/api/default/default'
import type {
  FontReviewPriority,
  FontStyleGroup,
  FontStyleProfile,
  FontStyleRole,
  ProjectStyle,
} from '@/lib/api/schemas'
import { applyOp } from '@/lib/io/scene'
import { ops } from '@/lib/ops'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useJobsStore } from '@/lib/stores/jobsStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'
import { cn } from '@/lib/utils'

const ROLE_OPTIONS: Array<{ value: FontStyleRole; label: string }> = [
  { value: 'bubble_body', label: '气泡正文' },
  { value: 'bubble_emphasis', label: '气泡强调' },
  { value: 'outside_bubble', label: '气泡外文字' },
  { value: 'caption', label: '旁白' },
  { value: 'sfx', label: '音效字' },
  { value: 'unknown', label: '未知' },
]

const PRIORITY_LABEL: Record<FontReviewPriority, string> = {
  none: '无',
  medium: '建议审查',
  high: '必须审查',
}

export function FontReviewPanel() {
  const { scene } = useScene()
  const pageId = useSelectionStore((s) => s.pageId)
  const jobs = useJobsStore((s) => s.jobs)
  const isProcessing = Object.values(jobs).some((job) => job.status === 'running')
  const [busy, setBusy] = useState<'build' | 'apply' | 'save' | null>(null)

  const style = scene?.project.style
  const profile = style?.fontProfile ?? null
  const groups = profile?.styleGroups ?? []
  const queue = (style?.fontReviewQueue ?? []).filter((item) => item.status !== 'resolved')
  const buckets = Object.keys(style?.fontPolicy?.buckets ?? {})
  const counts = useMemo(() => profileCounts(scene, profile), [scene, profile])

  if (!scene) {
    return (
      <div className='flex flex-1 items-center justify-center text-xs text-muted-foreground'>
        当前没有打开项目
      </div>
    )
  }

  const disabled = isProcessing || busy !== null

  return (
    <div className='flex min-h-0 flex-1 flex-col text-xs' data-testid='font-review-panel'>
      <div className='flex items-center justify-between border-b border-border px-2 py-1.5'>
        <div className='flex items-center gap-1.5 font-semibold tracking-wide text-muted-foreground uppercase'>
          <TagsIcon className='size-3.5' />
          <span>字体审查</span>
        </div>
        <div className='flex items-center gap-1'>
          <Button
            size='xs'
            variant='outline'
            disabled={disabled}
            data-testid='font-profile-build'
            onClick={() => void runFontWorkflow('build')}
          >
            {busy === 'build' ? (
              <LoaderCircleIcon className='size-3.5 animate-spin' />
            ) : (
              <RefreshCwIcon className='size-3.5' />
            )}
            建立/更新
          </Button>
          <Button
            size='xs'
            disabled={disabled || !profile}
            data-testid='font-profile-apply'
            onClick={() => void runFontWorkflow('apply')}
          >
            {busy === 'apply' ? (
              <LoaderCircleIcon className='size-3.5 animate-spin' />
            ) : (
              <CheckCircle2Icon className='size-3.5' />
            )}
            应用档案
          </Button>
        </div>
      </div>

      <div className='min-h-0 flex-1 space-y-2 overflow-y-auto p-2'>
        <ProfileSummary profile={profile} counts={counts} queueCount={queue.length} />

        <section className='space-y-1.5'>
          <SectionHeader title='Style groups' value={`${groups.length}组`} />
          {groups.length === 0 ? (
            <EmptyState text='还没有字体档案。首次自动化会自动建立，也可以点击建立/更新。' />
          ) : (
            groups.map((group) => (
              <StyleGroupCard
                key={group.id}
                group={group}
                buckets={buckets}
                affectedCount={counts.byGroup.get(group.id) ?? 0}
                disabled={disabled}
                onUpdate={(patch) => void updateGroup(group.id, patch)}
                onReapply={(scope) => void reapply(scope)}
              />
            ))
          )}
        </section>

        <section className='space-y-1.5'>
          <SectionHeader title='异常队列' value={`${queue.length}项`} />
          {queue.length === 0 ? (
            <EmptyState text='当前没有待审查的字体异常。' />
          ) : (
            <div className='space-y-1'>
              {queue.slice(0, 12).map((item) => (
                <div
                  key={item.id}
                  className='rounded-md border border-border bg-card/80 px-2 py-1.5'
                  data-testid='font-review-item'
                >
                  <div className='flex items-center justify-between gap-2'>
                    <span className='truncate font-medium'>{item.blockId}</span>
                    <span
                      className={cn(
                        'shrink-0 rounded-full px-1.5 py-0.5 text-[9px] font-semibold',
                        item.reviewPriority === 'high'
                          ? 'bg-destructive/10 text-destructive'
                          : 'bg-amber-500/10 text-amber-700 dark:text-amber-300',
                      )}
                    >
                      {PRIORITY_LABEL[item.reviewPriority]}
                    </span>
                  </div>
                  <div className='mt-1 text-[10px] text-muted-foreground'>
                    {item.styleGroupId} · v{item.profileVersion}
                  </div>
                  {(item.riskReasons ?? []).length > 0 && (
                    <div className='mt-1 line-clamp-2 text-[10px] text-muted-foreground'>
                      {(item.riskReasons ?? []).join(' / ')}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      </div>
    </div>
  )

  async function runFontWorkflow(mode: 'build' | 'apply') {
    setBusy(mode)
    try {
      if (mode === 'build') {
        const baseStyle = style ? { ...style } : {}
        await saveProjectStyle({ ...baseStyle, fontProfile: null, fontReviewQueue: [] })
      }
      await startFontPipeline(undefined)
    } finally {
      setBusy(null)
    }
  }

  async function updateGroup(groupId: string, patch: Partial<FontStyleGroup>) {
    if (!profile || !scene) return
    setBusy('save')
    try {
      const nextVersion = profile.version + 1
      const affected = collectAffectedBlocks(scene, groupId)
      const nextProfile: FontStyleProfile = {
        ...profile,
        version: nextVersion,
        status: 'auto_active',
        reviewState: 'unreviewed',
        styleGroups: groups.map((group) =>
          group.id === groupId ? { ...group, ...patch, needsReview: true } : group,
        ),
        previousVersions: [...(profile.previousVersions ?? []), `v${profile.version}`],
        changeLog: [
          ...(profile.changeLog ?? []),
          {
            version: nextVersion,
            change: `Updated ${groupId} font policy`,
            affectedBlocks: affected,
          },
        ],
      }
      const baseStyle = style ? { ...style } : {}
      await saveProjectStyle({ ...baseStyle, fontProfile: nextProfile })
    } finally {
      setBusy(null)
    }
  }

  async function reapply(scope: 'current' | 'all') {
    setBusy('apply')
    try {
      await startFontPipeline(scope === 'current' && pageId ? [pageId] : undefined)
    } finally {
      setBusy(null)
    }
  }

  async function saveProjectStyle(nextStyle: ProjectStyle) {
    await applyOp(ops.updateProjectMeta({ style: nextStyle }))
  }
}

function ProfileSummary({
  profile,
  counts,
  queueCount,
}: {
  profile: FontStyleProfile | null
  counts: ReturnType<typeof profileCounts>
  queueCount: number
}) {
  return (
    <section className='rounded-md border border-border bg-card/90 p-2'>
      <div className='flex items-center justify-between gap-2'>
        <div>
          <div className='text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'>
            Profile
          </div>
          <div className='mt-0.5 font-medium' data-testid='font-profile-status'>
            {profile ? `${profile.status} / ${profile.reviewState}` : '未建立'}
          </div>
        </div>
        <div className='text-right'>
          <div className='text-[10px] text-muted-foreground'>confidence</div>
          <div className='font-mono text-sm'>
            {profile ? `${Math.round(profile.profileConfidence * 100)}%` : '--'}
          </div>
        </div>
      </div>
      <div className='mt-2 grid grid-cols-4 gap-1'>
        <Stat label='已应用' value={counts.applied} />
        <Stat label='需审查' value={queueCount} />
        <Stat label='未知' value={counts.unknown} />
        <Stat label='版本' value={profile?.version ?? 0} />
      </div>
      {(profile?.profileRisks ?? []).length > 0 && (
        <div className='mt-2 flex items-start gap-1 rounded-md border border-amber-300/70 bg-amber-50 px-2 py-1.5 text-[10px] text-amber-800 dark:bg-amber-950/30 dark:text-amber-200'>
          <AlertTriangleIcon className='mt-0.5 size-3 shrink-0' />
          <span>{profile!.profileRisks!.map((risk) => risk.message || risk.type).join(' / ')}</span>
        </div>
      )}
    </section>
  )
}

function StyleGroupCard({
  group,
  buckets,
  affectedCount,
  disabled,
  onUpdate,
  onReapply,
}: {
  group: FontStyleGroup
  buckets: string[]
  affectedCount: number
  disabled: boolean
  onUpdate: (patch: Partial<FontStyleGroup>) => void
  onReapply: (scope: 'current' | 'all') => void
}) {
  return (
    <div className='rounded-md border border-border bg-card/90 p-2' data-testid='font-style-group'>
      <div className='flex items-start justify-between gap-2'>
        <div className='min-w-0'>
          <div className='truncate font-medium'>{group.label}</div>
          <div className='mt-0.5 truncate text-[10px] text-muted-foreground'>{group.id}</div>
        </div>
        <span className='shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[9px] text-muted-foreground'>
          {affectedCount} blocks
        </span>
      </div>
      <div className='mt-2 grid grid-cols-[4.5rem_minmax(0,1fr)] items-center gap-1.5'>
        <LabelText>角色</LabelText>
        <Select
          value={group.role}
          disabled={disabled}
          onValueChange={(value) => onUpdate({ role: value as FontStyleRole })}
        >
          <SelectTrigger className='h-7 text-[11px]' data-testid={`font-role-${group.id}`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ROLE_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <LabelText>Bucket</LabelText>
        {buckets.length > 0 ? (
          <Select
            value={group.targetBucket}
            disabled={disabled}
            onValueChange={(value) => onUpdate({ targetBucket: value })}
          >
            <SelectTrigger className='h-7 text-[11px]' data-testid={`font-bucket-${group.id}`}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {buckets.map((bucket) => (
                <SelectItem key={bucket} value={bucket}>
                  {bucket}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : (
          <Input
            value={group.targetBucket}
            disabled={disabled}
            className='h-7 text-[11px]'
            onChange={(event) => onUpdate({ targetBucket: event.target.value || 'review' })}
          />
        )}
      </div>
      <div className='mt-2 flex items-center justify-between gap-2'>
        <Button
          type='button'
          size='xs'
          variant={group.preserveSourceStyle ? 'default' : 'outline'}
          disabled={disabled}
          onClick={() => onUpdate({ preserveSourceStyle: !group.preserveSourceStyle })}
        >
          保留源风格
        </Button>
        <div className='flex items-center gap-1'>
          <Button
            type='button'
            size='xs'
            variant='ghost'
            disabled={disabled}
            onClick={() => onReapply('current')}
          >
            <RotateCcwIcon className='size-3.5' />
            当前页
          </Button>
          <Button
            type='button'
            size='xs'
            variant='ghost'
            disabled={disabled}
            onClick={() => onReapply('all')}
          >
            <RotateCcwIcon className='size-3.5' />
            全部
          </Button>
        </div>
      </div>
      {(group.riskReasons ?? []).length > 0 && (
        <div className='mt-1.5 text-[10px] text-muted-foreground'>
          {(group.riskReasons ?? []).join(' / ')}
        </div>
      )}
    </div>
  )
}

function SectionHeader({ title, value }: { title: string; value: string }) {
  return (
    <div className='flex items-center justify-between text-[10px] font-semibold tracking-wide text-muted-foreground uppercase'>
      <span>{title}</span>
      <span>{value}</span>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className='rounded-md border border-border bg-muted/40 px-1.5 py-1 text-center'>
      <div className='font-mono text-sm text-foreground'>{value}</div>
      <div className='text-[9px] text-muted-foreground'>{label}</div>
    </div>
  )
}

function LabelText({ children }: { children: React.ReactNode }) {
  return <span className='text-[10px] text-muted-foreground'>{children}</span>
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className='rounded-md border border-dashed border-border p-2 text-xs text-muted-foreground'>
      {text}
    </div>
  )
}

async function startFontPipeline(pages?: string[]) {
  const cfg = await getConfig()
  const steps = [cfg.pipeline?.font_detector, cfg.pipeline?.renderer].filter(
    (step): step is string => !!step,
  )
  if (steps.length === 0) return
  const editor = useEditorUiStore.getState()
  const prefs = usePreferencesStore.getState()
  await startPipeline({
    steps,
    pages,
    targetLanguage: editor.selectedLanguage,
    systemPrompt: prefs.customSystemPrompt,
    defaultFont: prefs.defaultFont,
    readingOrder: editor.readingOrder === 'custom' ? undefined : editor.readingOrder,
  })
}

function profileCounts(scene: ReturnType<typeof useScene>['scene'], profile: FontStyleProfile | null) {
  const byGroup = new Map<string, number>()
  let applied = 0
  let unknown = 0
  if (!scene || !profile) return { applied, unknown, byGroup }
  for (const page of Object.values(scene.pages ?? {})) {
    for (const node of Object.values(page.nodes ?? {})) {
      if (!('text' in node.kind)) continue
      const trace = node.kind.text.workflow?.fontTrace
      if (trace?.profileId !== profile.id || trace.profileVersion !== profile.version) continue
      applied += 1
      const groupId = trace.styleGroupId ?? 'unknown/new_style'
      byGroup.set(groupId, (byGroup.get(groupId) ?? 0) + 1)
      if (groupId === 'unknown/new_style' || trace.textRole === 'unknown') unknown += 1
    }
  }
  return { applied, unknown, byGroup }
}

function collectAffectedBlocks(scene: NonNullable<ReturnType<typeof useScene>['scene']>, groupId: string) {
  const affected: string[] = []
  for (const page of Object.values(scene.pages ?? {})) {
    for (const [nodeId, node] of Object.entries(page.nodes ?? {})) {
      if (!('text' in node.kind)) continue
      if (node.kind.text.workflow?.fontTrace?.styleGroupId === groupId) affected.push(nodeId)
    }
  }
  return affected
}
