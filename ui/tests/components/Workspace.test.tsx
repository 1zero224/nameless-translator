import { fireEvent, screen, waitFor } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { Workspace } from '@/components/canvas/Workspace'
import { getGetSceneJsonQueryKey } from '@/lib/api/default/default'
import type { SceneSnapshot } from '@/lib/api/schemas'
import { queryClient } from '@/lib/queryClient'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

let getContextSpy: ReturnType<typeof vi.spyOn> | null = null
const brushBlockDraftState = vi.hoisted(() => ({ hasDraft: false }))

vi.mock('@/hooks/useBlobData', async () => {
  const actual = await vi.importActual<typeof import('@/hooks/useBlobData')>('@/hooks/useBlobData')
  return {
    ...actual,
    useBlobData: (hash: string | undefined) => (hash ? new Uint8Array([hash.length]) : undefined),
  }
})

vi.mock('@/components/Image', () => ({
  Image: ({
    data,
    visible = true,
    ...props
  }: {
    data?: Uint8Array
    visible?: boolean
    [key: string]: unknown
  }) => (data && visible ? <div {...props} /> : null),
}))

vi.mock('@/hooks/useBrushBlockDrafting', () => ({
  useBrushBlockDrafting: () => ({
    canvasRef: { current: null },
    bind: () => ({}),
    visible: brushBlockDraftState.hasDraft,
    hasDraft: brushBlockDraftState.hasDraft,
    finalize: vi.fn(),
    reset: vi.fn(),
  }),
}))

function sceneWithDualModeBlock(resultMode: 'lettering' | 'repair'): SceneSnapshot {
  return {
    epoch: 1,
    scene: {
      project: { name: 'P' } as never,
      pages: {
        p1: {
          id: 'p1',
          name: 'P1',
          width: 100,
          height: 100,
          nodes: {
            src: {
              id: 'src',
              transform: { x: 0, y: 0, width: 100, height: 100, rotationDeg: 0 },
              visible: true,
              kind: {
                image: {
                  role: 'source',
                  blob: 'source-blob',
                  opacity: 1,
                  naturalWidth: 100,
                  naturalHeight: 100,
                },
              },
            },
            repair1: {
              id: 'repair1',
              transform: { x: 0, y: 0, width: 100, height: 100, rotationDeg: 0 },
              visible: true,
              kind: {
                image: {
                  role: 'custom',
                  blob: 'repair-blob',
                  opacity: 1,
                  naturalWidth: 100,
                  naturalHeight: 100,
                  name: 'gpt-image repair',
                },
              },
            },
            t1: {
              id: 't1',
              transform: { x: 10, y: 10, width: 40, height: 20, rotationDeg: 0 },
              visible: true,
              kind: {
                text: {
                  text: '原文',
                  translation: 'translation',
                  workflow: {
                    modes: ['lettering', 'repair'],
                    resultMode,
                    repairLayer: 'repair1',
                  },
                },
              },
            },
          },
        },
      },
    } as never,
  }
}

describe('Workspace repair result display', () => {
  beforeEach(() => {
    getContextSpy?.mockRestore()
    getContextSpy = vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      beginPath: vi.fn(),
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      fillRect: vi.fn(),
      lineTo: vi.fn(),
      moveTo: vi.fn(),
      restore: vi.fn(),
      save: vi.fn(),
      stroke: vi.fn(),
    } as never)
    useSelectionStore.getState().setPage('p1')
    useSelectionStore.getState().selectMany([])
    useEditorUiStore.setState({
      mode: 'select',
      showTextBlocksOverlay: true,
      showRenderedImage: false,
      showRepairResultLayers: true,
    })
    brushBlockDraftState.hasDraft = false
  })

  afterEach(() => {
    getContextSpy?.mockRestore()
    getContextSpy = null
  })

  it('renders a bound custom repair layer when the text block result mode is repair', async () => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('repair'))

    renderWithQuery(<Workspace />)

    expect(await screen.findByTestId('workspace-repair-layer-repair1')).toBeInTheDocument()
  })

  it('does not render the bound custom repair layer when the text block result mode is lettering', () => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('lettering'))

    renderWithQuery(<Workspace />)

    expect(screen.queryByTestId('workspace-repair-layer-repair1')).not.toBeInTheDocument()
  })

  it('does not render active repair result layers when the repair result layer toggle is off', () => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('repair'))
    useEditorUiStore.setState({ showRepairResultLayers: false })

    renderWithQuery(<Workspace />)

    expect(screen.queryByTestId('workspace-repair-layer-repair1')).not.toBeInTheDocument()
  })

  it('keeps brush block confirm actions outside of the scrollable image canvas', () => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('repair'))
    useEditorUiStore.setState({ mode: 'brushBlock' })
    brushBlockDraftState.hasDraft = true

    renderWithQuery(<Workspace />)

    const actions = screen.getByTestId('workspace-brush-block-actions')
    const canvas = screen.getByTestId('workspace-canvas')
    expect(canvas.contains(actions)).toBe(false)
    expect(actions).toHaveClass('right-4')
  })

  it('shows lasso draft points while clicking the canvas in lasso mode', async () => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('repair'))
    useEditorUiStore.setState({ mode: 'lasso' })

    renderWithQuery(<Workspace />)

    const canvas = screen.getByTestId('workspace-canvas')
    vi.spyOn(canvas, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 100,
      bottom: 100,
      width: 100,
      height: 100,
      toJSON: () => ({}),
    } as DOMRect)

    fireEvent.pointerDown(canvas, {
      button: 0,
      buttons: 1,
      clientX: 24,
      clientY: 32,
      pointerId: 1,
    })

    const draft = await screen.findByTestId('workspace-lasso-draft')
    expect(draft).toBeInTheDocument()
    expect(draft.querySelector('circle')).toHaveAttribute('cx', '24')
    expect(draft.querySelector('circle')).toHaveAttribute('cy', '32')
  })

  it('creates a polygon text block from lasso points on double click', async () => {
    let lastOp: any = null
    server.use(
      http.post('/api/v1/history/apply', async ({ request }) => {
        lastOp = await request.json()
        return HttpResponse.json({ epoch: 2 })
      }),
    )
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithDualModeBlock('repair'))
    useEditorUiStore.setState({ mode: 'lasso' })
    vi.spyOn(crypto, 'randomUUID').mockReturnValue('lasso-node-id')

    renderWithQuery(<Workspace />)

    const canvas = screen.getByTestId('workspace-canvas')
    vi.spyOn(canvas, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: 100,
      bottom: 100,
      width: 100,
      height: 100,
      toJSON: () => ({}),
    } as DOMRect)

    fireEvent.pointerDown(canvas, { button: 0, buttons: 1, clientX: 20, clientY: 20, pointerId: 1 })
    fireEvent.pointerDown(canvas, { button: 0, buttons: 1, clientX: 80, clientY: 25, pointerId: 1 })
    fireEvent.pointerDown(canvas, { button: 0, buttons: 1, clientX: 50, clientY: 70, pointerId: 1 })
    fireEvent.pointerDown(canvas, {
      button: 0,
      buttons: 1,
      clientX: 50,
      clientY: 70,
      detail: 2,
      pointerId: 1,
    })
    fireEvent.doubleClick(canvas, { button: 0, clientX: 50, clientY: 70, detail: 2 })

    await waitFor(() => expect(lastOp).not.toBeNull())
    expect(lastOp).toMatchObject({
      addNode: {
        page: 'p1',
        node: {
          id: 'lasso-node-id',
          transform: { x: 20, y: 20, width: 60, height: 50, rotationDeg: 0 },
          kind: {
            text: {
              workflow: {
                selection: {
                  shapes: [
                    {
                      kind: 'polygon',
                      points: [
                        [20, 20],
                        [80, 25],
                        [50, 70],
                      ],
                    },
                  ],
                },
              },
            },
          },
        },
      },
    })
  })
})
