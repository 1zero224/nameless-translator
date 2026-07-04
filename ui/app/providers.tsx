'use client'

import { QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { useEffect, type ReactNode } from 'react'
import { I18nextProvider } from 'react-i18next'

import ClientOnly from '@/components/ClientOnly'
import { TooltipProvider } from '@/components/ui/tooltip'
import { UpdaterProvider } from '@/components/Updater'
import { useListOperations } from '@/lib/api/default/default'
import { connectEvents } from '@/lib/events'
import i18n from '@/lib/i18n'
import { queryClient } from '@/lib/queryClient'
import { useJobsStore } from '@/lib/stores/jobsStore'

export function Providers({ children }: { children: ReactNode }) {
  useEffect(() => {
    const onLang = (lng: string) => {
      document.documentElement.lang = lng
    }
    onLang(i18n.language)
    i18n.on('languageChanged', onLang)
    return () => {
      i18n.off('languageChanged', onLang)
    }
  }, [])

  useEffect(() => connectEvents(), [])

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider attribute='class' defaultTheme='system' enableSystem>
        <ClientOnly>
          <I18nextProvider i18n={i18n}>
            <TooltipProvider delayDuration={0}>
              <UpdaterProvider>
                <OperationsSnapshotSync />
                {children}
              </UpdaterProvider>
            </TooltipProvider>
          </I18nextProvider>
        </ClientOnly>
      </ThemeProvider>
    </QueryClientProvider>
  )
}

export function OperationsSnapshotSync() {
  const hasRunningOperation = useJobsStore((s) =>
    Object.values(s.jobs).some((job) => job.status === 'running'),
  )
  const { data } = useListOperations({
    query: {
      refetchInterval: hasRunningOperation ? 1000 : false,
      refetchOnMount: 'always',
      refetchOnWindowFocus: true,
    },
  })

  useEffect(() => {
    if (!data) return
    useJobsStore.getState().setSnapshot(data.operations)
  }, [data])

  return null
}

export default Providers
