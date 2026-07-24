import { defineConfig } from 'vite'
import wasm from 'vite-plugin-wasm'

export default defineConfig({
    base: '/markdown-it-rs/',
    build: {
        outDir: 'build',
    },
    plugins: [ wasm() ],
})
