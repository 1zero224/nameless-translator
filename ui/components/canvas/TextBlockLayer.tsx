'use client'

import { useDrag } from '@use-gesture/react'
import { useMemo, useRef } from 'react'
import { useHotkeys } from 'react-hotkeys-hook'

import { useBlobImage } from '@/hooks/useBlobData'
import { useCurrentPage, useTextNodes, type TextNodeEntry } from '@/hooks/useCurrentPage'
import type { NodeDataPatch, Transform } from '@/lib/api/schemas'
import { applyOp, queueAutoRender } from '@/lib/io/scene'
import { ops } from '@/lib/ops'
import { useEditorUiStore } from '@/lib/stores/editorUiStore'
import { useSelectionStore } from '@/lib/stores/selectionStore'
import { resolveRotationDrag, type Point } from '@/lib/textBlockTransforms'

type TextBlockLayerProps = {
  showSprites?: boolean
  hiddenSpriteNodeIds?: ReadonlySet<string>
  scale: number
  style?: React.CSSProperties
}

/**
 * Overlay for the active page's Text nodes. Each rectangle is draggable /
 * resizable; commits dispatch `Op::UpdateNode { transform }` through
 * `applyCommand`. Selection is driven by `selectionStore.nodeIds`.
 */
export function TextBlockLayer({
  showSprites,
  hiddenSpriteNodeIds,
  scale,
  style,
}: TextBlockLayerProps) {
  const nodes = useTextNodes()
  const page = useCurrentPage()
  const selectedIds = useSelectionStore((s) => s.nodeIds)
  const select = useSelectionStore((s) => s.select)
  const mode = useEditorUiStore((s) => s.mode)
  const interactive = mode === 'select' || mode === 'block'

  const hasSelection = useMemo(() => {
    for (const id of selectedIds) if (id) return true
    return false
  }, [selectedIds])

  const removeNode = async (id: string) => {
    if (!page) return
    const node = page.nodes[id]
    if (!node) return
    const idx = Object.keys(page.nodes).indexOf(id)
    await applyOp(ops.removeNode(page.id, id, node, idx < 0 ? 0 : idx))
    if ('text' in node.kind) queueAutoRender(page.id)
  }

  const removeSelected = async () => {
    if (!page) return
    // Snapshot selection now: each op invalidates the page state by removing a
    // node, so we can't iterate against a stale closure mid-loop.
    const ids = Array.from(selectedIds).filter((id): id is string => !!id)
    for (const id of ids) {
      await removeNode(id)
    }
  }

  const updateTransform = async (id: string, t: Transform) => {
    if (!page) return
    const data: NodeDataPatch = {
      text: {
        lockLayoutBox: true,
      },
    }
    await applyOp(ops.updateNode(page.id, id, { transform: t, data }))
    queueAutoRender(page.id)
  }

  useHotkeys(
    'delete',
    () => {
      if (hasSelection && interactive) void removeSelected()
    },
    { enabled: hasSelection && interactive },
    [selectedIds, interactive],
  )

  return (
    <div
      data-text-block-layer
      style={{
        ...style,
        position: 'absolute',
        inset: 0,
        width: '100%',
        height: '100%',
        pointerEvents: 'none',
      }}
    >
      {showSprites &&
        nodes
          .filter((n) => !hiddenSpriteNodeIds?.has(n.id))
          .map((n, i) => <BlockSprite key={`sprite-${n.id ?? i}`} node={n} scale={scale} />)}
      {nodes.map((n, i) => (
        <TextBlockItem
          key={n.id}
          node={n}
          index={i}
          scale={scale}
          selected={selectedIds.has(n.id)}
          interactive={interactive}
          onSelect={(id, additive) => select(id, additive)}
          onCommit={(t) => void updateTransform(n.id, t)}
        />
      ))}
    </div>
  )
}

type TextBlockItemProps = {
  node: TextNodeEntry
  index: number
  scale: number
  selected: boolean
  interactive: boolean
  onSelect: (id: string, additive: boolean) => void
  onCommit: (transform: Transform) => void
}

const isAdditiveEvent = (event: unknown): boolean => {
  if (!event || typeof event !== 'object') return false
  const e = event as { shiftKey?: boolean; metaKey?: boolean; ctrlKey?: boolean }
  return !!(e.shiftKey || e.metaKey || e.ctrlKey)
}

const RESIZE_HANDLE_SIZE = 8
const ROTATE_HANDLE_OFFSET = 24

type ResizeEdge = { top: boolean; bottom: boolean; left: boolean; right: boolean }
type RotationDragStart = {
  center: Point
  startPoint: Point
  rotationDeg: number
}

function TextBlockItem({
  node,
  index,
  scale,
  selected,
  interactive,
  onSelect,
  onCommit,
}: TextBlockItemProps) {
  const boxRef = useRef<HTMLDivElement>(null)
  const dragStart = useRef({ x: 0, y: 0, w: 0, h: 0 })
  const rotationStart = useRef<RotationDragStart | null>(null)
  const edgeRef = useRef<ResizeEdge | null>(null)
  const isResizeRef = useRef(false)
  const isRotateRef = useRef(false)

  const setBox = (x: number, y: number, w: number, h: number, rotationDeg: number) => {
    const el = boxRef.current
    if (!el) return
    el.style.transform = `translate(${x}px, ${y}px) rotate(${rotationDeg}deg)`
    el.style.width = `${w}px`
    el.style.height = `${h}px`
  }

  const t = node.transform

  const bind = useDrag(
    ({ first, last, movement: [mx, my], event, tap }) => {
      if (!interactive) return
      event?.stopPropagation()
      const additive = isAdditiveEvent(event)
      if (tap) {
        if (isResizeRef.current || isRotateRef.current) {
          isResizeRef.current = false
          isRotateRef.current = false
          edgeRef.current = null
          rotationStart.current = null
          return
        }
        onSelect(node.id, additive)
        return
      }
      if (first) {
        dragStart.current = {
          x: t.x * scale,
          y: t.y * scale,
          w: t.width * scale,
          h: t.height * scale,
        }
        if (isRotateRef.current) {
          rotationStart.current = rotationDragStart(boxRef.current, event, t.rotationDeg ?? 0)
        }
        // Keep multi-selection intact when dragging a node that's already selected;
        // otherwise this click is a single-select (unless the modifier is held).
        if (additive || !selected) onSelect(node.id, additive)
      }
      const { x: sx, y: sy, w: sw, h: sh } = dragStart.current
      const edge = edgeRef.current
      if (isRotateRef.current) {
        const currentPoint = pointFromEvent(event)
        const start = rotationStart.current
        if (!currentPoint || !start) return
        const rotationDeg = resolveRotationDrag({
          center: start.center,
          startPoint: start.startPoint,
          currentPoint,
          startRotationDeg: start.rotationDeg,
        })
        setBox(sx, sy, sw, sh, rotationDeg)
        if (last) {
          isRotateRef.current = false
          rotationStart.current = null
          onCommit({
            x: t.x,
            y: t.y,
            width: t.width,
            height: t.height,
            rotationDeg,
          })
        }
      } else if (isResizeRef.current && edge) {
        let dx = 0
        let dy = 0
        let w = sw
        let h = sh
        if (edge.right) w += mx
        if (edge.left) {
          w -= mx
          dx = mx
        }
        if (edge.bottom) h += my
        if (edge.top) {
          h -= my
          dy = my
        }
        w = Math.max(4 * scale, w)
        h = Math.max(4 * scale, h)
        if (edge.left && w === 4 * scale) dx = sw - 4 * scale
        if (edge.top && h === 4 * scale) dy = sh - 4 * scale
        setBox(sx + dx, sy + dy, w, h, t.rotationDeg ?? 0)
        if (last) {
          isResizeRef.current = false
          edgeRef.current = null
          onCommit({
            x: Math.round((sx + dx) / scale),
            y: Math.round((sy + dy) / scale),
            width: Math.max(4, Math.round(w / scale)),
            height: Math.max(4, Math.round(h / scale)),
            rotationDeg: t.rotationDeg ?? 0,
          })
        }
      } else {
        setBox(sx + mx, sy + my, sw, sh, t.rotationDeg ?? 0)
        if (last) {
          onCommit({
            x: Math.round((sx + mx) / scale),
            y: Math.round((sy + my) / scale),
            width: t.width,
            height: t.height,
            rotationDeg: t.rotationDeg ?? 0,
          })
        }
      }
    },
    {
      pointer: { buttons: 1, touch: true },
      filterTaps: true,
      preventDefault: true,
      eventOptions: { passive: false },
    },
  )

  const handleEdgePointerDown = (edge: ResizeEdge) => {
    if (!interactive || !selected) return
    isResizeRef.current = true
    edgeRef.current = edge
  }

  const handleRotatePointerDown = () => {
    if (!interactive || !selected) return
    isRotateRef.current = true
    rotationStart.current = null
  }

  const w = t.width * scale
  const h = t.height * scale
  const rotationDeg = t.rotationDeg ?? 0

  return (
    <div
      ref={boxRef}
      {...bind()}
      data-testid={`text-block-${node.id}`}
      style={{
        position: 'absolute',
        top: 0,
        left: 0,
        transform: `translate(${t.x * scale}px, ${t.y * scale}px) rotate(${rotationDeg}deg)`,
        width: w,
        height: h,
        transformOrigin: 'center center',
        pointerEvents: interactive ? 'auto' : 'none',
        zIndex: selected ? 20 : 10,
        touchAction: 'none',
        cursor: interactive ? 'move' : 'default',
      }}
    >
      <div
        className={`absolute inset-0 rounded-md ${
          selected
            ? 'border-[3px] border-primary bg-primary/15'
            : 'border-2 border-rose-400/60 bg-rose-400/5'
        }`}
      />
      <div
        className={`pointer-events-none absolute -top-1.5 -left-1.5 flex h-4 w-4 items-center justify-center rounded-full text-[9px] font-semibold text-white shadow ${
          selected ? 'bg-primary' : 'bg-rose-400'
        }`}
      >
        {index + 1}
      </div>
      {selected && interactive && (
        <>
          <RotateHandle nodeId={node.id} onPointerDown={handleRotatePointerDown} />
          <ResizeHandles onEdgePointerDown={handleEdgePointerDown} />
        </>
      )}
    </div>
  )
}

function pointFromEvent(event: unknown): Point | null {
  if (!event || typeof event !== 'object') return null
  const e = event as { clientX?: number; clientY?: number }
  if (typeof e.clientX !== 'number' || typeof e.clientY !== 'number') return null
  return { x: e.clientX, y: e.clientY }
}

function rotationDragStart(
  el: HTMLElement | null,
  event: unknown,
  rotationDeg: number,
): RotationDragStart | null {
  const point = pointFromEvent(event)
  if (!el || !point) return null
  const rect = el.getBoundingClientRect()
  return {
    center: { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 },
    startPoint: point,
    rotationDeg,
  }
}

function BlockSprite({ node, scale }: { node: TextNodeEntry; scale: number }) {
  const sprite = (node.data.sprite as string | null | undefined) ?? undefined
  const { data: src } = useBlobImage(sprite)
  if (!src) return null
  const spriteT = node.data.spriteTransform
  const x = (spriteT?.x ?? node.transform.x) * scale
  const y = (spriteT?.y ?? node.transform.y) * scale
  return (
    <img
      alt=''
      src={src}
      draggable={false}
      className='pointer-events-none absolute select-none'
      style={{
        top: 0,
        left: 0,
        transformOrigin: 'top left',
        transform: `translate(${x}px, ${y}px) scale(${scale})`,
      }}
    />
  )
}

function RotateHandle({ nodeId, onPointerDown }: { nodeId: string; onPointerDown: () => void }) {
  return (
    <>
      <div
        aria-hidden='true'
        className='pointer-events-none absolute left-1/2 w-px -translate-x-1/2 bg-primary/70'
        style={{ top: -ROTATE_HANDLE_OFFSET, height: ROTATE_HANDLE_OFFSET - 6 }}
      />
      <div
        data-testid={`text-block-rotate-${nodeId}`}
        aria-label='Rotate text block'
        onPointerDown={onPointerDown}
        className='absolute left-1/2 size-3 -translate-x-1/2 rounded-full border-2 border-white bg-primary shadow-sm'
        style={{
          top: -ROTATE_HANDLE_OFFSET - 6,
          cursor: 'grab',
          zIndex: 35,
        }}
      />
    </>
  )
}

function ResizeHandles({ onEdgePointerDown }: { onEdgePointerDown: (edge: ResizeEdge) => void }) {
  const s = RESIZE_HANDLE_SIZE
  const half = s / 2

  const edges: { edge: ResizeEdge; style: React.CSSProperties; cursor: string }[] = [
    {
      edge: { top: true, left: true, bottom: false, right: false },
      cursor: 'nwse-resize',
      style: { top: -half, left: -half, width: s, height: s },
    },
    {
      edge: { top: true, left: false, bottom: false, right: true },
      cursor: 'nesw-resize',
      style: { top: -half, right: -half, width: s, height: s },
    },
    {
      edge: { top: false, left: true, bottom: true, right: false },
      cursor: 'nesw-resize',
      style: { bottom: -half, left: -half, width: s, height: s },
    },
    {
      edge: { top: false, left: false, bottom: true, right: true },
      cursor: 'nwse-resize',
      style: { bottom: -half, right: -half, width: s, height: s },
    },
    {
      edge: { top: true, left: false, bottom: false, right: false },
      cursor: 'ns-resize',
      style: { top: -half, left: s, right: s, height: s },
    },
    {
      edge: { top: false, left: false, bottom: true, right: false },
      cursor: 'ns-resize',
      style: { bottom: -half, left: s, right: s, height: s },
    },
    {
      edge: { top: false, left: true, bottom: false, right: false },
      cursor: 'ew-resize',
      style: { left: -half, top: s, bottom: s, width: s },
    },
    {
      edge: { top: false, left: false, bottom: false, right: true },
      cursor: 'ew-resize',
      style: { right: -half, top: s, bottom: s, width: s },
    },
  ]

  return (
    <>
      {edges.map((e, i) => (
        <div
          key={i}
          onPointerDown={() => onEdgePointerDown(e.edge)}
          style={{ position: 'absolute', ...e.style, cursor: e.cursor, zIndex: 30 }}
        />
      ))}
    </>
  )
}
