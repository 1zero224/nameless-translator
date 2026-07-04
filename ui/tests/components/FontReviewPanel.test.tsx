import { screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it } from 'vitest'

import { FontReviewPanel } from '@/components/panels/FontReviewPanel'
import { useSelectionStore } from '@/lib/stores/selectionStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

function sceneWithFontProfile() {
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
              kind: {
                text: {
                  workflow: {
                    modes: ['lettering'],
                    resultMode: 'lettering',
                    fontTrace: {
                      profileId: 'font_profile_main',
                      profileVersion: 2,
                      styleGroupId: 'body_bubble_primary',
                      textRole: 'bubble_body',
                      recommendedFontBucket: 'body',
                    },
                  },
                },
              },
            },
          },
        },
      },
      project: {
        name: 'Proj',
        style: {
          defaultFont: 'BodyFont',
          fontProfile: {
            id: 'font_profile_main',
            version: 2,
            status: 'auto_active',
            reviewState: 'unreviewed',
            source: 'mimo_calibrated',
            profileConfidence: 0.82,
            styleGroups: [
              {
                id: 'body_bubble_primary',
                label: '气泡正文',
                role: 'bubble_body',
                preserveSourceStyle: false,
                targetBucket: 'body',
                confidence: 0.82,
                needsReview: true,
                riskReasons: ['style_group_unreviewed'],
              },
            ],
            profileRisks: [],
          },
          fontPolicy: {
            buckets: {
              body: { fonts: ['BodyFont'] },
              round: { fonts: ['RoundFont'] },
              review: { fonts: ['BodyFont'] },
            },
            fallbackFont: 'BodyFont',
          },
          fontReviewQueue: [
            {
              id: 'font-review-1',
              blockId: 't1',
              profileId: 'font_profile_main',
              profileVersion: 2,
              styleGroupId: 'body_bubble_primary',
              reviewPriority: 'medium',
              riskReasons: ['style_group_unreviewed'],
              suggestedAction: 'review_style_group',
              status: 'open',
            },
          ],
        },
      },
    },
  }
}

describe('FontReviewPanel', () => {
  beforeEach(() => {
    useSelectionStore.getState().setPage('p1')
    useSelectionStore.getState().clear()
  })

  it('shows project font profile summary and review queue', async () => {
    server.use(http.get('/api/v1/scene.json', () => HttpResponse.json(sceneWithFontProfile())))

    renderWithQuery(<FontReviewPanel />)

    expect(await screen.findByTestId('font-profile-status')).toHaveTextContent(
      'auto_active / unreviewed',
    )
    expect(screen.getAllByText('气泡正文').length).toBeGreaterThan(0)
    expect(screen.getByTestId('font-review-item')).toHaveTextContent('style_group_unreviewed')
  })

  it('patches project style when a style group bucket is changed', async () => {
    const historyOps: unknown[] = []
    server.use(
      http.get('/api/v1/scene.json', () => HttpResponse.json(sceneWithFontProfile())),
      http.post('/api/v1/history/apply', async ({ request }) => {
        historyOps.push(await request.json())
        return HttpResponse.json({ epoch: 2 })
      }),
    )

    renderWithQuery(<FontReviewPanel />)

    await userEvent.click(await screen.findByTestId('font-bucket-body_bubble_primary'))
    await userEvent.click(await screen.findByText('round'))

    await waitFor(() => expect(historyOps).toHaveLength(1))
    expect(historyOps[0]).toMatchObject({
      updateProjectMeta: {
        patch: {
          style: {
            fontProfile: {
              version: 3,
              styleGroups: [
                {
                  id: 'body_bubble_primary',
                  targetBucket: 'round',
                  needsReview: true,
                },
              ],
            },
          },
        },
      },
    })
  })
})
