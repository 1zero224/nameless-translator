import type { Node, Transform } from '@/lib/api/schemas'
import { DEFAULT_MANUAL_WORKFLOW } from '@/lib/workflow'

export type RectangleBlockDraft = {
  kind: 'rectangle'
  x: number
  y: number
  width: number
  height: number
}

export type PolygonBlockDraft = {
  kind: 'polygon'
  points: number[][]
}

export type ManualBlockDraft = RectangleBlockDraft | PolygonBlockDraft

export function createManualTextNode(id: string, draft: ManualBlockDraft): Node {
  const transform = draftToTransform(draft)
  return {
    id,
    transform,
    visible: true,
    kind: {
      text: {
        lockLayoutBox: true,
        workflow: {
          ...DEFAULT_MANUAL_WORKFLOW,
          modes: [...DEFAULT_MANUAL_WORKFLOW.modes],
          selection: {
            shapes: [draftToSelectionShape(draft, transform)],
          },
        },
      },
    },
  } as unknown as Node
}

function draftToTransform(draft: ManualBlockDraft): Transform {
  if (draft.kind === 'rectangle') {
    return {
      x: draft.x,
      y: draft.y,
      width: draft.width,
      height: draft.height,
      rotationDeg: 0,
    }
  }

  if (draft.points.length < 3) {
    throw new Error('polygon text block needs at least three points')
  }

  const xs = draft.points.map((point) => point[0] ?? 0)
  const ys = draft.points.map((point) => point[1] ?? 0)
  const minX = Math.min(...xs)
  const minY = Math.min(...ys)
  const maxX = Math.max(...xs)
  const maxY = Math.max(...ys)
  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY,
    rotationDeg: 0,
  }
}

function draftToSelectionShape(draft: ManualBlockDraft, transform: Transform) {
  if (draft.kind === 'rectangle') {
    return { kind: 'rectangle' as const, transform }
  }
  return { kind: 'polygon' as const, points: draft.points.map(([x, y]) => [x, y]) }
}
