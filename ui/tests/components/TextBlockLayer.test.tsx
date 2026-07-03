import { screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import { TextBlockLayer } from '@/components/canvas/TextBlockLayer'
import { getGetSceneJsonQueryKey } from '@/lib/api/default/default'
import type { SceneSnapshot } from '@/lib/api/schemas'
import { queryClient } from '@/lib/queryClient'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'

function sceneWithRotatedTextBlock(): SceneSnapshot {
  return {
    epoch: 1,
    scene: {
      project: { name: 'P' } as never,
      pages: {
        p1: {
          id: 'p1',
          name: 'P1',
          width: 200,
          height: 200,
          nodes: {
            t1: {
              id: 't1',
              transform: { x: 20, y: 30, width: 80, height: 40, rotationDeg: 30 },
              visible: true,
              kind: { text: { text: 'hello' } },
            },
          },
        },
      },
    } as never,
  }
}

describe('TextBlockLayer', () => {
  beforeEach(() => {
    queryClient.setQueryData(getGetSceneJsonQueryKey(), sceneWithRotatedTextBlock())
    useSelectionStore.getState().setPage('p1')
    useSelectionStore.getState().select('t1')
    useEditorUiStore.setState({ mode: 'select' })
  })

  it('renders selected text blocks with rotation and a rotation handle', () => {
    renderWithQuery(<TextBlockLayer scale={1} />)

    expect(screen.getByTestId('text-block-t1')).toHaveStyle({
      transform: 'translate(20px, 30px) rotate(30deg)',
    })
    expect(screen.getByTestId('text-block-rotate-t1')).toBeInTheDocument()
  })
})
