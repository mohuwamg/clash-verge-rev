import {
  ArrowForwardRounded,
  CheckCircleOutlineRounded,
  ChevronRightRounded,
  HistoryRounded,
  LanguageRounded,
  RefreshRounded,
  SettingsOutlined,
  ShieldOutlined,
  SpeedRounded,
  TuneRounded,
} from '@mui/icons-material'
import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  Divider,
  FormControlLabel,
  Stack,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Typography,
  alpha,
  useTheme,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { useAutoFailover } from '@/hooks/use-auto-failover'
import { useVerge } from '@/hooks/use-verge'
import { checkAutoFailoverNow, setAutoFailoverConfig } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import { defaultAutoFailoverConfig } from '@/types/auto-failover'

import AutoFailoverConfigDialog from './auto-failover-config-dialog'

const formatTime = (value?: number | null) =>
  value ? new Date(value).toLocaleTimeString() : '—'

const subscriptionName = (address: string) => {
  try {
    const url = new URL(address)
    return url.pathname.split('/').filter(Boolean).pop() || url.hostname
  } catch {
    return '—'
  }
}

function RecoveryCard({
  title,
  icon,
  action,
  children,
}: {
  title: string
  icon: ReactNode
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <Box
      component="section"
      sx={{
        minWidth: 0,
        borderRadius: 2,
        bgcolor: (theme) =>
          theme.palette.mode === 'dark' ? '#282a36' : '#ffffff',
      }}
    >
      <Stack
        direction="row"
        sx={{
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 1,
          px: 2,
          py: 1.5,
          borderBottom: 1,
          borderColor: 'divider',
        }}
      >
        <Stack direction="row" sx={{ alignItems: 'center', gap: 1 }}>
          {icon}
          <Typography
            component="h2"
            variant="subtitle1"
            sx={{ fontWeight: 600 }}
          >
            {title}
          </Typography>
        </Stack>
        {action}
      </Stack>
      <Box sx={{ p: 2 }}>{children}</Box>
    </Box>
  )
}

export default function AutoFailoverOverview() {
  const { t } = useTranslation()
  const theme = useTheme()
  const { verge, mutateVerge } = useVerge()
  const status = useAutoFailover()
  const [configOpen, setConfigOpen] = useState(false)
  const [configSaving, setConfigSaving] = useState(false)
  const [toggling, setToggling] = useState(false)
  const [checking, setChecking] = useState(false)
  const config = { ...defaultAutoFailoverConfig, ...verge?.auto_failover }
  const enabled = config.enabled
  const phase = enabled ? (status?.phase ?? 'disabled') : 'disabled'
  const loadingStatus = enabled && !status
  const busy =
    enabled &&
    (checking ||
      status?.measuring ||
      ['checking', 'switching', 'updating_subscription'].includes(phase))
  const phaseLabel = loadingStatus
    ? t('settings.autoFailover.overview.loading')
    : {
        disabled: t('settings.autoFailover.phases.disabled'),
        paused: t('settings.autoFailover.phases.paused'),
        waiting_core: t('settings.autoFailover.phases.waitingCore'),
        checking: t('settings.autoFailover.phases.checking'),
        healthy: t('settings.autoFailover.phases.healthy'),
        verification_required: t(
          'settings.autoFailover.phases.verificationRequired',
        ),
        switching: t('settings.autoFailover.phases.switching'),
        updating_subscription: t(
          'settings.autoFailover.phases.updatingSubscription',
        ),
        retry_wait: t('settings.autoFailover.phases.retryWait'),
      }[phase]
  const statusColor =
    phase === 'healthy'
      ? theme.palette.success.main
      : phase === 'verification_required' || phase === 'retry_wait'
        ? theme.palette.warning.main
        : enabled
          ? theme.palette.primary.main
          : theme.palette.text.secondary
  const panelSx = {
    borderRadius: 2,
    bgcolor: theme.palette.mode === 'dark' ? '#282a36' : '#ffffff',
  }
  const websites = status?.websites ?? []
  const nodes = status?.nodes ?? []
  const switchHistory = status?.switch_history ?? []
  const reasons: Record<string, string> = {
    pair_unavailable: t('settings.autoFailover.messages.allNodesFailed'),
    profile_mismatch: t('settings.autoFailover.messages.profileMismatch'),
    subscription_updating: t(
      'settings.autoFailover.messages.subscriptionUpdating',
    ),
    subscription_update_cooldown: t('settings.autoFailover.messages.cooldown'),
    core_unavailable: t('settings.autoFailover.messages.coreUnavailable'),
    routing_cleanup_pending: t('settings.autoFailover.messages.cleanupPending'),
    direct_mode: t('settings.autoFailover.messages.directMode'),
    verification_required: t(
      'settings.autoFailover.messages.verificationRequired',
    ),
    access_denied: t('settings.autoFailover.messages.accessDenied'),
    rate_limited: t('settings.autoFailover.messages.rateLimited'),
    blocked_page: t('settings.autoFailover.messages.blockedPage'),
  }
  const explain = (message: string) => reasons[message.split(':')[0]] ?? message
  const switchReasons = {
    automatic: t('settings.autoFailover.history.automatic'),
    manual: t('settings.autoFailover.history.manual'),
    restore: t('settings.autoFailover.history.restore'),
  }

  const toggle = useLockFn(async (nextEnabled: boolean) => {
    if (nextEnabled && !config.subscription_url.trim()) {
      setConfigOpen(true)
      return
    }
    setToggling(true)
    try {
      const next = { ...config, enabled: nextEnabled }
      await setAutoFailoverConfig(next)
      mutateVerge((prev) => (prev ? { ...prev, auto_failover: next } : prev))
      mutateVerge()
    } catch (error) {
      showNotice.error(error)
    } finally {
      setToggling(false)
    }
  })
  const check = useLockFn(async () => {
    setChecking(true)
    try {
      await checkAutoFailoverNow()
    } catch (error) {
      showNotice.error(error)
    } finally {
      setChecking(false)
    }
  })

  return (
    <Stack spacing={2} sx={{ p: { xs: 0.5, sm: 1 }, minWidth: 0 }}>
      <Stack
        direction="row"
        sx={{
          justifyContent: 'space-between',
          alignItems: 'center',
          flexWrap: 'wrap',
          gap: 2,
        }}
      >
        <Box>
          <Typography component="h1" variant="h6" sx={{ fontWeight: 600 }}>
            {t('settings.autoFailover.title')}
          </Typography>
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            {t('settings.autoFailover.overview.subtitle')}
          </Typography>
        </Box>
        <Button
          variant="outlined"
          startIcon={<TuneRounded />}
          onClick={() => setConfigOpen(true)}
          disabled={!verge || toggling}
        >
          {t('settings.autoFailover.overview.configure')}
        </Button>
      </Stack>

      <Box
        component="section"
        aria-label={t('settings.autoFailover.overview.status')}
        sx={{ ...panelSx, p: { xs: 2, sm: 3 } }}
      >
        <Stack
          direction="row"
          sx={{
            alignItems: 'center',
            justifyContent: 'space-between',
            flexWrap: 'wrap',
            gap: 2,
          }}
        >
          <Stack
            direction="row"
            sx={{ alignItems: 'center', gap: 2, minWidth: 0 }}
          >
            <Box
              sx={{
                width: 48,
                height: 48,
                flexShrink: 0,
                borderRadius: 2,
                display: 'grid',
                placeItems: 'center',
                bgcolor: alpha(statusColor, 0.1),
                color: statusColor,
              }}
            >
              {busy || loadingStatus ? (
                <CircularProgress size={25} color="inherit" />
              ) : phase === 'healthy' ? (
                <CheckCircleOutlineRounded fontSize="large" />
              ) : (
                <ShieldOutlined fontSize="large" />
              )}
            </Box>
            <Box sx={{ minWidth: 0 }} aria-live="polite">
              <Typography
                variant="h5"
                component="h2"
                sx={{ fontWeight: 600, overflowWrap: 'anywhere' }}
              >
                {phaseLabel}
              </Typography>
              <Typography
                variant="body2"
                color="text.secondary"
                sx={{ mt: 0.5 }}
              >
                {enabled
                  ? t('settings.autoFailover.overview.monitoring')
                  : t('settings.autoFailover.overview.disabledHint')}
              </Typography>
            </Box>
          </Stack>
          <FormControlLabel
            sx={{ m: 0 }}
            labelPlacement="start"
            label={t('settings.autoFailover.enabled')}
            control={
              <Switch
                checked={enabled}
                disabled={!verge || toggling || configSaving}
                onChange={(_, checked) => toggle(checked)}
              />
            }
          />
        </Stack>
        <Divider sx={{ my: 2.5 }} />
        <Box
          sx={{
            display: 'grid',
            gridTemplateColumns: {
              xs: 'minmax(0, 1fr)',
              sm: 'minmax(0, 1.3fr) repeat(2, minmax(0, 1fr))',
            },
            gap: 2.5,
          }}
        >
          {[
            [
              t('settings.autoFailover.currentNode'),
              status?.current_node ?? '—',
            ],
            [
              t('settings.autoFailover.lastCheck'),
              formatTime(status?.checked_at),
            ],
            [
              t('settings.autoFailover.nextCheck'),
              enabled ? formatTime(status?.next_check_at) : '—',
            ],
          ].map(([label, value]) => (
            <Box
              key={label}
              sx={{
                minWidth: 0,
                '& + &': {
                  borderLeft: { sm: 1 },
                  borderColor: { sm: 'divider' },
                  pl: { sm: 2.5 },
                },
              }}
            >
              <Typography variant="body2" color="text.secondary">
                {label}
              </Typography>
              <Typography
                variant="h6"
                sx={{
                  mt: 0.5,
                  fontWeight: 600,
                  overflowWrap: 'anywhere',
                  fontVariantNumeric: 'tabular-nums',
                }}
              >
                {value}
              </Typography>
            </Box>
          ))}
        </Box>
        {status?.message && (
          <Alert severity="warning" sx={{ mt: 2, overflowWrap: 'anywhere' }}>
            {explain(status.message)}
          </Alert>
        )}
      </Box>

      <Box
        sx={{
          display: 'grid',
          gridTemplateColumns: {
            xs: 'minmax(0, 1fr)',
            md: 'repeat(2, minmax(0, 1fr))',
          },
          alignItems: 'start',
          gap: 2,
        }}
      >
        <RecoveryCard
          title={t('settings.autoFailover.websites')}
          icon={<LanguageRounded fontSize="small" />}
        >
          <Stack spacing={1.5} aria-live="polite">
            {websites.length === 0 ? (
              <Typography variant="body2" color="text.secondary">
                {t('settings.autoFailover.overview.noWebsiteChecks')}
              </Typography>
            ) : (
              websites.map((site) => (
                <Box
                  key={site.url}
                  sx={{
                    border: 1,
                    borderColor: 'divider',
                    borderRadius: 1.5,
                    p: 1.5,
                    bgcolor: alpha(theme.palette.text.primary, 0.025),
                    overflowWrap: 'anywhere',
                  }}
                >
                  <Typography variant="body2" sx={{ fontWeight: 500 }}>
                    {site.url}
                  </Typography>
                  <Typography
                    variant="body2"
                    sx={{
                      mt: 0.75,
                      color: site.reachable
                        ? 'success.main'
                        : site.verification_required
                          ? 'warning.main'
                          : 'error.main',
                    }}
                  >
                    {site.reachable
                      ? site.status_code === 401
                        ? t('settings.autoFailover.networkReachable')
                        : t('settings.autoFailover.reachable')
                      : site.verification_required
                        ? t('settings.autoFailover.verificationRequired')
                        : site.status_code != null
                          ? t('settings.autoFailover.pageFailed')
                          : t('settings.autoFailover.requestFailed')}
                  </Typography>
                  <Stack
                    direction="row"
                    sx={{
                      justifyContent: 'space-between',
                      gap: 1,
                      flexWrap: 'wrap',
                      mt: 0.75,
                    }}
                  >
                    <Typography variant="caption" color="text.secondary">
                      {site.status_code != null &&
                        `HTTP ${site.status_code} · `}
                      {site.elapsed_ms} ms
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      {site.node}
                    </Typography>
                  </Stack>
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    component="p"
                  >
                    {t('settings.autoFailover.lastCheck')}:{' '}
                    {formatTime(site.checked_at)}
                  </Typography>
                  {site.error && !site.verification_required && (
                    <Typography
                      variant="caption"
                      color="error"
                      component="p"
                      sx={{ mt: 0.5 }}
                    >
                      {explain(site.error)}
                    </Typography>
                  )}
                </Box>
              ))
            )}
            <Typography variant="caption" color="text.secondary">
              {t('settings.autoFailover.probeHint')}
            </Typography>
          </Stack>
        </RecoveryCard>
        <RecoveryCard
          title={t('settings.autoFailover.overview.nodeProbes')}
          icon={<SpeedRounded fontSize="small" />}
          action={
            <Button
              size="small"
              variant="outlined"
              startIcon={<RefreshRounded />}
              loading={Boolean(busy)}
              disabled={
                !enabled || !verge || toggling || configSaving || Boolean(busy)
              }
              onClick={check}
            >
              {t('settings.autoFailover.checkNow')}
            </Button>
          }
        >
          {status?.measuring && (
            <Typography
              variant="caption"
              color="text.secondary"
              component="p"
              sx={{ mb: 1 }}
            >
              {t('settings.autoFailover.measuring')}
            </Typography>
          )}
          {nodes.length === 0 ? (
            <Typography variant="body2" color="text.secondary">
              {t('settings.autoFailover.overview.noNodeChecks')}
            </Typography>
          ) : (
            <TableContainer
              tabIndex={0}
              aria-label={t('settings.autoFailover.overview.nodeProbes')}
              sx={{ maxHeight: 300 }}
            >
              <Table
                size="small"
                stickyHeader
                aria-label={t('settings.autoFailover.overview.nodeProbes')}
                sx={{ tableLayout: 'fixed' }}
              >
                <TableHead>
                  <TableRow>
                    <TableCell sx={{ pl: 0 }}>
                      {t('settings.autoFailover.overview.node')}
                    </TableCell>
                    <TableCell align="right" sx={{ pr: 0, width: '35%' }}>
                      {t('settings.autoFailover.overview.latency')}
                    </TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {nodes.map((node) => (
                    <TableRow
                      key={node.name}
                      sx={{ '&:last-child td': { borderBottom: 0 } }}
                    >
                      <TableCell sx={{ pl: 0, overflowWrap: 'anywhere' }}>
                        <Stack
                          direction="row"
                          sx={{
                            alignItems: 'center',
                            gap: 0.75,
                            flexWrap: 'wrap',
                          }}
                        >
                          <Typography variant="body2">{node.name}</Typography>
                          {node.name === status?.current_node && (
                            <Chip
                              size="small"
                              color="primary"
                              variant="outlined"
                              label={t(
                                'settings.autoFailover.overview.current',
                              )}
                              sx={{ height: 21, fontSize: 11 }}
                            />
                          )}
                        </Stack>
                        <Typography variant="caption" color="text.secondary">
                          {formatTime(node.checked_at)}
                        </Typography>
                        {node.error && (
                          <Typography
                            variant="caption"
                            color="error"
                            component="p"
                          >
                            {explain(node.error)}
                          </Typography>
                        )}
                      </TableCell>
                      <TableCell
                        align="right"
                        sx={{
                          pr: 0,
                          color:
                            node.delay_ms == null
                              ? 'warning.main'
                              : 'text.primary',
                          overflowWrap: 'anywhere',
                          fontVariantNumeric: 'tabular-nums',
                        }}
                      >
                        {node.delay_ms == null
                          ? t('settings.autoFailover.unreachable')
                          : `${node.delay_ms} ms`}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          )}
        </RecoveryCard>
      </Box>

      <RecoveryCard
        title={t('settings.autoFailover.history.title')}
        icon={<HistoryRounded fontSize="small" />}
        action={
          <Typography variant="caption" color="text.secondary">
            {t('settings.autoFailover.overview.historyCount', {
              count: switchHistory.length,
            })}
          </Typography>
        }
      >
        {status?.switch_history_error && (
          <Alert severity="warning" sx={{ mb: 1.5 }}>
            {t('settings.autoFailover.history.storageError')}
          </Alert>
        )}
        {switchHistory.length === 0 ? (
          <Typography variant="body2" color="text.secondary">
            {t('settings.autoFailover.history.empty')}
          </Typography>
        ) : (
          <Stack
            component="ol"
            tabIndex={0}
            aria-label={t('settings.autoFailover.history.title')}
            spacing={2}
            sx={{
              m: 0,
              p: 0,
              listStyle: 'none',
              maxHeight: 320,
              overflow: 'auto',
            }}
          >
            {switchHistory.map((entry) => (
              <Box
                component="li"
                key={entry.id}
                sx={{
                  display: 'grid',
                  gridTemplateColumns: {
                    xs: 'minmax(0, 1fr)',
                    sm: '150px minmax(0, 1fr) auto',
                  },
                  gap: { xs: 0.75, sm: 2 },
                  alignItems: 'center',
                }}
              >
                <Typography
                  component="time"
                  dateTime={new Date(entry.switched_at).toISOString()}
                  variant="caption"
                  color="text.secondary"
                  sx={{ fontVariantNumeric: 'tabular-nums' }}
                >
                  {new Date(entry.switched_at).toLocaleString()}
                </Typography>
                <Stack
                  direction="row"
                  sx={{
                    alignItems: 'center',
                    gap: 1,
                    flexWrap: 'wrap',
                    minWidth: 0,
                    overflowWrap: 'anywhere',
                  }}
                >
                  <Typography variant="body2">
                    {entry.from_node ??
                      t('settings.autoFailover.history.unknown')}
                  </Typography>
                  <ArrowForwardRounded
                    fontSize="small"
                    sx={{ color: 'text.secondary' }}
                    titleAccess={t('settings.autoFailover.history.to')}
                  />
                  <Typography variant="body2" sx={{ fontWeight: 500 }}>
                    {entry.to_node}
                  </Typography>
                </Stack>
                <Chip
                  size="small"
                  label={switchReasons[entry.reason]}
                  variant="outlined"
                  sx={{ justifySelf: 'start', fontSize: 12 }}
                />
              </Box>
            ))}
          </Stack>
        )}
        <Typography
          variant="caption"
          color="text.secondary"
          component="p"
          sx={{ mt: 1.5 }}
        >
          {t('settings.autoFailover.history.description')}
        </Typography>
      </RecoveryCard>

      <Box component="section" sx={{ ...panelSx, p: 2 }}>
        <Stack
          direction="row"
          sx={{
            alignItems: 'center',
            justifyContent: 'space-between',
            flexWrap: 'wrap',
            gap: 2,
          }}
        >
          <Stack
            direction="row"
            sx={{
              alignItems: 'center',
              gap: 1.5,
              minWidth: 0,
              flex: '1 1 260px',
            }}
          >
            <Box
              sx={{
                color: 'primary.main',
                bgcolor: alpha(theme.palette.primary.main, 0.1),
                p: 1,
                display: 'flex',
                borderRadius: 1.5,
              }}
            >
              <SettingsOutlined />
            </Box>
            <Box sx={{ minWidth: 0 }}>
              <Typography component="h2" variant="subtitle2">
                {t('settings.autoFailover.overview.strategy')}
              </Typography>
              <Typography
                variant="body2"
                color="text.secondary"
                sx={{ mt: 0.5 }}
              >
                {t('settings.autoFailover.description')}
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {t('settings.autoFailover.overview.concurrency', {
                  count: config.concurrency,
                })}
              </Typography>
            </Box>
          </Stack>
          <Button
            variant="outlined"
            size="small"
            endIcon={<ChevronRightRounded />}
            onClick={() => setConfigOpen(true)}
            disabled={!verge || toggling}
          >
            {t('settings.autoFailover.overview.editStrategy')}
          </Button>
        </Stack>
      </Box>
      <Typography
        variant="caption"
        color="text.secondary"
        sx={{ overflowWrap: 'anywhere' }}
      >
        {t('settings.autoFailover.overview.subscription')}:{' '}
        {subscriptionName(config.subscription_url)} ·{' '}
        {t('settings.autoFailover.subscriptionUpdated')}:{' '}
        {formatTime(status?.subscription_updated_at)}
      </Typography>
      <AutoFailoverConfigDialog
        open={configOpen}
        onClose={() => setConfigOpen(false)}
        onSavingChange={setConfigSaving}
      />
    </Stack>
  )
}
