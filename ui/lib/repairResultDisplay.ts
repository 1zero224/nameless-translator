import type { Node, Page } from '@/lib/api/schemas'
import { normalizeWorkflow } from '@/lib/workflow'

export type RepairLayerDisplay = {
  id: string
  blob: string
  opacity: number
}

export type RepairResultDisplay = {
  repairTextNodeIds: Set<string>
  repairLayers: RepairLayerDisplay[]
}

export function resolveRepairResultDisplay(page: Page): RepairResultDisplay {
  const repairTextNodeIds = new Set<string>()
  const repairLayerIds = new Set<string>()

  for (const [id, node] of Object.entries(page.nodes)) {
    if (!isTextNode(node)) continue
    const workflow = normalizeWorkflow(node.kind.text)
    if (workflow.resultMode !== 'repair' || !workflow.repairLayer) continue
    repairTextNodeIds.add(id)
    repairLayerIds.add(workflow.repairLayer)
  }

  const repairLayers: RepairLayerDisplay[] = []
  const seen = new Set<string>()
  for (const [id, node] of Object.entries(page.nodes)) {
    if (!repairLayerIds.has(id) || seen.has(id) || !isImageNode(node)) continue
    const image = node.kind.image
    if (image.role !== 'custom') continue
    seen.add(id)
    repairLayers.push({
      id,
      blob: image.blob,
      opacity: image.opacity ?? 1,
    })
  }

  return { repairTextNodeIds, repairLayers }
}

function isImageNode(
  node: Node,
): node is Node & { kind: { image: import('@/lib/api/schemas').ImageData } } {
  return 'image' in node.kind
}

function isTextNode(
  node: Node,
): node is Node & { kind: { text: import('@/lib/api/schemas').TextData } } {
  return 'text' in node.kind
}
