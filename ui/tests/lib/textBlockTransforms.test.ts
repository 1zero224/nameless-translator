import { describe, expect, it } from 'vitest'

import { rotationFromPointer, resolveRotationDrag } from '@/lib/textBlockTransforms'

describe('text block transform helpers', () => {
  it('measures rotation with the top handle position as zero degrees', () => {
    const center = { x: 50, y: 50 }

    expect(rotationFromPointer(center, { x: 50, y: 20 })).toBe(0)
    expect(rotationFromPointer(center, { x: 80, y: 50 })).toBe(90)
    expect(rotationFromPointer(center, { x: 50, y: 80 })).toBe(180)
    expect(rotationFromPointer(center, { x: 20, y: 50 })).toBe(-90)
  })

  it('applies pointer angle deltas to the starting rotation', () => {
    const center = { x: 50, y: 50 }

    expect(
      resolveRotationDrag({
        center,
        startPoint: { x: 50, y: 20 },
        currentPoint: { x: 80, y: 50 },
        startRotationDeg: 15,
      }),
    ).toBe(105)
  })

  it('normalizes rotation into a compact signed range', () => {
    const center = { x: 50, y: 50 }

    expect(
      resolveRotationDrag({
        center,
        startPoint: { x: 50, y: 20 },
        currentPoint: { x: 20, y: 50 },
        startRotationDeg: -120,
      }),
    ).toBe(150)
  })
})
