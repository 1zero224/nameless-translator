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

function sceneWithTranslatedWorkflowModes(modes: string[]) {
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
                    translation: `translation ${index + 1}`,
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
    expect(await screen.findByTestId('toolbar-automation-plan')).toHaveTextContent('缺2译文')

    await userEvent.click(await screen.findByTestId('toolbar-automation-plan'))

    expect(screen.getByText('缺少译文 2')).toBeInTheDocument()
  })

  it('does not show automation as running when another pipeline step is running', async () => {
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithTranslatedWorkflowModes(['lettering'])),
      ),
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            font_detector: 'mimo-font-selection',
            inpainter: 'lama-manga',
            repairer: 'gpt-image-2-repair',
            renderer: 'koharu-renderer',
          },
        }),
      ),
    )
    useJobsStore.getState().progress({
      jobId: 'detect-job',
      status: 'running',
      step: 'detect',
      currentPage: 0,
      totalPages: 1,
      currentStepIndex: 0,
      totalSteps: 1,
      overallPercent: 0.5,
    })

    renderWithQuery(<CanvasToolbar />)

    expect(await screen.findByTestId('toolbar-automation')).toHaveAttribute(
      'data-automation-running',
      'false',
    )
    expect(screen.getByTestId('toolbar-detect')).toHaveAttribute('data-step-running', 'true')
  })

  it('starts project automation with mode-derived steps across the whole project', async () => {
    const pipelineRequests: unknown[] = []
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(
          sceneWithTranslatedWorkflowModes(['lettering', 'repair', 'lettering+repair']),
        ),
      ),
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          pipeline: {
            font_detector: 'mimo-font-selection',
            inpainter: 'lama-manga',
            repairer: 'gpt-image-2-repair',
            renderer: 'koharu-renderer',
          },
        }),
      ),
      http.post('/api/v1/pipelines', async ({ request }) => {
        pipelineRequests.push(await request.json())
        return HttpResponse.json({ operationId: 'op-automation' })
      }),
    )

    renderWithQuery(<CanvasToolbar />)

    const automation = await screen.findByTestId('toolbar-automation')
    await waitFor(() => expect(automation).toBeEnabled())

    await userEvent.click(automation)

    await waitFor(() => expect(pipelineRequests).toHaveLength(1))
    expect(pipelineRequests[0]).toMatchObject({
      steps: ['mimo-font-selection', 'lama-manga', 'gpt-image-2-repair', 'koharu-renderer'],
    })
    expect(pipelineRequests[0]).not.toHaveProperty('pages')
  })

  it('groups model configuration into translation, vision, and repair sections', async () => {
    server.use(
      http.get('/api/v1/scene.json', () =>
        HttpResponse.json(sceneWithTranslatedWorkflowModes(['lettering'])),
      ),
    )

    renderWithQuery(<CanvasToolbar />)

    await userEvent.click(await screen.findByTestId('model-config-trigger'))

    expect(screen.getByText('翻译模型')).toBeInTheDocument()
    expect(screen.getByText('视觉模型')).toBeInTheDocument()
    expect(screen.getByText('修图模型')).toBeInTheDocument()
  })
})
