import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { MenuBar } from '@/components/MenuBar'
import { getGetConfigQueryKey, getGetSceneJsonQueryKey } from '@/lib/api/default/default'
import { queryClient } from '@/lib/queryClient'
import { useJobsStore } from '@/lib/stores/jobsStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

const pageId = 'p1'
const textNodeId = 't1'

vi.mock('@/lib/io/openFiles', () => ({
  openImageFiles: vi.fn().mockResolvedValue([]),
  openImageFolder: vi.fn().mockResolvedValue([]),
  openKhrFile: vi.fn().mockResolvedValue(null),
}))

beforeEach(() => {
  // Default: config + scene exist so the menu enables scene-dependent items.
  useJobsStore.getState().clear()
  usePreferencesStore.getState().resetPreferences()
  useSelectionStore.getState().setPage(null)
  server.use(
    http.get('/api/v1/scene.json', () =>
      HttpResponse.json({
        epoch: 0,
        scene: { pages: {}, project: { name: 'P' } as never },
      }),
    ),
    http.get('/api/v1/config', () => HttpResponse.json({})),
  )
  queryClient.setQueryData(getGetSceneJsonQueryKey(), {
    epoch: 0,
    scene: { pages: {}, project: { name: 'P' } },
  })
  queryClient.setQueryData(getGetConfigQueryKey(), {})
})

function seedSceneWithTextNode() {
  queryClient.setQueryData(getGetSceneJsonQueryKey(), {
    epoch: 0,
    scene: {
      project: { name: 'P' },
      pages: {
        [pageId]: {
          id: pageId,
          name: 'p1',
          width: 100,
          height: 100,
          nodes: {
            [textNodeId]: {
              id: textNodeId,
              visible: true,
              transform: { x: 10, y: 12, width: 40, height: 20, rotationDeg: 5 },
              kind: {
                text: {
                  confidence: 1,
                  text: '原文テキスト',
                  translation: '中文译文',
                  workflow: { modes: ['repair'], resultMode: 'repair' },
                },
              },
            },
          },
        },
      },
    },
  })
  useSelectionStore.getState().setPage(pageId)
}

describe('MenuBar', () => {
  it('renders File / View / Process / Help triggers', async () => {
    renderWithQuery(<MenuBar />)
    expect(screen.getByTestId('menu-file-trigger')).toBeInTheDocument()
    expect(screen.getByTestId('menu-process-trigger')).toBeInTheDocument()
  })

  it('runs standard processing without font detection outside project automation', async () => {
    const pipelineRequests: unknown[] = []
    server.use(
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            detector: 'text-detector',
            segmenter: 'text-segmenter',
            bubble_segmenter: 'bubble-segmenter',
            font_detector: 'font-detector',
            ocr: 'ocr',
            translator: 'translator',
            inpainter: 'inpainter',
            renderer: 'renderer',
          },
        }),
      ),
      http.post('/api/v1/pipelines', async ({ request }) => {
        pipelineRequests.push(await request.json())
        return HttpResponse.json({ operationId: 'op-process' })
      }),
    )

    renderWithQuery(<MenuBar />)

    await userEvent.click(screen.getByTestId('menu-process-trigger'))
    await userEvent.click(await screen.findByTestId('menu-process-all'))

    await waitFor(() => expect(pipelineRequests).toHaveLength(1))
    expect(pipelineRequests[0]).toMatchObject({
      steps: [
        'text-detector',
        'text-segmenter',
        'bubble-segmenter',
        'ocr',
        'translator',
        'inpainter',
        'renderer',
      ],
    })
  })

  it('runs custom detect without font detection outside project automation', async () => {
    const pipelineRequests: unknown[] = []
    usePreferencesStore.getState().setCustomPipeline({
      detect: true,
      ocr: false,
      translator: false,
      inpainter: false,
      renderer: false,
    })
    server.use(
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            detector: 'text-detector',
            segmenter: 'text-segmenter',
            bubble_segmenter: 'bubble-segmenter',
            font_detector: 'font-detector',
          },
        }),
      ),
      http.post('/api/v1/pipelines', async ({ request }) => {
        pipelineRequests.push(await request.json())
        return HttpResponse.json({ operationId: 'op-custom-detect' })
      }),
    )

    renderWithQuery(<MenuBar />)

    await userEvent.click(screen.getByTestId('menu-process-trigger'))
    await userEvent.click(await screen.findByText('menu.runCustomAll'))

    await waitFor(() => expect(pipelineRequests).toHaveLength(1))
    expect(pipelineRequests[0]).toMatchObject({
      steps: ['text-detector', 'text-segmenter', 'bubble-segmenter'],
    })
  })

  it('disables menu pipeline actions while a pipeline is running', async () => {
    useSelectionStore.getState().setPage('p1')
    usePreferencesStore.getState().setCustomPipeline({
      detect: true,
      ocr: false,
      translator: false,
      inpainter: false,
      renderer: false,
    })
    useJobsStore.getState().started('running-pipeline', 'pipeline')

    renderWithQuery(<MenuBar />)

    await userEvent.click(screen.getByTestId('menu-process-trigger'))

    expect(await screen.findByTestId('menu-process-current')).toHaveAttribute('data-disabled')
    expect(await screen.findByTestId('menu-process-rerender')).toHaveAttribute('data-disabled')
    expect(await screen.findByTestId('menu-process-all')).toHaveAttribute('data-disabled')
    expect(await screen.findByTestId('menu-process-custom-current')).toHaveAttribute(
      'data-disabled',
    )
    expect(await screen.findByTestId('menu-process-custom-all')).toHaveAttribute('data-disabled')
  })

  it('disables GPT Image menu action without a selected text block', async () => {
    seedSceneWithTextNode()

    renderWithQuery(<MenuBar />)

    await userEvent.click(screen.getByTestId('menu-process-trigger'))

    expect(await screen.findByTestId('menu-process-gpt-image')).toHaveAttribute('data-disabled')
  })

  it('opens GPT Image dialog with a prompt from the selected text block and posts textNodeId', async () => {
    const aiRequests: unknown[] = []
    seedSceneWithTextNode()
    useSelectionStore.getState().select(textNodeId)
    server.use(
      http.get('/api/v1/ai/codex/auth/status', () =>
        HttpResponse.json({ signedIn: true, accountId: 'acct-test' }),
      ),
      http.post('/api/v1/ai/codex/images', async ({ request }) => {
        aiRequests.push(await request.json())
        return HttpResponse.json({ operationId: 'op-ai' })
      }),
    )

    renderWithQuery(<MenuBar />)

    await userEvent.click(screen.getByTestId('menu-process-trigger'))
    await userEvent.click(await screen.findByTestId('menu-process-gpt-image'))

    const prompt = await screen.findByTestId('gpt-image-prompt')
    expect((prompt as HTMLTextAreaElement).value).toContain('原文テキスト')
    expect((prompt as HTMLTextAreaElement).value).toContain('中文译文')

    await userEvent.click(screen.getByTestId('gpt-image-submit'))

    await waitFor(() => expect(aiRequests).toHaveLength(1))
    expect(aiRequests[0]).toMatchObject({
      pageId,
      textNodeId,
      prompt: expect.stringContaining('原文テキスト'),
    })
  })

  it('Close Project calls DELETE /projects/current and invalidates scene', async () => {
    let deleted = 0
    server.use(
      http.delete('/api/v1/projects/current', () => {
        deleted += 1
        return new HttpResponse(null, { status: 204 })
      }),
    )
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')

    renderWithQuery(<MenuBar />)
    await userEvent.click(screen.getByTestId('menu-file-trigger'))
    const close = await screen.findByTestId('menu-file-close-project')
    await userEvent.click(close)

    await waitFor(() => expect(deleted).toBe(1))
    await waitFor(() => {
      const invalidatedKeys = invalidateSpy.mock.calls.map((c) => c[0]?.queryKey)
      expect(invalidatedKeys).toContainEqual(getGetSceneJsonQueryKey())
    })
  })

  it('Close Project is disabled when no project is open', async () => {
    // Clear seeded cache + point /scene.json at the 400 response so useScene
    // resolves to null.
    queryClient.clear()
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json({ message: 'no project' }, { status: 400 }),
      ),
    )
    renderWithQuery(<MenuBar />)
    await waitFor(() => expect(queryClient.isFetching()).toBe(0))
    await userEvent.click(screen.getByTestId('menu-file-trigger'))
    const close = await screen.findByTestId('menu-file-close-project')
    expect(close).toHaveAttribute('data-disabled')
  })
})
