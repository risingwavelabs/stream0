import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import { defineConfig } from 'vite'
import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  envPrefix: ['VITE_', 'NEXT_PUBLIC_'],
  server: {
    port: 3000,
    proxy: {
      '/workspaces': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/machines': { target: 'http://127.0.0.1:8080', changeOrigin: true },
      '/users': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
  resolve: {
    tsconfigPaths: true,
  },
  plugins: [tailwindcss(), tanstackStart(), viteReact()],
})
