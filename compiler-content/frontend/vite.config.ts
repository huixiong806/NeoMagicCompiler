import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    lib: {
      entry: 'src/index.ts',
      formats: ['iife'],
      name: 'RiscvPlayground',
      cssFileName: 'riscv-playground',
    },
    outDir: '../theme/dist',
    emptyOutDir: true,
    sourcemap: true,
    minify: false,
    rollupOptions: {
      output: {
        entryFileNames: 'riscv-playground.js',
      },
    },
  },
  server: {
    port: 5173,
    open: '/index.html',
  },
});
