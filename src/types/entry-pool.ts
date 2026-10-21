export interface EntryPoolConfig {
  enabled: boolean
  profile_uid?: string | null
  entries: string[]
  exit_node: string
  target_group: string
}

export const ENTRY_POOL_GROUP = 'ZK Chain Entry'
export const FAILOVER_GROUP = 'ZK Auto Failover'

export const emptyEntryPool: EntryPoolConfig = {
  enabled: false,
  entries: [],
  exit_node: '',
  target_group: '',
}

export function entryPoolIsApplied(runtime: unknown, config: EntryPoolConfig) {
  if (!config.enabled || typeof runtime !== 'object' || runtime === null)
    return false
  const data = runtime as Record<string, unknown>
  const groups = data['proxy-groups']
  const proxies = data.proxies
  if (!Array.isArray(groups) || !Array.isArray(proxies)) return false
  const group = groups.find((g) => g?.name === ENTRY_POOL_GROUP)
  const exit = proxies.find((p) => p?.name === config.exit_node)
  return (
    group?.type === 'fallback' &&
    Array.isArray(group.proxies) &&
    group.proxies.length === config.entries.length &&
    group.proxies.every(
      (name: string, index: number) => name === config.entries[index],
    ) &&
    exit?.['dialer-proxy'] === ENTRY_POOL_GROUP
  )
}
