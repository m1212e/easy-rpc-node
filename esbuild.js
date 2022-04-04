const esbuild = require('esbuild');

esbuild.buildSync({
        entryPoints: ['main.ts'],
        outdir: 'dist',
        bundle: true,
        sourcemap: true,
        minify: true,
        platform: 'node',
        target: ['node16.14'],
        });