// Installing Pennoly as an app. Chrome, Edge and Samsung Internet offer an
// install prompt the page can show on a button; Safari on iPhone and iPad
// has none, so there the steps are explained instead.

interface InstallPromptEvent extends Event {
	prompt(): Promise<void>;
	userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

const isBrowser = typeof window !== 'undefined';

function standalone(): boolean {
	if (!isBrowser) return false;
	return matchMedia('(display-mode: standalone)').matches || (navigator as Navigator & { standalone?: boolean }).standalone === true;
}

export const install = $state({
	/** Running as the installed app. */
	installed: standalone(),
	/** The browser has an install prompt ready. */
	canPrompt: false,
	/** Safari on iPhone or iPad (Add to Home Screen by hand). */
	ios: isBrowser && /iphone|ipad|ipod/i.test(navigator.userAgent) && !/crios|fxios|edgios/i.test(navigator.userAgent),
	/** Plain http on a network address: browsers only install over https. */
	insecure: isBrowser && !isSecureContext
});

let deferred: InstallPromptEvent | null = null;

/** Call once, early: the browser fires its event shortly after the page loads. */
export function watchInstall() {
	if (!isBrowser) return;
	window.addEventListener('beforeinstallprompt', (e) => {
		e.preventDefault();
		deferred = e as InstallPromptEvent;
		install.canPrompt = true;
	});
	window.addEventListener('appinstalled', () => {
		install.installed = true;
		install.canPrompt = false;
		deferred = null;
	});
	matchMedia('(display-mode: standalone)').addEventListener('change', () => (install.installed = standalone()));
}

/** Shows the browser's install prompt. True when the person installed it. */
export async function promptInstall(): Promise<boolean> {
	if (!deferred) return false;
	await deferred.prompt();
	const { outcome } = await deferred.userChoice;
	deferred = null;
	install.canPrompt = false;
	if (outcome === 'accepted') install.installed = true;
	return outcome === 'accepted';
}
