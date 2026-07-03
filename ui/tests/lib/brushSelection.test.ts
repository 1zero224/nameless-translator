import { describe, expect, it } from 'vitest'

import { brushSelectionRegionToDraft, mergeBrushSelectionRegion } from '@/lib/brushSelection'

describe('brush selection helpers', () => {
  it('merges disconnected stroke regions into one text block region', () => {
    const first = { x: 40, y: 10, width: 20, height: 12 }
    const second = { x: 8, y: 50, width: 10, height: 9 }

    expect(mergeBrushSelectionRegion(null, first)).toEqual(first)
    expect(mergeBrushSelectionRegion(first, second)).toEqual({
      x: 8,
      y: 10,
      width: 52,
      height: 49,
    })
  })

  it('converts a stored mask and merged region into a brush text block draft', () => {
    expect(
      brushSelectionRegionToDraft('mask-hash', {
        x: 8,
        y: 10,
        width: 52,
        height: 49,
      }),
    ).toEqual({
      kind: 'brush',
      mask: 'mask-hash',
      x: 8,
      y: 10,
      width: 52,
      height: 49,
    })
  })
})
