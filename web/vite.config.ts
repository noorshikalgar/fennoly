import adapter from '@sveltejs/adapter-static';
import tailwindcss from '@tailwindcss/vite';
import { withPwa } from '@vite-pwa/sveltekit';
import { defineConfig } from 'vite';

// The API server (cargo run -p fennoly-server) listens on 8686 by default.
const api = process.env.FENNOLY_API ?? 'http://127.0.0.1:8686';

export default defineConfig({
	plugins: [
		tailwindcss(),
		...withPwa(
			{
				// Single-page app: the Rust server returns index.html for unknown paths.
				adapter: adapter({ fallback: 'index.html' }),
				alias: { $lib: 'src/lib' }
			},
			{
				registerType: 'autoUpdate',
				injectRegister: 'auto',
				manifest: {
					name: 'Fennoly',
					short_name: 'Fennoly',
					description: 'Your family drive',
					start_url: '/',
					scope: '/',
					display: 'standalone',
					background_color: '#f4f7f5',
					theme_color: '#0f6e56',
					icons: [
						{ src: '/icons/icon-192.png', sizes: '192x192', type: 'image/png' },
						{ src: '/icons/icon-512.png', sizes: '512x512', type: 'image/png' },
						{ src: '/icons/maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' }
					]
				},
				kit: { adapterFallback: 'index.html', spa: true },
				generateSW: {
					globPatterns: ['client/**/*.{js,css,ico,png,svg,webmanifest,woff,woff2}'],
					// There are no prerendered pages in this single-page app.
					globStrict: false,
					navigateFallbackDenylist: [/^\/api\//]
				},
				devOptions: { enabled: false }
			}
		)
	],
	server: {
		port: 5173,
		strictPort: true,
		proxy: { '/api': { target: api, changeOrigin: false } }
	}
});
