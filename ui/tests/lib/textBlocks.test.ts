import { describe, expect, it } from 'vitest'

import { createManualTextNode } from '@/lib/textBlocks'

describe('manual text block node creation', () => {
  it('stores a rectangle selection when creating a rectangular block', () => {
    const node = createManualTextNode('node-1', {
      kind: 'rectangle',
      x: 12,
      y: 20,
      width: 80,
      height: 40,
    })

    expect(node.transform).toEqual({ x: 12, y: 20, width: 80, height: 40, rotationDeg: 0 })
    expect(node.kind).toMatchObject({
      text: {
        lockLayoutBox: true,
        workflow: {
          modes: ['repair'],
          resultMode: 'repair',
          selection: {
            shapes: [
              {
                kind: 'rectangle',
                transform: { x: 12, y: 20, width: 80, height: 40, rotationDeg: 0 },
              },
            ],
          },
        },
      },
    })
  })

  it('stores polygon points and uses their outer bounding box as the text transform', () => {
    const node = createManualTextNode('node-2', {
      kind: 'polygon',
      points: [
        [48, 16],
        [90, 30],
        [76, 84],
        [20, 60],
      ],
    })

    expect(node.transform).toEqual({ x: 20, y: 16, width: 70, height: 68, rotationDeg: 0 })
    expect(node.kind).toMatchObject({
      text: {
        lockLayoutBox: true,
        workflow: {
          modes: ['repair'],
          resultMode: 'repair',
          selection: {
            shapes: [
              {
                kind: 'polygon',
                points: [
                  [48, 16],
                  [90, 30],
                  [76, 84],
                  [20, 60],
                ],
              },
            ],
          },
        },
      },
    })
  })

  it('stores brush mask selections and uses the stroke bounds as the text transform', () => {
    const node = createManualTextNode('node-3', {
      kind: 'brush',
      mask: 'brush-mask-hash',
      x: 14,
      y: 22,
      width: 64,
      height: 48,
    })

    expect(node.transform).toEqual({ x: 14, y: 22, width: 64, height: 48, rotationDeg: 0 })
    expect(node.kind).toMatchObject({
      text: {
        lockLayoutBox: true,
        workflow: {
          modes: ['repair'],
          resultMode: 'repair',
          selection: {
            shapes: [
              {
                kind: 'brush',
                mask: 'brush-mask-hash',
              },
            ],
          },
        },
      },
    })
  })
})
