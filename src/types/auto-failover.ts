export interface AutoFailoverConfig {
  enabled: boolean
  subscription_url: string
  profile_uid?: string | null
  websites: string[]
  delay_urls: string[]
  delay_timeout_ms: number
  website_timeout_ms: number
  concurrency: number
}

export interface NodeHealth {
  name: string
  delay_ms: number | null
  checked_at: number
  error: string | null
}

export interface WebsiteHealth {
  url: string
  node: string
  reachable: boolean
  verification_required: boolean
  status_code: number | null
  elapsed_ms: number
  checked_at: number
  error: string | null
}

export interface NodeSwitchRecord {
  id: string
  switched_at: number
  from_node: string | null
  to_node: string
  reason: 'automatic' | 'manual' | 'restore'
}

export interface AutoFailoverStatus {
  phase:
    | 'disabled'
    | 'paused'
    | 'waiting_core'
    | 'checking'
    | 'healthy'
    | 'verification_required'
    | 'switching'
    | 'updating_subscription'
    | 'retry_wait'
  message: string | null
  current_node: string | null
  last_healthy_node: string | null
  nodes: NodeHealth[]
  websites: WebsiteHealth[]
  switch_history: NodeSwitchRecord[]
  switch_history_error: boolean
  checked_at: number | null
  next_check_at: number | null
  subscription_updated_at: number | null
  measuring: boolean
  generation: number
}

export const AUTO_FAILOVER_GROUP = 'ZK Auto Failover'
export const defaultAutoFailoverConfig: AutoFailoverConfig = {
  enabled: false,
  subscription_url: '',
  websites: ['https://api.openai.com/v1/models'],
  delay_urls: ['https://api.openai.com/v1/models'],
  delay_timeout_ms: 5000,
  website_timeout_ms: 10000,
  concurrency: 5,
}
