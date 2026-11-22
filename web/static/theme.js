// Apply the saved theme before first paint, so the page never flashes.
// A file of its own (not inline) so the Content-Security-Policy can forbid
// inline scripts.
(function () {
	try {
		var t = localStorage.getItem('fennoly-theme') || 'system';
		if (t === 'system') t = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
		document.documentElement.dataset.theme = t;
		var l = localStorage.getItem('fennoly-locale');
		if (l) document.documentElement.lang = l;
	} catch (e) {}
})();
