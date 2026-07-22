import { createI18n } from 'vue-i18n'
import enUS from './en-US.json'
import zhCN from './zh-CN.json'

const I18N_KEY = 'tailr-locale'

export type MessageSchema = typeof enUS

const SUPPORTED_LOCALES: Record<string, typeof enUS> = {
  'en-US': enUS,
  'zh-CN': zhCN,
}

function getInitialLocale(): string {
  const saved = localStorage.getItem(I18N_KEY)
  if (saved && saved in SUPPORTED_LOCALES) return saved
  // Default to English for new users. Users can switch to zh-CN in Settings,
  // and their choice is persisted across sessions.
  return 'en-US'
}

const i18n = createI18n({
  legacy: false,
  locale: getInitialLocale(),
  fallbackLocale: 'en-US',
  messages: SUPPORTED_LOCALES,
})

export async function loadLocale(locale: string): Promise<void> {
  if (!(locale in SUPPORTED_LOCALES)) {
    console.warn(`Unsupported locale: ${locale}`)
    return
  }
  ;(i18n.global.locale as unknown as { value: string }).value = locale
  localStorage.setItem(I18N_KEY, locale)
}

export default i18n
