import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import VueI18nPlugin from '@intlify/unplugin-vue-i18n/vite'
import { resolve } from 'path'

export default defineConfig({
  plugins: [
    vue(),
    VueI18nPlugin({
      include: resolve(__dirname, './src/locales/*.json'),
    }),
  ],
  server: {
    proxy: {
      '/api': 'http://localhost:7700',
      '/ws': {
        target: 'ws://localhost:7700',
        ws: true,
      },
    },
  },
})
