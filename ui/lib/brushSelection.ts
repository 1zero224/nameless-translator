import type { Region } from '@/lib/api/schemas'
import type { BrushBlockDraft } from '@/lib/textBlocks'

export function mergeBrushSelectionRegion(current: Region | null, next: Region): Region {
  if (!current) return next
  const x0 = Math.min(current.x, next.x)
  const y0 = Math.min(current.y, next.y)
  const x1 = Math.max(current.x + current.width, next.x + next.width)
  const y1 = Math.max(current.y + current.height, next.y + next.height)
  return {
    x: x0,
    y: y0,
    width: x1 - x0,
    height: y1 - y0,
  }
}

export function brushSelectionRegionToDraft(mask: string, region: Region): BrushBlockDraft {
  return {
    kind: 'brush',
    mask,
    x: Math.round(region.x),
    y: Math.round(region.y),
    width: Math.round(region.width),
    height: Math.round(region.height),
  }
}
