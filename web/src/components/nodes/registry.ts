import type { Component } from 'vue'
import { NODE_TYPE } from '@/constants'
import AgentNode from './agent/AgentNode.vue'
import HttpNode from './http/HttpNode.vue'
import ManualTriggerNode from './trigger/ManualTriggerNode.vue'
import WebhookTriggerNode from './trigger/WebhookTriggerNode.vue'
import ScheduleTriggerNode from './trigger/ScheduleTriggerNode.vue'

/**
 * Node component registry
 * Maps node types to their corresponding Vue components
 */
export const NODE_COMPONENT_REGISTRY: Record<string, Component> = {
  [NODE_TYPE.AGENT]: AgentNode,
  [NODE_TYPE.HTTP_REQUEST]: HttpNode,
  [NODE_TYPE.MANUAL_TRIGGER]: ManualTriggerNode,
  [NODE_TYPE.WEBHOOK_TRIGGER]: WebhookTriggerNode,
  [NODE_TYPE.SCHEDULE_TRIGGER]: ScheduleTriggerNode,
}

export function getNodeComponent(nodeType: string): Component | undefined {
  return NODE_COMPONENT_REGISTRY[nodeType]
}

export function isNodeTypeRegistered(nodeType: string): boolean {
  return nodeType in NODE_COMPONENT_REGISTRY
}

export function getRegisteredNodeTypes(): string[] {
  return Object.keys(NODE_COMPONENT_REGISTRY)
}
