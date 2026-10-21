import { describe, expect, it } from 'vitest'

import {
  ENTRY_POOL_GROUP,
  entryPoolIsApplied,
  type EntryPoolConfig,
} from './entry-pool'

const config: EntryPoolConfig = {
  enabled: true,
  entries: ['Entry A', 'Entry B'],
  exit_node: 'Exit',
  target_group: 'GLOBAL',
}
const runtime = {
  proxies: [{ name: 'Exit', 'dialer-proxy': ENTRY_POOL_GROUP }],
  'proxy-groups': [
    {
      name: ENTRY_POOL_GROUP,
      type: 'fallback',
      proxies: ['Entry A', 'Entry B'],
    },
  ],
}

describe('entrance pool applied status', () => {
  it('requires the real fallback group and exit dialer link, not saved UI state', () => {
    expect(entryPoolIsApplied(runtime, config)).toBe(true)
    expect(entryPoolIsApplied({}, config)).toBe(false)
    expect(entryPoolIsApplied(undefined, config)).toBe(false)
    expect(
      entryPoolIsApplied({ ...runtime, proxies: [{ name: 'Exit' }] }, config),
    ).toBe(false)
    expect(entryPoolIsApplied(runtime, { ...config, enabled: false })).toBe(
      false,
    )
  })
  it('rejects stale or reordered entrances and a different exit', () => {
    expect(
      entryPoolIsApplied(runtime, {
        ...config,
        entries: ['Entry B', 'Entry A'],
      }),
    ).toBe(false)
    expect(
      entryPoolIsApplied(runtime, { ...config, entries: ['Entry A'] }),
    ).toBe(false)
    expect(entryPoolIsApplied(runtime, { ...config, exit_node: 'Other' })).toBe(
      false,
    )
  })
})
