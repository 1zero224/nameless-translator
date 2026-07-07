import { fireEvent, screen, waitFor } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it } from 'vitest'

import { SettingsDialog } from '@/components/SettingsDialog'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

describe('SettingsDialog AI model settings', () => {
  beforeEach(() => {
    server.use(
      http.get('/api/v1/catalog', () => HttpResponse.json({ localModels: [], providers: [] })),
      http.get('/api/v1/engines', () =>
        HttpResponse.json({
          detectors: [],
          fontDetectors: [],
          segmenters: [],
          bubbleSegmenters: [],
          ocr: [],
          translators: [],
          inpainters: [],
          repairers: [],
          renderers: [],
        }),
      ),
      http.get('/api/v1/meta', () => HttpResponse.json({ version: 'test' })),
      http.get('/api/v1/ai/codex/auth/status', () => HttpResponse.json({ signedIn: false })),
    )
  })

  it('shows AI model defaults and saves model changes', async () => {
    const patches: unknown[] = []
    server.use(
      http.get('/api/v1/config', () =>
        HttpResponse.json({
          ai_models: {
            gpt_image: 'gpt-image-2',
            mimo_text: 'mimo-v2.5-pro',
            mimo_vision: 'mimo-v2.5',
          },
        }),
      ),
      http.patch('/api/v1/config', async ({ request }) => {
        patches.push(await request.json())
        return HttpResponse.json({
          ai_models: {
            gpt_image: 'gpt-image-2-test',
            mimo_text: 'mimo-v2.5-pro-test',
            mimo_vision: 'mimo-v2.5-test',
          },
        })
      }),
    )

    renderWithQuery(<SettingsDialog open onOpenChange={() => {}} defaultTab='ai' />)

    const gptImage = await screen.findByTestId('settings-gpt-image-model')
    const mimoText = await screen.findByTestId('settings-mimo-text-model')
    const mimoVision = await screen.findByTestId('settings-mimo-vision-model')

    expect(gptImage).toHaveValue('gpt-image-2')
    expect(mimoText).toHaveValue('mimo-v2.5-pro')
    expect(mimoVision).toHaveValue('mimo-v2.5')

    fireEvent.change(gptImage, { target: { value: 'gpt-image-2-test' } })
    fireEvent.change(mimoText, { target: { value: 'mimo-v2.5-pro-test' } })
    fireEvent.change(mimoVision, { target: { value: 'mimo-v2.5-test' } })
    fireEvent.click(screen.getByTestId('settings-ai-models-save'))

    await waitFor(() => expect(patches).toHaveLength(1))
    expect(patches[0]).toMatchObject({
      aiModels: {
        gptImage: 'gpt-image-2-test',
        mimoText: 'mimo-v2.5-pro-test',
        mimoVision: 'mimo-v2.5-test',
      },
    })
  })
})
