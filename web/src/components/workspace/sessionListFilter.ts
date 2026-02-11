export const ALL_AGENTS_FILTER_VALUE = '__all_agents__'

export function encodeAgentFilterValue(agentFilter: string | null): string {
  return agentFilter ?? ALL_AGENTS_FILTER_VALUE
}

export function decodeAgentFilterValue(value: string): string | null {
  return value === ALL_AGENTS_FILTER_VALUE ? null : value
}
