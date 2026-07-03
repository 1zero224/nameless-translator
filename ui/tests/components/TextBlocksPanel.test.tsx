import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { TextBlocksPanel } from '@/components/panels/TextBlocksPanel'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useJobsStore } from '@/lib/stores/jobsStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

const openImageLayerFileMock = vi.hoisted(() => vi.fn<[], Promise<File | null>>())

vi.mock('@/lib/io/openFiles', () => ({
  openImageLayerFile: openImageLayerFileMock,
}))

function sceneWithTextNodes({
  repairSecond = false,
  fontTraceSecond = false,
}: { repairSecond?: boolean; fontTraceSecond?: boolean } = {}) {
  return {
    epoch: 1,
    scene: {
      pages: {
        p1: {
          id: 'p1',
          name: 'P1',
          width: 100,
          height: 100,
          nodes: {
            t1: {
              id: 't1',
              transform: { x: 0, y: 0, width: 10, height: 10, rotationDeg: 0 },
              visible: true,
              kind: { text: { text: 'first' } },
            },
            t2: {
              id: 't2',
              transform: { x: 10, y: 10, width: 10, height: 10, rotationDeg: 0 },
              visible: true,
              kind: {
                text: fontTraceSecond
                  ? fontTraceWorkflowText('second')
                  : repairSecond
                    ? repairWorkflowText('second')
                    : { text: 'second' },
              },
            },
          },
        },
      },
      project: { name: 'Proj' },
    },
  }
}

function repairWorkflowText(text: string) {
  return {
    text,
    workflow: {
      modes: ['lettering', 'repair'],
      resultMode: 'lettering',
    },
  }
}

function fontTraceWorkflowText(text: string) {
  return {
    text,
    workflow: {
      modes: ['lettering'],
      resultMode: 'lettering',
      fontTrace: {
        primaryCategory: 'sans_serif',
        secondaryCategory: 'gothic',
        candidateFonts: ['Koharu Gothic', 'Koharu Rounded'],
        selectedFont: 'Koharu Gothic',
        notes: ['mimo category validated', 'candidate comparison score 0.92'],
      },
    },
  }
}

describe('TextBlocksPanel', () => {
  beforeEach(() => {
    openImageLayerFileMock.mockReset()
    useSelectionStore.getState().setPage('p1')
    useSelectionStore.getState().select('t2', false)
    useJobsStore.getState().clear()
    useEditorUiStore.setState({ selectedLanguage: 'en' })
    usePreferencesStore.setState({
      customSystemPrompt: 'translate naturally',
      defaultFont: 'Arial',
    })
  })

  it('generates translation only for the clicked text block', async () => {
    const pipelineRequests: unknown[] = []
    server.use(
      http.get('/api/v1/scene.json', () => HttpResponse.json(sceneWithTextNodes())),
      http.get('/api/v1/config', () =>
        HttpResponse.json({ pipeline: { translator: 'llm', renderer: 'koharu-renderer' } }),
      ),
      http.get('/api/v1/llm/current', () =>
        HttpResponse.json({ status: 'ready', target: null, error: null }),
      ),
      http.post('/api/v1/pipelines', async ({ request }) => {
        pipelineRequests.push(await request.json())
        return HttpResponse.json({ operationId: 'op-1' })
      }),
    )

    renderWithQuery(<TextBlocksPanel />)

    const generateButton = await screen.findByTestId('textblock-generate-1')
    await waitFor(() => expect(generateButton).not.toBeDisabled())
    await userEvent.click(generateButton)

    await waitFor(() => expect(pipelineRequests).toHaveLength(1))
    expect(pipelineRequests[0]).toMatchObject({
      steps: ['llm', 'koharu-renderer'],
      pages: ['p1'],
      textNodeIds: ['t2'],
      targetLanguage: 'en',
      systemPrompt: 'translate naturally',
      defaultFont: 'Arial',
    })
  })

  it('patches workflow when toggling repair mode', async () => {
    const historyOps: unknown[] = []
    server.use(
      http.get('/api/v1/scene.json', () => HttpResponse.json(sceneWithTextNodes())),
      http.post('/api/v1/history/apply', async ({ request }) => {
        historyOps.push(await request.json())
        return HttpResponse.json({ epoch: 2 })
      }),
    )

    renderWithQuery(<TextBlocksPanel />)

    await userEvent.click(await screen.findByRole('button', { name: '修图模式' }))

    await waitFor(() => expect(historyOps).toHaveLength(1))
    expect(historyOps[0]).toMatchObject({
      updateNode: {
        page: 'p1',
        id: 't2',
        patch: {
          data: {
            text: {
              workflow: {
                modes: ['lettering', 'repair'],
                resultMode: 'lettering',
              },
            },
          },
        },
      },
    })
  })

  it('uploads a repair image layer bound to the selected repair text block', async () => {
    const uploaded: { repairText: string | null; hasFile: boolean }[] = []
    openImageLayerFileMock.mockResolvedValue(
      new File([new Uint8Array([1, 2, 3])], 'repair.png', { type: 'image/png' }),
    )
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithTextNodes({ repairSecond: true })),
      ),
      http.post('/api/v1/pages/:id/image-layers', async ({ request }) => {
        const url = new URL(request.url)
        const form = await request.formData()
        const file = form.get('file')
        uploaded.push({
          repairText: url.searchParams.get('repairText'),
          hasFile: file !== null,
        })
        return HttpResponse.json({ node: 'repair-layer-1' })
      }),
    )

    renderWithQuery(<TextBlocksPanel />)

    await userEvent.click(await screen.findByRole('button', { name: '绑定修图图层' }))

    await waitFor(() => expect(uploaded).toEqual([{ repairText: 't2', hasFile: true }]))
  })

  it('shows font workflow classification, candidates, final pick, and notes', async () => {
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithTextNodes({ fontTraceSecond: true })),
      ),
    )

    renderWithQuery(<TextBlocksPanel />)

    expect(await screen.findByText(/无衬线/)).toBeInTheDocument()
    expect(screen.getByText(/黑体/)).toBeInTheDocument()
    expect(screen.getAllByText(/Koharu Gothic/).length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText(/候选: Koharu Gothic \/ Koharu Rounded/)).toBeInTheDocument()
    expect(screen.getByText('mimo category validated')).toBeInTheDocument()
    expect(screen.getByText('candidate comparison score 0.92')).toBeInTheDocument()
  })
})
