import type { TextSelectionShape, TextWorkflow, Transform } from '@/lib/api/schemas'

export function translateWorkflowSelectionForTransform(
  workflow: TextWorkflow | null | undefined,
  previous: Transform,
  next: Transform,
): TextWorkflow | undefined {
  const shapes = workflow?.selection?.shapes
  if (!workflow || !shapes || shapes.length === 0) return undefined
  if (!isPureMove(previous, next)) return undefined

  const dx = next.x - previous.x
  const dy = next.y - previous.y
  if (dx === 0 && dy === 0) return undefined

  let changed = false
  const translated = shapes.map((shape) => {
    const nextShape = translateSelectionShape(shape, dx, dy)
    if (nextShape !== shape) changed = true
    return nextShape
  })
  if (!changed) return undefined

  return {
    ...workflow,
    selection: {
      ...workflow.selection,
      shapes: translated,
    },
  }
}

function translateSelectionShape(
  shape: TextSelectionShape,
  dx: number,
  dy: number,
): TextSelectionShape {
  switch (shape.kind) {
    case 'polygon':
      return {
        kind: 'polygon',
        points: shape.points.map(([x, y]) => [(x ?? 0) + dx, (y ?? 0) + dy]),
      }
    case 'rectangle':
      return {
        kind: 'rectangle',
        transform: {
          ...shape.transform,
          x: shape.transform.x + dx,
          y: shape.transform.y + dy,
        },
      }
    case 'brush':
      return shape
  }
}

function isPureMove(previous: Transform, next: Transform): boolean {
  return (
    previous.width === next.width &&
    previous.height === next.height &&
    (previous.rotationDeg ?? 0) === (next.rotationDeg ?? 0)
  )
}
