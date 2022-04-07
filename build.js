const esbuild = require('esbuild');
const fs = require('fs');

esbuild.buildSync({
        entryPoints: ['./main.ts'],
        outdir: 'dist',
        bundle: true,
        sourcemap: true,
        minify: true,
        platform: 'node',
        target: ['node16.14'],
});


fs.copyFileSync('package.json', 'dist/package.json');
fs.copyFileSync('LICENSE', 'dist/LICENSE');