import type { Locale } from '$lib/types';
import { en, type MessageKey } from './en';
import { hi } from './hi';
import { mr } from './mr';

const dictionaries: Record<Locale, Record<MessageKey, string>> = { en, hi, mr };

export const LANGUAGES: { code: Locale; label: string }[] = [
	{ code: 'en', label: 'English' },
	{ code: 'hi', label: 'हिंदी' },
	{ code: 'mr', label: 'मराठी' }
];

/** BCP 47 tags for number and date formatting (Indian conventions). */
export const INTL_LOCALE: Record<Locale, string> = { en: 'en-IN', hi: 'hi-IN', mr: 'mr-IN' };

function initialLocale(): Locale {
	try {
		const saved = localStorage.getItem('fennoly-locale') as Locale | null;
		if (saved && saved in dictionaries) return saved;
	} catch {
		/* storage unavailable */
	}
	const nav = (navigator.language || 'en').slice(0, 2) as Locale;
	return nav in dictionaries ? nav : 'en';
}

export const i18n = $state({ locale: initialLocale() });

export function setLocale(locale: Locale) {
	i18n.locale = locale;
	document.documentElement.lang = locale;
	try {
		localStorage.setItem('fennoly-locale', locale);
	} catch {
		/* ignore */
	}
}

/** Translates a key, filling `{placeholders}`. Reactive: re-renders on language change. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
	let text = dictionaries[i18n.locale]?.[key] ?? en[key] ?? key;
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			text = text.replaceAll(`{${k}}`, String(v));
		}
	}
	return text;
}

export type { MessageKey };
