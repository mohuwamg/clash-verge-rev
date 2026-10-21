import { describe, expect, it } from 'vitest'

import { resolveMenuOrder } from './use-nav-menu-order'

const defaultOrder = ['/codex', '/', '/proxies', '/settings']
const items = new Map(defaultOrder.map((path) => [path, { path }]))

describe('navigation order migration', () => {
  it('places Codex above Home for a new menu', () => {
    expect(resolveMenuOrder(undefined, defaultOrder, items)).toEqual(
      defaultOrder,
    )
  })

  it('inserts Codex above Home while preserving the saved page order', () => {
    expect(
      resolveMenuOrder(['/settings', '/', '/proxies'], defaultOrder, items),
    ).toEqual(['/settings', '/codex', '/', '/proxies'])
  })

  it('keeps an explicitly saved Codex position', () => {
    const savedOrder = ['/', '/proxies', '/codex', '/settings']
    expect(resolveMenuOrder(savedOrder, defaultOrder, items)).toEqual(
      savedOrder,
    )
  })

  it('removes stale and duplicate paths without losing the new page', () => {
    expect(
      resolveMenuOrder(
        ['/', '/removed', '/', '/settings'],
        defaultOrder,
        items,
      ),
    ).toEqual(['/codex', '/', '/settings', '/proxies'])
  })

  it('inserts Codex before Home when both are missing from a saved order', () => {
    expect(resolveMenuOrder(['/settings'], defaultOrder, items)).toEqual([
      '/settings',
      '/codex',
      '/',
      '/proxies',
    ])
  })
})
