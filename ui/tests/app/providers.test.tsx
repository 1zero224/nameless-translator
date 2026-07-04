import { waitFor } from '@testing-library/react'
import { http, HttpResponse } from 'msw'
import { beforeEach, describe, expect, it } from 'vitest'

import { OperationsSnapshotSync } from '@/app/providers'
import { queryClient } from '@/lib/queryClient'
import { useJobsStore } from '@/lib/stores/jobsStore'

import { renderWithQuery } from '../helpers'
import { server } from '../msw/server'

describe('OperationsSnapshotSync', () => {
  beforeEach(() => {
    queryClient.clear()
    useJobsStore.getState().clear()
  })

  it('hydrates a running pipeline with latest progress from /operations', async () => {
    server.use(
      http.get('/api/v1/operations', () =>
        HttpResponse.json({
          operations: [
            {
              id: 'op-running',
              kind: 'pipeline',
              status: 'running',
              progress: {
                jobId: 'op-running',
                status: { status: 'running' },
                step: 'ocr',
                currentPage: 2,
                totalPages: 5,
                currentStepIndex: 1,
                totalSteps: 4,
                overallPercent: 37,
              },
            },
          ],
        }),
      ),
    )

    renderWithQuery(<OperationsSnapshotSync />)

    await waitFor(() => {
      const job = useJobsStore.getState().jobs['op-running']
      expect(job).toMatchObject({
        id: 'op-running',
        kind: 'pipeline',
        status: 'running',
      })
      expect(job.progress?.step).toBe('ocr')
      expect(job.progress?.overallPercent).toBe(37)
    })
  })
})
