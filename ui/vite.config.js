import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Weekling ships as a single binary: the SvelteKit app builds to ../web as a
// static SPA, which rust-embed bakes into the server. See ui/README.md.
export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({
				pages: '../web',
				assets: '../web',
				fallback: 'index.html',
				precompress: false,
				strict: false
			})
		})
	]
});
