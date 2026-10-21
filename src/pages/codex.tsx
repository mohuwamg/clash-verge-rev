import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import AutoFailoverOverview from '@/components/codex/auto-failover-overview'

export default function CodexPage() {
  const { t } = useTranslation()

  return (
    <BasePage title={t('layout.components.navigation.tabs.codex')}>
      <AutoFailoverOverview />
    </BasePage>
  )
}
