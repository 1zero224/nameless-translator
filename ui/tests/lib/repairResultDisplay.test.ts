import { describe, expect, it } from 'vitest'

import type { Node, Page } from '@/lib/api/schemas'
import { resolveRepairResultDisplay } from '@/lib/repairResultDisplay'

function customLayer(id: string): Node {
  return {
    id,
    transform: { x: 0, y: 0, width: 100, height: 100, rotationDeg: 0 },
    visible: true,
    kind: {
      image: {
        role: 'custom',
        blob: `blob-${id}`,
        opacity: 0.75,
        naturalWidth: 100,
        naturalHeight: 100,
        name: `${id} repair`,
      },
    },
  } as unknown as Node
}

function textNode(
  id: string,
  resultMode: 'lettering' | 'repair',
  repairLayer: string | null,
): Node {
  return {
    id,
    transform: { x: 10, y: 10, width: 40, height: 20, rotationDeg: 0 },
    visible: true,
    kind: {
      text: {
        text: '原文',
        translation: 'translation',
        sprite: `sprite-${id}`,
        workflow: {
          modes: ['lettering', 'repair'],
          resultMode,
          repairLayer,
        },
      },
    },
  } as unknown as Node
}

function pageWithNodes(nodes: Record<string, Node>): Page {
  return {
    id: 'p1',
    name: 'P1',
    width: 100,
    height: 100,
    nodes,
  } as unknown as Page
}

describe('resolveRepairResultDisplay', () => {
  it('selects bound custom layers and hides sprites for repair-result text blocks', () => {
    const page = pageWithNodes({
      r1: customLayer('r1'),
      t1: textNode('t1', 'repair', 'r1'),
    })

    const display = resolveRepairResultDisplay(page)

    expect(display.repairTextNodeIds).toEqual(new Set(['t1']))
    expect(display.repairLayers).toEqual([
      { id: 'r1', blob: 'blob-r1', opacity: 0.75 },
    ])
  })

  it('skips bound repair layers when the text block result mode is lettering', () => {
    const page = pageWithNodes({
      r1: customLayer('r1'),
      t1: textNode('t1', 'lettering', 'r1'),
    })

    const display = resolveRepairResultDisplay(page)

    expect(display.repairTextNodeIds).toEqual(new Set())
    expect(display.repairLayers).toEqual([])
  })

  it('ignores dangling or non-custom repair layer bindings', () => {
    const sourceImage = {
      ...customLayer('source'),
      kind: {
        image: {
          role: 'source',
          blob: 'source-blob',
          opacity: 1,
          naturalWidth: 100,
          naturalHeight: 100,
        },
      },
    } as unknown as Node
    const page = pageWithNodes({
      source: sourceImage,
      t1: textNode('t1', 'repair', 'missing'),
      t2: textNode('t2', 'repair', 'source'),
    })

    const display = resolveRepairResultDisplay(page)

    expect(display.repairTextNodeIds).toEqual(new Set(['t1', 't2']))
    expect(display.repairLayers).toEqual([])
  })
})
