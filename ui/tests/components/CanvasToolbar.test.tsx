import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it } from 'vitest'

import { CanvasToolbar } from '@/components/canvas/CanvasToolbar'
import { queryClient } from '@/lib/queryClient'
import { useJobsStore } from '@/lib/stores/jobsStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

function sceneWithWorkflowModes(modes: string[]) {
  return {
    epoch: 1,
    scene: {
      project: { name: 'Automation' },
      pages: {
        p1: {
          id: 'p1',
          name: 'P1',
          width: 100,
          height: 100,
          nodes: Object.fromEntries(
            modes.map((mode, index) => [
              `t${index + 1}`,
              {
                id: `t${index + 1}`,
                transform: { x: 0, y: index * 12, width: 10, height: 10, rotationDeg: 0 },
                visible: true,
                kind: {
                  text: {
                    text: `text ${index + 1}`,
                    workflow: {
                      modes: mode.split('+'),
                      resultMode: mode.includes('repair') ? 'repair' : 'lettering',
                    },
                  },
                },
              },
            ]),
          ),
        },
      },
    },
  }
}

function sceneWithTranslations(translations: Array<string | null | undefined>) {
  return {
    epoch: 1,
    scene: {
      project: { name: 'Automation' },
      pages: {
        p1: {
          id: 'p1',
          name: 'P1',
          width: 100,
          height: 100,
          nodes: Object.fromEntries(
            translations.map((translation, index) => [
              `t${index + 1}`,
              {
                id: `t${index + 1}`,
                transform: { x: 0, y: index * 12, width: 10, height: 10, rotationDeg: 0 },
                visible: true,
                kind: {
                  text: {
                    text: `text ${index + 1}`,
                    translation,
                    workflow: {
                      modes: ['lettering'],
                      resultMode: 'lettering',
                    },
                  },
                },
              },
            ]),
          ),
        },
      },
    },
  }
}

describe('CanvasToolbar automation plan', () => {
  beforeEach(() => {
    queryClient.clear()
    useJobsStore.getState().clear()
  })

  it('disables project automation and explains missing engines', async () => {
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithWorkflowModes(['lettering', 'repair', 'lettering+repair'])),
      ),
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            font_detector: 'font-detector',
            inpainter: 'lama-manga',
            renderer: '',
          },
        }),
      ),
    )

    renderWithQuery(<CanvasToolbar />)

    const automation = await screen.findByTestId('toolbar-automation')
    await waitFor(() => expect(automation).toBeDisabled())

    await userEvent.click(await screen.findByTestId('toolbar-automation-plan'))

    expect(screen.getByText('3 个文本块')).toBeInTheDocument()
    expect(screen.getByText('嵌字 2')).toBeInTheDocument()
    expect(screen.getByText('修图 2')).toBeInTheDocument()
    expect(screen.getByText('双模式 1')).toBeInTheDocument()
    expect(screen.getByText('缺少 repairer / renderer')).toBeInTheDocument()
  })

  it('disables project automation when workflow text blocks are missing translations', async () => {
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithTranslations(['译文', '', undefined])),
      ),
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            font_detector: 'font-detector',
            inpainter: 'lama-manga',
            renderer: 'koharu-renderer',
          },
        }),
      ),
    )

    renderWithQuery(<CanvasToolbar />)

    const automation = await screen.findByTestId('toolbar-automation')
    await waitFor(() => expect(automation).toBeDisabled())

    await userEvent.click(await screen.findByTestId('toolbar-automation-plan'))

    expect(screen.getByText('缺少译文 2')).toBeInTheDocument()
  })
})
