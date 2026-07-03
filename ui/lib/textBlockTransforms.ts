export type Point = {
  x: number
  y: number
}

export type RotationDrag = {
  center: Point
  startPoint: Point
  currentPoint: Point
  startRotationDeg: number
}

export function rotationFromPointer(center: Point, point: Point): number {
  const radians = Math.atan2(point.x - center.x, center.y - point.y)
  return normalizeRotation((radians * 180) / Math.PI)
}

export function resolveRotationDrag({
  center,
  startPoint,
  currentPoint,
  startRotationDeg,
}: RotationDrag): number {
  const startAngle = rotationFromPointer(center, startPoint)
  const currentAngle = rotationFromPointer(center, currentPoint)
  return normalizeRotation(startRotationDeg + currentAngle - startAngle)
}

export function normalizeRotation(degrees: number): number {
  let normalized = degrees % 360
  if (normalized > 180) normalized -= 360
  if (normalized <= -180) normalized += 360
  return Math.round(normalized)
}
