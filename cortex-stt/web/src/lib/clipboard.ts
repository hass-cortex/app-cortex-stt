/**
 * Copy text to the clipboard.
 *
 * The async Clipboard API requires a secure context (HTTPS or localhost), so
 * when the admin UI is opened over plain HTTP on a LAN IP (the typical HA
 * ingress / direct-port scenario) `navigator.clipboard` is undefined and the
 * call throws. Fall back to the legacy `document.execCommand('copy')` path
 * via a hidden textarea, which works in any context.
 */
export async function copyToClipboard(text: string): Promise<void> {
	if (typeof navigator !== "undefined" && navigator.clipboard && window.isSecureContext) {
		await navigator.clipboard.writeText(text);
		return;
	}

	const textarea = document.createElement("textarea");
	textarea.value = text;
	textarea.setAttribute("readonly", "");
	textarea.style.position = "fixed";
	textarea.style.top = "0";
	textarea.style.left = "0";
	textarea.style.opacity = "0";
	textarea.style.pointerEvents = "none";
	document.body.appendChild(textarea);

	const selection = document.getSelection();
	const previousRange = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;

	textarea.focus();
	textarea.select();
	textarea.setSelectionRange(0, textarea.value.length);

	try {
		const ok = document.execCommand("copy");
		if (!ok) throw new Error("execCommand('copy') returned false");
	} finally {
		document.body.removeChild(textarea);
		if (previousRange && selection) {
			selection.removeAllRanges();
			selection.addRange(previousRange);
		}
	}
}
