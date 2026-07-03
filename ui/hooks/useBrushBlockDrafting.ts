'use client'

import { useCallback, useEffect, useRef, useState } from 'react'

import { useCanvasDrawing, type CanvasDims } from '@/hooks/useCanvasDrawing'
import type { PointerToDocumentFn } from '@/hooks/usePointerToDocument'
import type { Page, Region } from '@/lib/api/schemas'
import { brushSelectionRegionToDraft, mergeBrushSelectionRegion } from '@/lib/brushSelection'
import { uploadSelectionMask } from '@/lib/io/scene'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { usePreferencesStore } from '@/lib/stores/preferencesStore'
import type { BrushBlockDraft } from '@/lib/textBlocks'
import type { ToolMode } from '@/lib/types'

type BrushBlockDraftingOptions = {
  mode: ToolMode
  page: Page | null
  pointerToDocument: PointerToDocumentFn
  clearSelection: () => void
  onCreateBlock: (draft: BrushBlockDraft) => void | Promise<void>
}

export function useBrushBlockDrafting({
  mode,
  page,
  pointerToDocument,
  clearSelection,
  onCreateBlock,
}: BrushBlockDraftingOptions) {
  const fullMaskRef = useRef<Uint8Array | null>(null)
  const regionRef = useRef<Region | null>(null)
  const [hasDraft, setHasDraft] = useState(false)
  const isActive = mode === 'brushBlock'

  const dims: CanvasDims | null = page
    ? {
        width: page.width,
        height: page.height,
        key: page.id,
      }
    : null

  const canvasDrawing = useCanvasDrawing(dims, pointerToDocument, {
    getColor: () => 'rgba(244, 63, 94, 0.72)',
    blendMode: 'source-over',
    getBrushSize: () => usePreferencesStore.getState().brushConfig.size,
    enabled: isActive,
    clearAfterStroke: false,
    onFinalize: async () => {},
    onFinalizeFullCanvas: async (fullPng, region) => {
      fullMaskRef.current = fullPng
      regionRef.current = mergeBrushSelectionRegion(regionRef.current, region)
      setHasDraft(true)
      clearSelection()
    },
  })

  const reset = useCallback(() => {
    fullMaskRef.current = null
    regionRef.current = null
    setHasDraft(false)
    const canvas = canvasDrawing.canvasRef.current
    const ctx = canvas?.getContext('2d')
    if (canvas && ctx) ctx.clearRect(0, 0, canvas.width, canvas.height)
  }, [canvasDrawing.canvasRef])

  const finalize = useCallback(async () => {
    if (!isActive || !page) {
      reset()
      return
    }
    const mask = fullMaskRef.current
    const region = regionRef.current
    if (!mask || !region || region.width < 4 || region.height < 4) {
      reset()
      return
    }
    try {
      const maskHash = await uploadSelectionMask(mask)
      await onCreateBlock(brushSelectionRegionToDraft(maskHash, region))
      reset()
    } catch (e) {
      useEditorUiStore.getState().showError(String(e))
    }
  }, [isActive, onCreateBlock, page, reset])

  useEffect(() => {
    if (!isActive) reset()
  }, [isActive, reset])

  useEffect(() => {
    if (!isActive) return
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        reset()
      } else if (event.key === 'Enter') {
        event.preventDefault()
        void finalize()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [finalize, isActive, reset])

  return {
    canvasRef: canvasDrawing.canvasRef,
    bind: isActive ? canvasDrawing.bind : () => ({}),
    visible: isActive || hasDraft,
    hasDraft,
    finalize,
    reset,
  }
}
