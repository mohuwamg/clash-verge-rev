import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { defaultAutoFailoverConfig } from '@/types/auto-failover'

import AutoFailoverOverview from './auto-failover-overview'

const hooks = vi.hoisted(() => ({ status: vi.fn(), verge: vi.fn() }))
vi.mock('@/hooks/use-auto-failover', () => ({ useAutoFailover: hooks.status }))
vi.mock('@/hooks/use-verge', () => ({ useVerge: hooks.verge }))
vi.mock('@/services/cmds', () => ({
  checkAutoFailoverNow: vi.fn(),
  setAutoFailoverConfig: vi.fn(),
}))
vi.mock('@/services/notice-service', () => ({ showNotice: { error: vi.fn() } }))
vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}))
vi.mock('./auto-failover-config-dialog', () => ({ default: () => null }))

beforeEach(() => {
  hooks.verge.mockReturnValue({
    verge: { auto_failover: { ...defaultAutoFailoverConfig, enabled: true } },
    mutateVerge: vi.fn(),
  })
})

describe('automatic recovery overview', () => {
  it('labels an accepted API authentication response as connectivity, not a passed webpage', () => {
    hooks.status.mockReturnValue({
      phase: 'healthy',
      current_node: 'Node A',
      nodes: [],
      websites: [
        {
          url: 'https://api.openai.com/v1/models',
          reachable: true,
          status_code: 401,
          elapsed_ms: 50,
          node: 'Node A',
        },
      ],
    })
    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))
    expect(html).toContain('settings.autoFailover.networkReachable')
    expect(html).toContain('HTTP 401')
    expect(html).not.toContain('settings.autoFailover.pageFailed')
    expect(html).not.toContain('settings.autoFailover.reachable')
  })

  it('shows which website failed even when the node latency is healthy', () => {
    hooks.status.mockReturnValue({
      phase: 'switching',
      current_node: 'Node B',
      message: null,
      nodes: [{ name: 'Node B', delay_ms: 80, checked_at: 1 }],
      websites: [
        {
          url: 'https://github.com/',
          reachable: true,
          elapsed_ms: 50,
          node: 'Node B',
        },
        {
          url: 'https://chatgpt.com/',
          reachable: false,
          elapsed_ms: 100,
          error: 'access_denied',
          status_code: 403,
          node: 'Node B',
        },
      ],
    })
    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))
    expect(html).toContain('settings.autoFailover.phases.switching')
    expect(html).toContain('https://github.com/')
    expect(html).toContain('https://chatgpt.com/')
    expect(html).toContain('settings.autoFailover.messages.accessDenied')
    expect(html).toContain('80 ms')
    expect(html).toContain('settings.autoFailover.pageFailed')
    expect(html).toContain('HTTP 403')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
  })

  it('shows a verification challenge as pending without claiming the node is healthy or disconnected', () => {
    hooks.status.mockReturnValue({
      phase: 'verification_required',
      current_node: 'Node A',
      message: 'verification_required',
      nodes: [{ name: 'Node A', delay_ms: 800, checked_at: 1 }],
      websites: [
        {
          url: 'https://chatgpt.com/',
          node: 'Node A',
          reachable: false,
          verification_required: true,
          status_code: 403,
          elapsed_ms: 927,
          error: 'verification_required',
        },
      ],
    })
    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))
    expect(html).toContain('settings.autoFailover.phases.verificationRequired')
    expect(html).toContain('settings.autoFailover.verificationRequired')
    expect(html).toContain(
      'settings.autoFailover.messages.verificationRequired',
    )
    expect(html).toContain('HTTP 403')
    expect(html).toContain('800 ms')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
    expect(html).not.toContain('settings.autoFailover.pageFailed')
    expect(html).not.toContain('settings.autoFailover.requestFailed')
  })

  it('lists switch times, previous and selected nodes, and switch types newest first', () => {
    const time = 1791792000000
    hooks.status.mockReturnValue({
      phase: 'disabled',
      nodes: [],
      websites: [],
      switch_history_error: true,
      switch_history: [
        {
          id: 'switch-3',
          switched_at: time,
          from_node: 'Node B',
          to_node: 'Node C',
          reason: 'manual',
        },
        {
          id: 'switch-2',
          switched_at: time - 1000,
          from_node: 'Node A',
          to_node: 'Node B',
          reason: 'automatic',
        },
        {
          id: 'switch-1',
          switched_at: time - 2000,
          from_node: null,
          to_node: 'Node A',
          reason: 'restore',
        },
      ],
    })
    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))
    expect(html).toContain(new Date(time).toLocaleString())
    expect(html).toContain('Node B')
    expect(html).toContain('Node C')
    expect(html).toContain('settings.autoFailover.history.manual')
    expect(html).toContain('settings.autoFailover.history.automatic')
    expect(html).toContain('settings.autoFailover.history.restore')
    expect(html).toContain('settings.autoFailover.history.unknown')
    expect(html).toContain('settings.autoFailover.history.storageError')
    expect(html.indexOf('Node C')).toBeLessThan(html.indexOf('Node A'))
    expect(html).not.toContain('settings.autoFailover.history.empty')
  })

  it('shows disabled while no background status has been loaded', () => {
    hooks.status.mockReturnValue(undefined)
    hooks.verge.mockReturnValue({
      verge: {
        auto_failover: { ...defaultAutoFailoverConfig, enabled: false },
      },
      mutateVerge: vi.fn(),
    })
    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))
    expect(html).toContain('settings.autoFailover.phases.disabled')
    expect(html).toContain('disabled=""')
    expect(html).toContain('settings.autoFailover.history.empty')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
  })

  it('shows loading when recovery is enabled but no background status has arrived', () => {
    hooks.status.mockReturnValue(undefined)

    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))

    expect(html).toContain('settings.autoFailover.overview.loading')
    expect(html).not.toContain('settings.autoFailover.phases.disabled')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
  })

  it('does not treat a rejected API authentication response as connectivity', () => {
    hooks.status.mockReturnValue({
      phase: 'retry_wait',
      current_node: 'Node A',
      nodes: [],
      websites: [
        {
          url: 'https://api.openai.com/v1/models',
          reachable: false,
          verification_required: false,
          status_code: 401,
          elapsed_ms: 50,
          node: 'Node A',
          error: 'access_denied',
        },
      ],
    })

    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))

    expect(html).toContain('HTTP 401')
    expect(html).toContain('settings.autoFailover.pageFailed')
    expect(html).toContain('settings.autoFailover.messages.accessDenied')
    expect(html).not.toContain('settings.autoFailover.networkReachable')
    expect(html).not.toContain('settings.autoFailover.reachable')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
  })

  it('shows disabled after recovery is turned off even if the last status was healthy', () => {
    hooks.verge.mockReturnValue({
      verge: {
        auto_failover: { ...defaultAutoFailoverConfig, enabled: false },
      },
      mutateVerge: vi.fn(),
    })
    hooks.status.mockReturnValue({
      phase: 'healthy',
      current_node: 'Node A',
      nodes: [],
      websites: [],
    })

    const html = renderToStaticMarkup(createElement(AutoFailoverOverview))

    expect(html).toContain('settings.autoFailover.phases.disabled')
    expect(html).toContain('settings.autoFailover.overview.disabledHint')
    expect(html).toContain('disabled=""')
    expect(html).not.toContain('settings.autoFailover.phases.healthy')
    expect(html).not.toContain('settings.autoFailover.overview.monitoring')
  })
})
