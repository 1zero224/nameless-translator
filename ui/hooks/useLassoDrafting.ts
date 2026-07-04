'use client'

import { useCallback, useEffect, useRef, useState } from 'react'
import type React from 'react'

import type { DocumentPointer, PointerToDocumentFn } from '@/hooks/usePointerToDocument'
import type { Page } from '@/lib/api/schemas'
import type { PolygonBlockDraft } from '@/lib/textBlocks'
import type { ToolMode } from '@/lib/types'

export type LassoDraft = {
  points: number[][]
}

type LassoDraftingOptions = {
  mode: ToolMode
  page: Page | null
  pointerToDocument: PointerToDocumentFn
  clearSelection: () => void
  onCreateBlock: (draft: PolygonBlockDraft) => void
}

const MIN_POINTS = 3
const MIN_BOUNDS = 4

export function useLassoDrafting({
  mode,
  page,
  pointerToDocument,
  clearSelection,
  onCreateBlock,
}: LassoDraftingOptions) {
  const pointsRef = useRef<number[][]>([])
  const [draft, setDraft] = useState<LassoDraft | null>(null)

  const reset = useCallback(() => {
    pointsRef.current = []
    setDraft(null)
  }, [])

  const addPoint = useCallback(
    (point: DocumentPointer) => {
      const next = [...pointsRef.current, [Math.round(point.x), Math.round(point.y)]]
      if (next.length === 1) clearSelection()
      pointsRef.current = next
      setDraft({ points: next })
    },
    [clearSelection],
  )

  const finalize = useCallback(() => {
    if (mode !== 'lasso' || !page) {
      reset()
      return
    }
    const points = pointsRef.current
    reset()
    if (!isValidPolygon(points)) return
    onCreateBlock({ kind: 'polygon', points })
  }, [mode, onCreateBlock, page, reset])

  useEffect(() => {
    if (mode !== 'lasso') reset()
  }, [mode, reset])

  useEffect(() => {
    if (mode !== 'lasso') return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        reset()
      } else if (event.key === 'Enter') {
        event.preventDefault()
        finalize()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [finalize, mode, reset])

  const bind = () => ({
    onPointerDown: (event: React.PointerEvent<Element>) => {
      if (!page || mode !== 'lasso' || event.button !== 0 || event.detail > 1) return
      const point = pointerToDocument(event)
      if (!point) return
      event.preventDefault()
      event.stopPropagation()
      addPoint(point)
    },
    onDoubleClick: (event: React.MouseEvent<Element>) => {
      if (!page || mode !== 'lasso') return
      event.preventDefault()
      event.stopPropagation()
      finalize()
    },
  })

  return { draftLasso: draft, bind, resetLasso: reset }
}

function isValidPolygon(points: number[][]): boolean {
  if (points.length < MIN_POINTS) return false
  const xs = points.map((point) => point[0] ?? 0)
  const ys = points.map((point) => point[1] ?? 0)
  const width = Math.max(...xs) - Math.min(...xs)
  const height = Math.max(...ys) - Math.min(...ys)
  return width >= MIN_BOUNDS && height >= MIN_BOUNDS
}
