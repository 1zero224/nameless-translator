import { screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { Workspace } from '@/components/canvas/Workspace'
import { getGetSceneJsonQueryKey } from '@/lib/api/default/default'
import type { SceneSnapshot } from '@/lib/api/schemas'
import { queryClient } from '@/lib/queryClient'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'

let getContextSpy: ReturnType<typeof vi.spyOn> | null = null

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
})
