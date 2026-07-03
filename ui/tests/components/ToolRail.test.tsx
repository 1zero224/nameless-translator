import { fireEvent, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'

import { ToolRail } from '@/components/canvas/ToolRail'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'

import { renderWithQuery } from '../helpers'

describe('ToolRail', () => {
  beforeEach(() => {
    useEditorUiStore.setState({
      mode: 'select',
      showNavigator: true,
      showTextBlocksOverlay: false,
    })
  })

  it('lets the user switch to polygon lasso block selection', () => {
    renderWithQuery(<ToolRail />)

    const lasso = screen.getByTestId('tool-lasso')
    fireEvent.click(lasso)

    expect(useEditorUiStore.getState().mode).toBe('lasso')
    expect(useEditorUiStore.getState().showTextBlocksOverlay).toBe(true)
    expect(lasso).toHaveAttribute('data-active', 'true')
  })
})
