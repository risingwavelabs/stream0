import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

const backendUrl = process.env.B0_FRONTEND_BACKEND_URL ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [vue()],
  server: {
    proxy: {
      '/workspaces': {
        target: backendUrl,
        changeOrigin: true,
      },
      '/machines': {
        target: backendUrl,
        changeOrigin: true,
      },
      '/users': {
        target: backendUrl,
        changeOrigin: true,
      },
    },
  },
})
