import { screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import { LayersPanel } from '@/components/panels/LayersPanel'
import { getGetSceneJsonQueryKey } from '@/lib/api/default/default'
import type { SceneSnapshot } from '@/lib/api/schemas'
import { queryClient } from '@/lib/queryClient'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'

function sceneWithRepairResultLayer(): SceneSnapshot {
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
                    resultMode: 'repair',
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

describe('LayersPanel repair result layer', () => {
  beforeEach(() => {
    useSelectionStore.getState().setPage('p1')
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithRepairResultLayer())
  })

  it('lists the active repair result layer group when the current page has one', () => {
    renderWithQuery(<LayersPanel />)

    const layer = screen.getByTestId('layer-repairResult')
    expect(layer).toHaveAttribute('data-has-content', 'true')
    expect(layer).toHaveAttribute('data-visible', 'true')
  })
})
