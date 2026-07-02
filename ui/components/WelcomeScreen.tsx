'use client'

import {
  AlertCircleIcon,
  ArrowDownAZIcon,
  BookOpenIcon,
  ClockIcon,
  FileArchiveIcon,
  ImagesIcon,
  MoreVerticalIcon,
  PlusIcon,
  TrashIcon,
  TypeIcon,
  XIcon,
} from 'lucide-react'
import Image from 'next/image'
import type React from 'react'
import { useCallback, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { ScrollArea } from '@/components/ui/scroll-area'
import { useDeleteProject, useListProjects } from '@/lib/api/default/default'
import type { ProjectSummary } from '@/lib/api/schemas'
import { importKhrFile } from '@/lib/io/pagesIo'
import { createAndOpenProject, switchProject } from '@/lib/io/scene'

type Busy = false | 'new' | 'open' | 'import'
type SortMode = 'updated' | 'name'

/**
 * Managed project bookshelf. The backend owns project paths; the client only
 * opens by id. Covers are project-scoped URLs because the bookshelf renders
 * before a project is opened as the active session.
 */
export function WelcomeScreen() {
  const { t } = useTranslation()
  const { data: projectsData, refetch: refetchProjects } = useListProjects()
  const [sortMode, setSortMode] = useState<SortMode>('updated')
  const projects = useMemo(() => {
    const all = projectsData?.projects ?? []
    return [...all].sort((a, b) =>
      sortMode === 'name'
        ? a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' })
        : (b.updatedAtMs ?? 0) - (a.updatedAtMs ?? 0),
    )
  }, [projectsData, sortMode])

  const [busy, setBusy] = useState<Busy | 'delete'>(false)
  const [error, setError] = useState<string | null>(null)
  const [newDialogOpen, setNewDialogOpen] = useState(false)
  const [projectToDelete, setProjectToDelete] = useState<ProjectSummary | null>(null)

  const deleteProjectMutation = useDeleteProject()

  const openById = useCallback(async (id: string) => {
    setError(null)
    setBusy('open')
    try {
      await switchProject({ id })
    } catch (e) {
      setError(`Open failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      setBusy(false)
    }
  }, [])

  const onDeleteConfirm = useCallback(async () => {
    if (!projectToDelete) return
    setError(null)
    setBusy('delete')
    try {
      await deleteProjectMutation.mutateAsync({ id: projectToDelete.id })
      await refetchProjects()
      setProjectToDelete(null)
    } catch (e) {
      setError(`Delete failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      setBusy(false)
    }
  }, [projectToDelete, deleteProjectMutation, refetchProjects])

  const onCreate = useCallback(async (name: string) => {
    setError(null)
    setBusy('new')
    try {
      await createAndOpenProject({ name })
    } catch (e) {
      setError(`New failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      setBusy(false)
      setNewDialogOpen(false)
    }
  }, [])

  const importKhr = useCallback(async () => {
    setError(null)
    setBusy('import')
    try {
      await importKhrFile()
      await refetchProjects()
    } catch (e) {
      setError(`Import failed: ${e instanceof Error ? e.message : String(e)}`)
    } finally {
      setBusy(false)
    }
  }, [refetchProjects])

  return (
    <div className='flex min-h-0 flex-1 overflow-hidden bg-background'>
      <div className='mx-auto flex w-full max-w-6xl flex-col gap-5 px-5 py-5'>
        <header className='flex flex-wrap items-center justify-between gap-3 border-b border-border pb-4'>
          <div className='flex min-w-0 items-center gap-3'>
            <Image src='/icon.png' alt='Koharu' width={40} height={40} priority />
            <div className='flex min-w-0 flex-col gap-0.5'>
              <h1 className='truncate text-xl font-semibold tracking-tight text-foreground'>
                {t('welcome.title')}
              </h1>
              <p className='truncate text-xs text-muted-foreground'>{t('welcome.subtitle')}</p>
            </div>
          </div>
          <div className='flex items-center gap-2'>
            <Button
              size='sm'
              onClick={() => setNewDialogOpen(true)}
              disabled={!!busy}
              data-testid='welcome-new'
            >
              <PlusIcon className='size-3.5' />
              {t('welcome.new')}
            </Button>
            <Button
              size='sm'
              variant='outline'
              onClick={importKhr}
              disabled={!!busy}
              data-testid='welcome-import'
            >
              <FileArchiveIcon className='size-3.5' />
              {t('welcome.importKhr')}
            </Button>
          </div>
        </header>

        {error && (
          <div className='flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive'>
            <AlertCircleIcon className='mt-0.5 size-3.5 shrink-0' />
            <div className='flex-1'>{error}</div>
            <button
              type='button'
              onClick={() => setError(null)}
              className='cursor-pointer text-destructive/70 hover:text-destructive'
              aria-label='dismiss'
            >
              <XIcon className='size-3.5' />
            </button>
          </div>
        )}

        <section className='flex min-h-0 flex-1 flex-col gap-3'>
          <div className='flex flex-wrap items-center justify-between gap-2'>
            <div className='flex items-center gap-2'>
              <h2 className='text-[11px] font-semibold tracking-[0.14em] text-muted-foreground uppercase'>
                {t('welcome.projects')}
              </h2>
              {projects.length > 0 && (
                <span className='rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground tabular-nums'>
                  {projects.length}
                </span>
              )}
            </div>
            <div className='flex items-center gap-1 rounded-md border border-border bg-muted/40 p-1'>
              <SortButton
                active={sortMode === 'updated'}
                icon={ClockIcon}
                label='修改时间'
                onClick={() => setSortMode('updated')}
              />
              <SortButton
                active={sortMode === 'name'}
                icon={ArrowDownAZIcon}
                label='项目名称'
                onClick={() => setSortMode('name')}
              />
            </div>
          </div>

          {projects.length > 0 ? (
            <ScrollArea className='min-h-0 flex-1 rounded-lg border border-border bg-muted/20'>
              <div className='grid grid-cols-[repeat(auto-fill,minmax(152px,1fr))] gap-3 p-3'>
                {projects.map((p) => (
                  <ProjectCard
                    key={p.id}
                    project={p}
                    onOpen={openById}
                    onDeleteRequest={setProjectToDelete}
                    disabled={!!busy}
                  />
                ))}
              </div>
            </ScrollArea>
          ) : (
            <RecentSkeleton />
          )}
        </section>
      </div>

      <NewProjectDialog
        open={newDialogOpen}
        onOpenChange={setNewDialogOpen}
        onSubmit={onCreate}
        busy={busy === 'new'}
      />

      <AlertDialog
        open={!!projectToDelete}
        onOpenChange={(open) => !open && setProjectToDelete(null)}
      >
        <AlertDialogContent>
          <div className='flex flex-col gap-1.5 text-center sm:text-left'>
            <AlertDialogTitle>{t('welcome.deleteConfirmTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('welcome.deleteConfirmDescription', { name: projectToDelete?.name })}
            </AlertDialogDescription>
          </div>
          <div className='flex flex-col-reverse gap-2 sm:flex-row sm:justify-end'>
            <AlertDialogCancel disabled={busy === 'delete'}>{t('common.cancel')}</AlertDialogCancel>
            <AlertDialogAction
              onClick={(e) => {
                e.preventDefault()
                void onDeleteConfirm()
              }}
              disabled={busy === 'delete'}
              className='text-destructive-foreground bg-destructive hover:bg-destructive/90'
            >
              {busy === 'delete' ? t('welcome.deleting') : t('welcome.delete')}
            </AlertDialogAction>
          </div>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

function SortButton({
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
      className='flex h-7 cursor-pointer items-center gap-1 rounded px-2 text-[11px] font-medium text-muted-foreground transition-colors data-[active=true]:bg-background data-[active=true]:text-foreground data-[active=true]:shadow-xs hover:bg-background/70 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none'
    >
      <Icon className='size-3.5' />
      {label}
    </button>
  )
}

function RecentSkeleton() {
  const { t } = useTranslation()
  return (
    <div className='flex min-h-72 items-center justify-center rounded-lg border border-dashed border-border/70 bg-card/20'>
      <div className='flex max-w-xs flex-col items-center gap-3 text-center'>
        <div className='flex size-12 items-center justify-center rounded-md border border-border bg-background text-muted-foreground'>
          <BookOpenIcon className='size-5' />
        </div>
        <p className='text-xs text-muted-foreground'>{t('welcome.emptyHint')}</p>
      </div>
    </div>
  )
}

function ProjectCard({
  project,
  onOpen,
  onDeleteRequest,
  disabled,
}: {
  project: ProjectSummary
  onOpen: (id: string) => void
  onDeleteRequest: (project: ProjectSummary) => void
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const when = project.updatedAtMs && project.updatedAtMs > 0 ? new Date(project.updatedAtMs) : null
  return (
    <Card className='group relative gap-0 overflow-hidden rounded-lg border-border bg-card/90 transition-colors hover:border-primary/40'>
      <button
        type='button'
        onClick={() => onOpen(project.id)}
        disabled={disabled}
        className='flex min-w-0 cursor-pointer flex-col text-left outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60'
      >
        <ProjectCover project={project} />
        <div className='flex min-h-24 flex-col gap-2 p-3'>
          <div className='min-w-0'>
            <div className='line-clamp-2 min-h-9 text-sm leading-snug font-semibold text-foreground'>
              {project.name}
            </div>
            <div className='mt-0.5 truncate text-[10px] text-muted-foreground'>{project.id}</div>
          </div>
          <div className='mt-auto flex flex-wrap items-center gap-1 text-[10px] text-muted-foreground'>
            <span className='inline-flex items-center gap-1 rounded bg-muted px-1.5 py-0.5'>
              <ImagesIcon className='size-3' />
              {project.pageCount ?? 0}
            </span>
            <span className='inline-flex items-center gap-1 rounded bg-muted px-1.5 py-0.5'>
              <TypeIcon className='size-3' />
              {project.textBlockCount ?? 0}
            </span>
            {when && (
              <span className='inline-flex items-center gap-1 rounded bg-muted px-1.5 py-0.5'>
                <ClockIcon className='size-3' />
                {formatRelative(when)}
              </span>
            )}
          </div>
        </div>
      </button>

      <div className='absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100'>
        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant='ghost'
              size='icon-xs'
              className='h-7 w-7 bg-background/80 text-muted-foreground backdrop-blur hover:bg-background hover:text-foreground'
              disabled={disabled}
              aria-label={t('welcome.projectOptions')}
            >
              <MoreVerticalIcon className='size-3.5' />
            </Button>
          </PopoverTrigger>
          <PopoverContent
            align='end'
            className='w-32 rounded-md border border-border bg-popover p-1 shadow-lg'
          >
            <button
              type='button'
              onClick={() => onDeleteRequest(project)}
              className='flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs text-destructive transition-colors outline-none hover:bg-destructive/10 hover:text-destructive focus-visible:bg-destructive/10'
            >
              <TrashIcon className='size-3.5' />
              <span>{t('welcome.delete')}</span>
            </button>
          </PopoverContent>
        </Popover>
      </div>
    </Card>
  )
}

function ProjectCover({ project }: { project: ProjectSummary }) {
  const coverUrl = project.coverUrl ?? null
  return (
    <div className='relative aspect-[3/4] w-full overflow-hidden bg-muted'>
      {coverUrl ? (
        <img
          src={coverUrl}
          alt={project.name}
          className='size-full object-cover'
          draggable={false}
        />
      ) : (
        <div className='flex size-full items-center justify-center text-muted-foreground'>
          <BookOpenIcon className='size-8' />
        </div>
      )}
      <div className='pointer-events-none absolute inset-x-0 bottom-0 h-10 bg-gradient-to-t from-background/70 to-transparent' />
    </div>
  )
}

function formatRelative(d: Date): string {
  const diff = Date.now() - d.getTime()
  const m = 60_000
  const h = 3_600_000
  const day = 86_400_000
  if (diff < m) return 'just now'
  if (diff < h) return `${Math.floor(diff / m)}m ago`
  if (diff < day) return `${Math.floor(diff / h)}h ago`
  if (diff < day * 30) return `${Math.floor(diff / day)}d ago`
  return d.toLocaleDateString()
}

function NewProjectDialog({
  open,
  onOpenChange,
  onSubmit,
  busy,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: (name: string) => void
  busy: boolean
}) {
  const { t } = useTranslation()
  const [name, setName] = useState('')

  const trimmed = name.trim()
  const canSubmit = trimmed.length > 0 && !busy

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o)
        if (!o) setName('')
      }}
    >
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle>{t('welcome.newDialogTitle')}</DialogTitle>
          <DialogDescription>{t('welcome.newDialogDescription')}</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault()
            if (canSubmit) onSubmit(trimmed)
          }}
          className='flex flex-col gap-4'
        >
          <Input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('welcome.newDialogPlaceholder')}
          />
          <DialogFooter>
            <Button type='button' variant='outline' onClick={() => onOpenChange(false)}>
              {t('common.cancel')}
            </Button>
            <Button type='submit' disabled={!canSubmit}>
              <PlusIcon className='size-3.5' />
              {t('welcome.newDialogSubmit')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
