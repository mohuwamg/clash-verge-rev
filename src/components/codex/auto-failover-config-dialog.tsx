import {
  Alert,
  FormControlLabel,
  Stack,
  Switch,
  TextField,
} from '@mui/material'
import { useLockFn } from 'ahooks'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BaseDialog } from '@/components/base'
import { useVerge } from '@/hooks/use-verge'
import { setAutoFailoverConfig } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import {
  defaultAutoFailoverConfig,
  type AutoFailoverConfig,
} from '@/types/auto-failover'

interface Props {
  open: boolean
  onClose: () => void
  onSavingChange?: (saving: boolean) => void
}

interface EditorProps extends Props {
  saving: boolean
  onSavingChange: (saving: boolean) => void
}

const lines = (text: string) => [
  ...new Set(
    text
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean),
  ),
]

function AutoFailoverConfigEditor({
  open,
  onClose,
  saving,
  onSavingChange,
}: EditorProps) {
  const { t } = useTranslation()
  const { verge, mutateVerge } = useVerge()
  const [values, setValues] = useState<AutoFailoverConfig>(() => ({
    ...(verge?.auto_failover ?? defaultAutoFailoverConfig),
  }))
  const [websites, setWebsites] = useState(() => values.websites.join('\n'))
  const [delayUrls, setDelayUrls] = useState(() => values.delay_urls.join('\n'))

  const save = useLockFn(async () => {
    onSavingChange(true)
    try {
      const submittedConfig = {
        ...values,
        subscription_url: values.subscription_url.trim(),
        websites: lines(websites),
        delay_urls: lines(delayUrls),
      }
      await setAutoFailoverConfig(submittedConfig)
      mutateVerge((prev) =>
        prev ? { ...prev, auto_failover: submittedConfig } : prev,
      )
      mutateVerge()
      onClose()
    } catch (error) {
      showNotice.error(error)
    } finally {
      onSavingChange(false)
    }
  })

  return (
    <BaseDialog
      open={open}
      loading={saving}
      title={t('settings.autoFailover.title')}
      contentSx={{ width: 520, maxWidth: '100%' }}
      okBtn={
        saving ? t('settings.autoFailover.saving') : t('shared.actions.save')
      }
      cancelBtn={
        saving ? t('shared.actions.close') : t('shared.actions.cancel')
      }
      onClose={onClose}
      onCancel={onClose}
      onOk={save}
    >
      <Stack spacing={2} sx={{ py: 1 }}>
        <Alert severity="info">{t('settings.autoFailover.enableHint')}</Alert>
        <FormControlLabel
          label={t('settings.autoFailover.enabled')}
          control={
            <Switch
              disabled={saving}
              checked={values.enabled}
              onChange={(_, enabled) =>
                setValues((prev) => ({ ...prev, enabled }))
              }
            />
          }
        />
        <TextField
          disabled={saving}
          required={values.enabled}
          error={values.enabled && !values.subscription_url.trim()}
          label={t('settings.autoFailover.subscriptionUrl')}
          value={values.subscription_url}
          onChange={(event) =>
            setValues((prev) => ({
              ...prev,
              subscription_url: event.target.value,
            }))
          }
        />
        <TextField
          disabled={saving}
          label={t('settings.autoFailover.websites')}
          helperText={t('settings.autoFailover.onePerLine')}
          multiline
          minRows={2}
          value={websites}
          onChange={(event) => setWebsites(event.target.value)}
        />
        <TextField
          disabled={saving}
          label={t('settings.autoFailover.delayUrls')}
          helperText={t('settings.autoFailover.delayUrlsHint')}
          multiline
          minRows={2}
          value={delayUrls}
          onChange={(event) => setDelayUrls(event.target.value)}
        />
        <TextField
          disabled={saving}
          type="number"
          label={t('settings.autoFailover.delayTimeout')}
          value={values.delay_timeout_ms}
          onChange={(event) =>
            setValues((prev) => ({
              ...prev,
              delay_timeout_ms: Number(event.target.value),
            }))
          }
        />
        <TextField
          disabled={saving}
          type="number"
          label={t('settings.autoFailover.websiteTimeout')}
          value={values.website_timeout_ms}
          onChange={(event) =>
            setValues((prev) => ({
              ...prev,
              website_timeout_ms: Number(event.target.value),
            }))
          }
        />
        <TextField
          disabled={saving}
          type="number"
          label={t('settings.autoFailover.concurrency')}
          value={values.concurrency}
          onChange={(event) =>
            setValues((prev) => ({
              ...prev,
              concurrency: Number(event.target.value),
            }))
          }
        />
      </Stack>
    </BaseDialog>
  )
}

export default function AutoFailoverConfigDialog({
  open,
  onClose,
  onSavingChange,
}: Props) {
  const [saving, setSaving] = useState(false)

  // Keep the editor and its save lock alive when a pending save is hidden.
  // Otherwise, mounting a new editor snapshots the latest saved configuration.
  return open || saving ? (
    <AutoFailoverConfigEditor
      open={open}
      onClose={onClose}
      saving={saving}
      onSavingChange={(pending) => {
        setSaving(pending)
        onSavingChange?.(pending)
      }}
    />
  ) : null
}
