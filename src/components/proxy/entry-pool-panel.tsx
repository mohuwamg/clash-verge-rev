import {
  Alert,
  Autocomplete,
  Box,
  Button,
  Checkbox,
  Stack,
  TextField,
  Typography,
} from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useRuntimeConfig } from '@/hooks/use-clash'
import { useProfiles } from '@/hooks/use-profiles'
import { useVerge } from '@/hooks/use-verge'
import { useAppRefreshers, useProxiesData } from '@/providers/app-data-context'
import { setEntryPoolConfig } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import {
  emptyEntryPool,
  ENTRY_POOL_GROUP,
  entryPoolIsApplied,
  FAILOVER_GROUP,
  type EntryPoolConfig,
} from '@/types/entry-pool'

interface Props {
  mode?: string
  selectedGroup?: string | null
}

export function EntryPoolPanel(props: Props) {
  const { verge } = useVerge()
  const { profiles } = useProfiles()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const saved = verge?.entry_pool ?? emptyEntryPool
  return (
    <EntryPoolEditor
      key={JSON.stringify([saved, profiles?.current])}
      {...props}
      saved={saved}
      busy={busy}
      setBusy={setBusy}
      error={error}
      setError={setError}
    />
  )
}

function EntryPoolEditor({
  mode,
  selectedGroup,
  saved,
  busy,
  setBusy,
  error,
  setError,
}: Props & {
  saved: EntryPoolConfig
  busy: boolean
  setBusy: (value: boolean) => void
  error: string
  setError: (value: string) => void
}) {
  const { t } = useTranslation()
  const { verge, mutateVerge } = useVerge()
  const { profiles } = useProfiles()
  const { data: runtime, refetch } = useRuntimeConfig()
  const { proxyView } = useProxiesData()
  const { refreshProxy } = useAppRefreshers()
  const [entries, setEntries] = useState(saved.entries)
  const [exit, setExit] = useState(saved.exit_node)
  const rawProxies =
    (runtime as { proxies?: Array<{ name?: string }> } | undefined)?.proxies ??
    []
  const names = rawProxies.flatMap((p) =>
    typeof p.name === 'string' ? [p.name] : [],
  )
  const managed =
    verge?.auto_failover?.enabled &&
    verge.auto_failover.profile_uid === profiles?.current
  const target = managed
    ? FAILOVER_GROUP
    : mode === 'global'
      ? 'GLOBAL'
      : (selectedGroup ?? '')
  const activeProfile = saved.profile_uid === profiles?.current
  const applied = activeProfile && entryPoolIsApplied(runtime, saved)
  const entryNow = proxyView?.groups.find(
    (g) => g.name === ENTRY_POOL_GROUP,
  )?.now
  const exitNow =
    saved.target_group === 'GLOBAL'
      ? proxyView?.global?.now
      : proxyView?.groups.find((g) => g.name === saved.target_group)?.now

  const save = async (enabled: boolean) => {
    setBusy(true)
    setError('')
    try {
      await setEntryPoolConfig(
        enabled
          ? {
              enabled,
              entries,
              exit_node: exit,
              target_group: target,
            }
          : { ...saved, enabled: false },
      )
      await refetch()
      await refreshProxy()
      mutateVerge()
      showNotice.success(t('proxies.page.chain.pool.saved'))
    } catch (err) {
      setError(String(err))
      mutateVerge()
    } finally {
      setBusy(false)
    }
  }

  return (
    <Stack spacing={2} sx={{ py: 2 }}>
      <Alert severity="info">{t('proxies.page.chain.pool.hint')}</Alert>
      <Autocomplete
        multiple
        disableCloseOnSelect
        limitTags={3}
        options={names.filter((name) => name !== exit)}
        value={entries}
        disabled={busy}
        onChange={(_, value) => setEntries(value)}
        renderOption={(props, option, { selected }) => {
          const { key, ...optionProps } = props
          return (
            <li key={key} {...optionProps}>
              <Checkbox
                checked={selected}
                tabIndex={-1}
                disableRipple
                aria-hidden
                sx={{ mr: 1 }}
              />
              {option}
            </li>
          )
        }}
        renderInput={(params) => (
          <TextField {...params} label={t('proxies.page.chain.pool.entries')} />
        )}
      />
      <Autocomplete
        options={names.filter((name) => !entries.includes(name))}
        value={exit || null}
        disabled={busy}
        onChange={(_, value) => setExit(value ?? '')}
        renderInput={(params) => (
          <TextField {...params} label={t('proxies.page.chain.pool.exit')} />
        )}
      />
      {!managed && (
        <Typography variant="body2">
          {t('proxies.page.chain.pool.target')}: {target || '—'}
        </Typography>
      )}
      {saved.enabled && (
        <Alert severity={applied ? 'success' : 'warning'}>
          {applied
            ? t('proxies.page.chain.pool.applied')
            : t('proxies.page.chain.pool.inactive')}
          {applied && (
            <Box>
              {t('proxies.page.chain.entryNode')}: {entryNow ?? '—'} →{' '}
              {t('proxies.page.chain.exitNode')}: {exitNow ?? '—'}
            </Box>
          )}
        </Alert>
      )}
      {error && <Alert severity="error">{error}</Alert>}
      {mode === 'direct' && (
        <Alert severity="warning">{t('proxies.page.chain.pool.direct')}</Alert>
      )}
      <Box sx={{ display: 'flex', gap: 1 }}>
        <Button
          variant="contained"
          disabled={
            busy ||
            !profiles?.current ||
            !target ||
            entries.length === 0 ||
            !exit ||
            mode === 'direct'
          }
          onClick={() => void save(true)}
        >
          {t('proxies.page.chain.pool.apply')}
        </Button>
        <Button
          disabled={busy || !saved.enabled}
          onClick={() => void save(false)}
        >
          {t('proxies.page.chain.pool.disable')}
        </Button>
      </Box>
    </Stack>
  )
}
