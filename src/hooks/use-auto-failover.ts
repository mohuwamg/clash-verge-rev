import { useEffect, useState } from 'react'

import { getAutoFailoverStatus } from '@/services/cmds'
import delayManager from '@/services/delay'
import { subscribeVergeEvents } from '@/services/events'
import {
  AUTO_FAILOVER_GROUP,
  type AutoFailoverStatus,
} from '@/types/auto-failover'

export function useAutoFailover() {
  const [status, setStatus] = useState<AutoFailoverStatus>()
  useEffect(() => {
    let disposed = false
    let revision = 0
    const apply = (next: AutoFailoverStatus) => {
      if (disposed) return
      setStatus(next)
      for (const node of next.nodes) {
        if (Date.now() - node.checked_at <= 60_000) {
          delayManager.setDelay(
            node.name,
            AUTO_FAILOVER_GROUP,
            node.delay_ms ?? 1e6,
          )
        }
      }
    }
    const unsubscribe = subscribeVergeEvents(
      {
        'verge://auto-failover': (next) => {
          revision++
          apply(next)
        },
      },
      () => {
        const captured = revision
        getAutoFailoverStatus()
          .then((next) => {
            if (revision === captured) apply(next)
          })
          .catch((error: unknown) =>
            console.error('[AutoFailover] Status unavailable', error),
          )
      },
    )
    return () => {
      disposed = true
      unsubscribe()
    }
  }, [])
  return status
}
