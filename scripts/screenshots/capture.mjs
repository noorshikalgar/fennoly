// Takes the README screenshots from a server filled by seed_demo.py.
//   node capture.mjs <base-url> <demo.json> <out-dir>
// Needs Playwright with Chromium (npm i playwright && npx playwright install chromium);
// run it from a folder where `playwright` is installed. finish.py then turns
// the PNGs into the WebP files in docs/screenshots.
import { chromium } from 'playwright';
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const [, , BASE = 'http://127.0.0.1:8695', DEMO = 'demo.json', OUT = 'shots'] = process.argv;
const demo = JSON.parse(readFileSync(DEMO, 'utf8'));
mkdirSync(OUT, { recursive: true });

// CHROMIUM=<path to chrome> uses an already installed browser build.
const browser = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {});

async function session(options) {
	const context = await browser.newContext({ locale: 'en-IN', colorScheme: 'light', ...options });
	if (options.login !== false) {
		const res = await context.request.post(`${BASE}/api/auth/login`, { data: { username: demo.user, password: demo.password } });
		if (!res.ok()) throw new Error(`login failed: ${res.status()}`);
	}
	const page = await context.newPage();
	return { context, page };
}

/** Sets theme and language for the demo user, then opens `path`. */
async function show(page, path, { theme = 'light', locale = 'en' } = {}) {
	await page.request.patch(`${BASE}/api/me`, { data: { theme, locale } });
	await page.goto(`${BASE}${path}`);
	await page.waitForLoadState('networkidle');
	// Thumbnails fade in; give transitions a moment.
	await page.waitForTimeout(900);
}

async function shot(page, name) {
	await page.mouse.move(0, 0);
	await page.screenshot({ path: join(OUT, `${name}.png`) });
	console.log('saved', name);
}

async function menu(page, itemName, entry) {
	await page.getByText(itemName, { exact: true }).first().click({ button: 'right' });
	await page.getByRole('menuitem', { name: entry, exact: true }).click();
	await page.waitForTimeout(700);
}

// ---- desktop ----------------------------------------------------------------
{
	const { context, page } = await session({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 2 });

	await show(page, '/');
	await shot(page, 'home');

	await show(page, `/files/${demo.goa}`, { theme: 'dark' });
	await shot(page, 'photos-dark');

	await page.getByText('Beach sunset.jpg', { exact: true }).first().dblclick();
	await page.waitForTimeout(1200);
	await shot(page, 'viewer');
	await page.keyboard.press('Escape');

	await show(page, `/files/${demo.home}`);
	await shot(page, 'my-files');
	await menu(page, 'Insurance', 'Share');
	await shot(page, 'share');
	await page.keyboard.press('Escape');

	await show(page, `/files/${demo.goa}`);
	await menu(page, 'Hill view.jpg', 'Compress');
	await page.waitForTimeout(3500);
	await shot(page, 'compress');
	await page.keyboard.press('Escape');

	await show(page, `/files/${demo.home}`);
	await menu(page, 'Goa trip 2025', 'Share by link');
	// Show a real-looking address instead of the local test server's.
	await page.evaluate((origin) => {
		for (const input of document.querySelectorAll('[role=dialog] input')) {
			if (input.value.startsWith(origin)) input.value = input.value.replace(origin, 'https://drive.example.com');
		}
	}, BASE);
	await shot(page, 'link');
	await page.keyboard.press('Escape');

	await show(page, `/files/${demo.family}`, { theme: 'saffron' });
	await shot(page, 'family-saffron');

	await show(page, '/shared');
	await shot(page, 'shared');

	await show(page, '/admin');
	await shot(page, 'members');

	await show(page, '/admin/backups');
	await shot(page, 'backups');

	await show(page, '/', { locale: 'mr', theme: 'light' });
	await shot(page, 'home-marathi');

	await show(page, '/', { locale: 'en', theme: 'light' });
	await context.close();
}

// ---- public link (a visitor without an account) -------------------------------
{
	const { context, page } = await session({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 2, login: false });
	await page.goto(`${BASE}/s/${demo.link}`);
	await page.waitForLoadState('networkidle');
	await page.waitForTimeout(900);
	await shot(page, 'public-link');
	await context.close();
}

// ---- phone ---------------------------------------------------------------------
{
	const { context, page } = await session({
		viewport: { width: 390, height: 844 },
		deviceScaleFactor: 3,
		isMobile: true,
		hasTouch: true
	});
	await show(page, '/', { theme: 'light', locale: 'en' });
	await shot(page, 'phone-home');
	await show(page, `/files/${demo.goa}`, { theme: 'dark', locale: 'hi' });
	await shot(page, 'phone-photos-hindi');
	await show(page, `/files/${demo.family}`, { theme: 'saffron', locale: 'mr' });
	await shot(page, 'phone-family-marathi');
	await show(page, '/', { theme: 'light', locale: 'en' });
	await context.close();
}

await browser.close();
